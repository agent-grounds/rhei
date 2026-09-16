// Recognition, persistence, and scheduling facts for provider-limit waits.
//
// This is one shared boundary because a provider refusal must mean the same
// thing in sequential and parallel completion, and every scheduler surface
// must read the same durable record.

// §FS-rhei-agents.2 §FS-rhei-run.3.3 §FS-rhei-run.5.1

use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat};
use chrono::{TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;

const PROVIDER_LIMITS_KEY: &str = "providerLimits";

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
        DateTime::parse_from_rfc3339(&self.next_attempt_at)
            .ok()?
            .timestamp()
            .try_into()
            .ok()
    }
}

fn resolved_provider_identity(resolved: &ResolvedAgent) -> Option<ProviderIdentity> {
    Some(ProviderIdentity {
        agent: resolved.agent.id().to_string(),
        provider: resolved.model_provider.clone()?,
    })
}

/// Strip ANSI CSI/OSC decoration without changing any other text. The caller
/// trims surrounding whitespace only after this operation. §FS-rhei-agents.2
fn strip_terminal_decoration(line: &str) -> std::borrow::Cow<'_, str> {
    static ANSI: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    ANSI.get_or_init(|| {
        Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")
            .expect("ANSI decoration regex is valid")
    })
    .replace_all(line, "")
}

fn provider_signal_regex() -> &'static Regex {
    static SIGNAL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    SIGNAL.get_or_init(|| {
        Regex::new(
            r"^You've hit your session limit · resets ([1-9]|1[0-2]):([0-5][0-9])(am|pm) \(([^()\s]+)\)$",
        )
        .expect("provider-limit signal regex is valid")
    })
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
    let boundary = reported.checked_add_signed(TimeDelta::minutes(1))?;
    if !matches!(zone.from_local_datetime(&reported), LocalResult::Single(_)) {
        return None;
    }
    match zone.from_local_datetime(&boundary) {
        LocalResult::Single(value) => Some(value.with_timezone(&Utc)),
        LocalResult::Ambiguous(_, _) | LocalResult::None => None,
    }
}

/// Recognize the one supported Codex/OpenAI reset signal and turn its named
/// local minute into the first safe UTC instant after that minute.
/// §FS-rhei-agents.2 §FS-rhei-run.3.3
fn classify_provider_limit(
    resolved: &ResolvedAgent,
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    captured_lines: &[String],
    observed: std::time::SystemTime,
) -> Option<ProviderLimit> {
    let identity = resolved_provider_identity(resolved)?;
    if identity.agent != "codex"
        || identity.provider != "openai"
        || status.success()
        || timed_out
        || interrupted
    {
        return None;
    }

    let matching = captured_lines
        .iter()
        .map(|line| strip_terminal_decoration(line).trim().to_string())
        .filter(|line| provider_signal_regex().is_match(line))
        .collect::<Vec<_>>();
    let [signal] = matching.as_slice() else { return None };
    let captures = provider_signal_regex().captures(signal)?;

    let hour12 = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let hour = match captures.get(3)?.as_str() {
        "am" => hour12 % 12,
        "pm" => (hour12 % 12) + 12,
        _ => return None,
    };
    let zone = captures.get(4)?.as_str().parse::<Tz>().ok()?;
    let observed_utc: DateTime<Utc> = observed.into();
    let local_date = observed_utc.with_timezone(&zone).date_naive();
    let mut deadline = unique_safe_boundary(zone, local_date, hour, minute)?;
    if deadline <= observed_utc {
        let next_date = local_date.succ_opt()?;
        deadline = unique_safe_boundary(zone, next_date, hour, minute)?;
        if deadline <= observed_utc {
            return None;
        }
    }

    Some(ProviderLimit {
        identity,
        signal: signal.clone(),
        observed_at: observed_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
        next_attempt_at: deadline.to_rfc3339_opts(SecondsFormat::Secs, true),
    })
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

/// Reset owns all runtime wait state, including provider deadlines.
/// §FS-rhei-reset.2 §FS-rhei-run.3.3
fn clear_runtime_provider_limits(existing: Option<&Metadata>) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };
    for value in tasks.values_mut() {
        if let YamlValue::Mapping(task) = value {
            task.remove(yaml_key(PROVIDER_LIMITS_KEY));
        }
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

/// Clear a successfully resumed invocation's record without touching poll or
/// visit counters. §FS-rhei-run.3.3
fn clear_persisted_provider_limit(
    input: &Path,
    loaded: &LoadedPlan,
    task_id: &str,
    state_name: &str,
) -> MietteResult<()> {
    let route = loaded.task_route(task_id, input);
    let metadata_id = parse_task_id(&route.metadata_id);
    let lock = LockedPlanFile::open(&route.metadata_file)?;
    let raw = lock.read_to_string("failed to read plan metadata file")?;
    let on_disk = parse_metadata_from_raw(&route.metadata_file, &raw)?;
    if provider_limit_for_task_state(on_disk.as_ref(), &metadata_id, state_name).is_none() {
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

fn visit_tasks<'a>(tasks: &'a [rhei_core::ast::Task], out: &mut Vec<&'a rhei_core::ast::Task>) {
    for task in tasks {
        out.push(task);
        visit_tasks(&task.children, out);
    }
}

/// Latest active deadline for one execution identity. Stale-state and expired
/// records do not suppress work. §FS-rhei-run.3.3
fn active_provider_deadline_for_identity(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    identity: &ProviderIdentity,
    now: u64,
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
        .filter(|deadline| *deadline > now)
        .max()
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

fn task_provider_limit(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    task: &rhei_core::ast::Task,
) -> Option<ProviderLimit> {
    let state = normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
    provider_limit_for_task_state(rhei.metadata.as_ref(), &task.id, &state)
}
