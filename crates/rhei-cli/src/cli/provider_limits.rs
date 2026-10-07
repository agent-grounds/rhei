// Recognition, persistence, and scheduling facts for provider-limit waits.
//
// This is one shared boundary because a provider refusal must mean the same
// thing in sequential and parallel completion, and every scheduler surface
// must read the same durable record.

// §FS-rhei-agents.2.3 §FS-rhei-run.3.3 §FS-rhei-run.5.1

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat};
use chrono::{TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;

/// The `metadata.tasks.<id>.providerLimits` key, named by the register of the
/// keys rhei writes. §FS-rhei-transitions.2.5 §FS-rhei-run.3.3
const PROVIDER_LIMITS_KEY: &str = rhei_core::metadata::PROVIDER_LIMITS_KEY;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderIdentity {
    agent: String,
    provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderLimit {
    identity: ProviderIdentity,
    signal: String,
    observed_at: String,
    next_attempt_at: String,
}

impl ProviderLimit {
    fn deadline_epoch(&self) -> Option<u64> {
        DateTime::parse_from_rfc3339(&self.next_attempt_at).ok()?.timestamp().try_into().ok()
    }
}

fn resolved_provider_identity(resolved: &ResolvedAgent) -> Option<ProviderIdentity> {
    Some(ProviderIdentity {
        agent: resolved.agent.id().to_string(),
        provider: resolved.model_provider.clone()?,
    })
}

/// Strip ANSI CSI/OSC decoration without changing any other text. The caller
/// trims surrounding whitespace only after this operation. §FS-rhei-agents.2.3
fn strip_terminal_decoration(line: &str) -> std::borrow::Cow<'_, str> {
    static ANSI: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    ANSI.get_or_init(|| {
        Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*?(?:\x07|\x1b\\))")
            .expect("ANSI decoration regex is valid")
    })
    .replace_all(line, "")
}

/// The closed set of providers whose limit line names a reset instant worth
/// sleeping on, for either supported refusal grammar. A
/// provider joins it by a change to the specification, not by a project's
/// configuration, and the agent registry id is never tested.
/// §FS-rhei-agents.2.3
const RECOGNIZED_PROVIDERS: [&str; 2] = ["openai", "anthropic"];

/// The time-of-day grammar has a closed period alternation and its minutes are
/// optional: `resets 6am` is the same sentence as `resets 6:00am`.
/// §FS-rhei-agents.2.3
fn provider_signal_regex() -> &'static Regex {
    static SIGNAL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    SIGNAL.get_or_init(|| {
        Regex::new(
            r"^You've hit your (?:session|weekly) limit · resets (?<hour>[1-9]|1[0-2])(?::(?<minute>[0-5][0-9]))?(?<meridiem>am|pm) \((?<zone>[^()\s]+)\)$",
        )
        .expect("provider-limit signal regex is valid")
    })
}

/// The dated Codex sentence is anchored independently of calendar resolution,
/// so an impossible date still counts against the single-signal rule.
/// §FS-rhei-agents.2.3
fn codex_provider_signal_regex() -> &'static Regex {
    static SIGNAL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    SIGNAL.get_or_init(|| {
        Regex::new(
            r"^You've hit your usage limit\. Visit https://chatgpt\.com/codex/settings/usage to purchase more credits or try again at (?<month>Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) (?<day>[1-9]|[12][0-9]|3[01])(?<ordinal>st|nd|rd|th), (?<year>[0-9]{4}) (?<hour>[1-9]|1[0-2]):(?<minute>[0-5][0-9]) (?<meridiem>AM|PM)\.$",
        )
        .expect("Codex provider-limit signal regex is valid")
    })
}

/// Validate the ordinal and calendar without guessing a zone or rolling the
/// printed absolute date forward. §FS-rhei-agents.2.3 §FS-rhei-run.3.3
fn codex_provider_reset_minute(signal: &str) -> Option<NaiveDateTime> {
    let captures = codex_provider_signal_regex().captures(signal)?;
    let months =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let printed_month = captures.name("month")?.as_str();
    let month = months.iter().position(|month| *month == printed_month)?;
    let day = captures.name("day")?.as_str().parse::<u32>().ok()?;
    let ordinal = match (day % 100, day % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    if captures.name("ordinal")?.as_str() != ordinal {
        return None;
    }
    let year = captures.name("year")?.as_str().parse::<i32>().ok()?;
    if year == 0 {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(year, month as u32 + 1, day)?;
    let hour12 = captures.name("hour")?.as_str().parse::<u32>().ok()?;
    let hour = match captures.name("meridiem")?.as_str() {
        "AM" => hour12 % 12,
        "PM" => (hour12 % 12) + 12,
        _ => return None,
    };
    let minute = captures.name("minute")?.as_str().parse::<u32>().ok()?;
    date.and_hms_opt(hour, minute, 0)
}

/// Resolve one local reset minute and its following-minute safe boundary. Both
/// instants must be unique, including when the boundary crosses a DST change.
/// §FS-rhei-run.3.3
fn unique_safe_boundary(
    zone: Tz,
    date: NaiveDate,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    let reported = NaiveDateTime::new(date, NaiveTime::from_hms_opt(hour, minute, 0)?);
    unique_safe_boundary_with_resolver(reported, &|minute| {
        zone.from_local_datetime(minute).single().map(|value| value.with_timezone(&Utc))
    })
}

/// Require unique resolution of both civil minutes through the same resolver.
/// Missing, ambiguous and unavailable resolution all fail closed. §FS-rhei-run.3.3
fn unique_safe_boundary_with_resolver(
    reported: NaiveDateTime,
    resolve: &impl Fn(&NaiveDateTime) -> Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let boundary = reported.checked_add_signed(TimeDelta::minutes(1))?;
    resolve(&reported)?;
    resolve(&boundary)
}

/// Expose Claude stdout result and Codex error.message logical lines.
/// Keep other physical lines and independent signals intact. §FS-rhei-agents.2.3
fn provider_limit_output_lines(
    family: &str,
    captured_lines: &[(rhei_tui::AgentStream, String)],
) -> Vec<String> {
    let mut lines = Vec::new();
    for (stream, raw_line) in captured_lines {
        if *stream == rhei_tui::AgentStream::Stdout {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw_line) {
                let field = match (family, value.get("type").and_then(serde_json::Value::as_str)) {
                    ("claude-code", Some("result")) => Some("result"),
                    ("codex", Some("error")) => Some("message"),
                    _ => None,
                };
                if let Some(field) = field {
                    if let Some(text) = value.get(field).and_then(serde_json::Value::as_str) {
                        lines.extend(text.lines().map(str::to_owned));
                        continue;
                    }
                }
            }
        }
        lines.push(raw_line.clone());
    }
    lines
}

/// Recognize the reset signal of a provider in `RECOGNIZED_PROVIDERS` and turn
/// its local minute into the first safe UTC instant after that minute. Dated
/// Codex signals use Chrono's OS-local resolver and reject non-unique results.
/// §FS-rhei-agents.2.3 §FS-rhei-run.3.3
fn classify_provider_limit(
    resolved: &ResolvedAgent,
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    captured_lines: &[String],
    observed: std::time::SystemTime,
) -> Option<ProviderLimit> {
    classify_provider_limit_with_local_resolver(
        resolved,
        status,
        timed_out,
        interrupted,
        captured_lines,
        observed,
        &|minute| {
            chrono::Local
                .from_local_datetime(minute)
                .single()
                .map(|value| value.with_timezone(&Utc))
        },
    )
}

/// Shared classification with an injectable local resolver. Count both signal
/// grammars before resolving either deadline. §FS-rhei-agents.2.3 §FS-rhei-run.3.3
fn classify_provider_limit_with_local_resolver(
    resolved: &ResolvedAgent,
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    captured_lines: &[String],
    observed: std::time::SystemTime,
    resolve_local: &impl Fn(&NaiveDateTime) -> Option<DateTime<Utc>>,
) -> Option<ProviderLimit> {
    // The identity is recorded whole, but only its provider half is a
    // recognition condition. §FS-rhei-agents.2.3
    let identity = resolved_provider_identity(resolved)?;
    if !RECOGNIZED_PROVIDERS.contains(&identity.provider.as_str())
        || status.success()
        || timed_out
        || interrupted
    {
        return None;
    }

    let matching = captured_lines
        .iter()
        .map(|line| strip_terminal_decoration(line).trim().to_string())
        .filter(|line| {
            provider_signal_regex().is_match(line) || codex_provider_signal_regex().is_match(line)
        })
        .collect::<Vec<_>>();
    let [signal] = matching.as_slice() else { return None };
    let observed_utc: DateTime<Utc> = observed.into();
    let deadline = if provider_signal_regex().is_match(signal) {
        named_provider_deadline(signal, observed_utc)?
    } else {
        unique_safe_boundary_with_resolver(codex_provider_reset_minute(signal)?, resolve_local)?
    };
    if deadline <= observed_utc {
        return None;
    }

    Some(ProviderLimit {
        identity,
        signal: signal.clone(),
        observed_at: observed_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        next_attempt_at: deadline.to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

/// Preserve time-of-day reset resolution on today's or tomorrow's date in the
/// printed IANA zone. Absolute Codex dates never use this rollover. §FS-rhei-run.3.3
fn named_provider_deadline(signal: &str, observed_utc: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let captures = provider_signal_regex().captures(signal)?;

    let hour12 = captures.name("hour")?.as_str().parse::<u32>().ok()?;
    // An absent minute group is that hour at `00`, not a failure to match: the
    // group is optional, so it does not participate in `resets 6am`.
    // §FS-rhei-agents.2.3
    let minute = match captures.name("minute") {
        Some(found) => found.as_str().parse::<u32>().ok()?,
        None => 0,
    };
    let hour = match captures.name("meridiem")?.as_str() {
        "am" => hour12 % 12,
        "pm" => (hour12 % 12) + 12,
        _ => return None,
    };
    let zone = captures.name("zone")?.as_str().parse::<Tz>().ok()?;
    let local_date = observed_utc.with_timezone(&zone).date_naive();
    let mut deadline = unique_safe_boundary(zone, local_date, hour, minute)?;
    if deadline <= observed_utc {
        let next_date = local_date.succ_opt()?;
        deadline = unique_safe_boundary(zone, next_date, hour, minute)?;
        if deadline <= observed_utc {
            return None;
        }
    }

    Some(deadline)
}

fn provider_limit_from_value(value: &YamlValue) -> Option<ProviderLimit> {
    let map = value.as_mapping()?;
    let identity = map.get(yaml_key("identity"))?.as_mapping()?;
    Some(ProviderLimit {
        identity: ProviderIdentity {
            agent: identity.get(yaml_key("agent"))?.as_str()?.to_string(),
            provider: identity.get(yaml_key("provider"))?.as_str()?.to_string(),
        },
        signal: map.get(yaml_key("signal"))?.as_str()?.to_string(),
        observed_at: map.get(yaml_key("observedAt"))?.as_str()?.to_string(),
        next_attempt_at: map.get(yaml_key("nextAttemptAt"))?.as_str()?.to_string(),
    })
}

fn provider_limit_for_task_state(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<ProviderLimit> {
    task_metadata_map(metadata, task_id)
        .and_then(|task| task.get(yaml_key(PROVIDER_LIMITS_KEY)))
        .and_then(YamlValue::as_mapping)
        .and_then(|limits| limits.get(yaml_key(state_name)))
        .and_then(provider_limit_from_value)
}

fn provider_limit_value(limit: &ProviderLimit) -> YamlValue {
    let mut identity = YamlMapping::new();
    identity.insert(yaml_key("agent"), yaml_key(&limit.identity.agent));
    identity.insert(yaml_key("provider"), yaml_key(&limit.identity.provider));
    let mut record = YamlMapping::new();
    record.insert(yaml_key("identity"), YamlValue::Mapping(identity));
    record.insert(yaml_key("signal"), yaml_key(&limit.signal));
    record.insert(yaml_key("observedAt"), yaml_key(&limit.observed_at));
    record.insert(yaml_key("nextAttemptAt"), yaml_key(&limit.next_attempt_at));
    YamlValue::Mapping(record)
}

fn set_provider_limit_metadata(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
    observed: &ProviderLimit,
) -> (Metadata, ProviderLimit) {
    let current = provider_limit_for_task_state(existing, task_id, state_name)
        .filter(|current| current.identity == observed.identity)
        .filter(|current| {
            matches!(
                (current.deadline_epoch(), observed.deadline_epoch()),
                (Some(current), Some(observed)) if current >= observed
            )
        });
    let effective = current.unwrap_or_else(|| observed.clone());
    let mut root = existing.cloned().unwrap_or_default();
    let metadata = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata, yaml_key("tasks"));
    let task = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let limits = ensure_mapping(task, yaml_key(PROVIDER_LIMITS_KEY));
    limits.insert(yaml_key(state_name), provider_limit_value(&effective));
    (root, effective)
}

/// Remove one state's provider record, pruning empty runtime containers.
/// §FS-rhei-run.3.3
fn clear_provider_limit_state_metadata(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(task)) = tasks.get_mut(task_id_yaml_key(task_id)) else {
        return Some(root);
    };
    if let Some(YamlValue::Mapping(limits)) = task.get_mut(yaml_key(PROVIDER_LIMITS_KEY)) {
        limits.remove(yaml_key(state_name));
    }
    if task
        .get(yaml_key(PROVIDER_LIMITS_KEY))
        .and_then(YamlValue::as_mapping)
        .is_some_and(YamlMapping::is_empty)
    {
        task.remove(yaml_key(PROVIDER_LIMITS_KEY));
    }
    Some(drop_empty_task_metadata(root))
}

/// Persist a reporting task's wait under its owning rhei's metadata lock. A
/// repeated signal can extend, but never shorten, the wait. §FS-rhei-run.3.3
fn persist_provider_limit(
    input: &Path,
    loaded: &LoadedPlan,
    task_id: &str,
    state_name: &str,
    observed: &ProviderLimit,
) -> MietteResult<ProviderLimit> {
    let route = loaded.task_route(task_id, input);
    let metadata_id = parse_task_id(&route.metadata_id);
    let lock = LockedPlanFile::open(&route.metadata_file)?;
    let raw = lock.read_to_string("failed to read plan metadata file")?;
    let on_disk = parse_metadata_from_raw(&route.metadata_file, &raw)?;
    let (updated, effective) =
        set_provider_limit_metadata(on_disk.as_ref(), &metadata_id, state_name, observed);
    let rewritten = rewrite_frontmatter(&raw, &updated)?;
    write_file_atomic_locked(&route.metadata_file, &rewritten, Some(&lock))?;
    Ok(effective)
}

/// Clear only a successful invocation of the reporting identity that started
/// after eligibility. An already-running sibling cannot consume this wait,
/// even if it finishes after the deadline. §FS-rhei-run.3.3
fn clear_persisted_provider_limit(
    input: &Path,
    loaded: &LoadedPlan,
    task_id: &str,
    state_name: &str,
    resolved: &ResolvedAgent,
    started_at: std::time::SystemTime,
) -> MietteResult<()> {
    let route = loaded.task_route(task_id, input);
    let metadata_id = parse_task_id(&route.metadata_id);
    let lock = LockedPlanFile::open(&route.metadata_file)?;
    let raw = lock.read_to_string("failed to read plan metadata file")?;
    let on_disk = parse_metadata_from_raw(&route.metadata_file, &raw)?;
    let Some(limit) = provider_limit_for_task_state(on_disk.as_ref(), &metadata_id, state_name)
    else {
        return Ok(());
    };
    if !provider_limit_resumed(&limit, resolved, started_at) {
        return Ok(());
    }
    let Some(updated) =
        clear_provider_limit_state_metadata(on_disk.as_ref(), &metadata_id, state_name)
    else {
        return Ok(());
    };
    let rewritten = rewrite_frontmatter(&raw, &updated)?;
    write_file_atomic_locked(&route.metadata_file, &rewritten, Some(&lock))
}

fn provider_limit_resumed(
    limit: &ProviderLimit,
    resolved: &ResolvedAgent,
    started_at: std::time::SystemTime,
) -> bool {
    resolved_provider_identity(resolved).as_ref() == Some(&limit.identity)
        && limit.deadline_epoch().is_some_and(|deadline| {
            started_at >= std::time::UNIX_EPOCH + Duration::from_secs(deadline)
        })
}

/// Keep a pool with free capacity responsive to durable provider waits, even
/// after their deadline expires. The refill applies poll and identity readiness.
/// Stale-state records do not add a timer. §FS-rhei-run.3.3 §FS-rhei-run.5.1
fn has_pending_provider_wait(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> bool {
    let mut tasks = Vec::new();
    visit_tasks(&rhei.tasks, &mut tasks);
    tasks.into_iter().any(|task| {
        !is_terminal_state(task.state.as_str(), machines.for_task(&task.id))
            && task_provider_limit(rhei, machines, task)
                .and_then(|limit| limit.deadline_epoch())
                .is_some()
    })
}

fn visit_tasks<'a>(tasks: &'a [rhei_core::ast::Task], out: &mut Vec<&'a rhei_core::ast::Task>) {
    for task in tasks {
        out.push(task);
        visit_tasks(&task.children, out);
    }
}

/// Latest authored deadline for one execution identity in the tasks' current
/// states. Stale-state records do not participate. §FS-rhei-run.3.3
fn provider_deadline_for_identity(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    identity: &ProviderIdentity,
) -> Option<u64> {
    let mut tasks = Vec::new();
    visit_tasks(&rhei.tasks, &mut tasks);
    tasks
        .into_iter()
        .filter_map(|task| {
            let state = normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
            let limit = provider_limit_for_task_state(rhei.metadata.as_ref(), &task.id, &state)?;
            (limit.identity == *identity).then(|| limit.deadline_epoch()).flatten()
        })
        .max()
}

/// Latest active deadline for one execution identity. Expired records do not
/// suppress work. §FS-rhei-run.3.3
fn active_provider_deadline_for_identity(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    identity: &ProviderIdentity,
    now: u64,
) -> Option<u64> {
    provider_deadline_for_identity(rhei, machines, identity).filter(|deadline| *deadline > now)
}

fn resolved_provider_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    resolved: &ResolvedAgent,
    now: u64,
) -> Option<u64> {
    let identity = resolved_provider_identity(resolved)?;
    active_provider_deadline_for_identity(rhei, machines, &identity, now)
}

/// The next instant at which an identity can be scheduled. A matching expired
/// record means "now", so a deadline changed between a worker-pool refill and
/// the outer scheduler check causes an immediate rescan rather than a false
/// end-of-run decision. §FS-rhei-run.3.3 §FS-rhei-run.5.1
fn resolved_provider_eligibility_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    resolved: &ResolvedAgent,
    now: u64,
) -> Option<u64> {
    let identity = resolved_provider_identity(resolved)?;
    provider_deadline_for_identity(rhei, machines, &identity).map(|deadline| deadline.max(now))
}

fn task_provider_limit(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    task: &rhei_core::ast::Task,
) -> Option<ProviderLimit> {
    let state = normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
    provider_limit_for_task_state(rhei.metadata.as_ref(), &task.id, &state)
}
