// What an agent's own stream says about a provider that stopped answering, and
// the wait that verdict earns: pi's events or Claude Code's, each read by its
// own parser.
//
// Its own part because it is the refusal's sibling, not one of its grammars: no
// reset instant is printed, and the verdict is the family's own count of its
// retries rather than a whole line. Nothing here decides a charge or prints a
// line; the exit handling does.

// §FS-rhei-agents.2.5 §FS-rhei-agents.3.2.3 §FS-rhei-run.3.3

/// How many provider-unavailable endings of one visit can be free. Built in,
/// with no setting. §FS-rhei-agents.3.2.3
const FREE_UNAVAILABLE_ENDINGS: u64 = 6;

/// Which wait a provider record holds. A record without `kind` is a limit, so
/// every record an older rhei wrote reads as the one it was. §FS-rhei-run.3.3
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderWaitKind {
    /// A recognized refusal's reset instant. §FS-rhei-agents.2.3
    Limit,
    /// A recognized unavailability's backoff. §FS-rhei-agents.2.5
    Unavailable,
}

impl ProviderWaitKind {
    /// The record's `kind`, absent or unknown reading as a limit. §FS-rhei-run.3.3
    fn read(value: Option<&YamlValue>) -> Self {
        match value.and_then(YamlValue::as_str) {
            Some("unavailable") => ProviderWaitKind::Unavailable,
            _ => ProviderWaitKind::Limit,
        }
    }

    /// What the record writes for `kind`: nothing for a limit, so a limit's
    /// record is byte-identical to the one written before `kind` existed.
    // §FS-rhei-run.3.3
    fn written(self) -> Option<&'static str> {
        match self {
            ProviderWaitKind::Limit => None,
            ProviderWaitKind::Unavailable => Some("unavailable"),
        }
    }
}

/// A family's verdict that its own retries gave up on the provider, with what
/// the wait is keyed on. A `pi` verdict decides nothing until the output check
/// finds a required output missing; a `claude-code` one settles its non-zero
/// exit. §FS-rhei-agents.2.5
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderUnavailable {
    identity: ProviderIdentity,
    /// The resolved family whose verdict this is, `pi` or `claude-code`.
    /// §FS-rhei-agents.3.2.1
    family: &'static str,
    /// The family's own give-up text. §FS-rhei-agents.8.4
    signal: String,
    /// Whether the provider answered anything in the session, as the family
    /// counts an answer, so the worker had begun the work.
    /// §FS-rhei-provider-unavailability.3
    answered: bool,
    /// When the session ended, the instant the backoff counts from. §FS-rhei-run.3.3
    observed: std::time::SystemTime,
}

/// Recognize a session whose family's own retries gave up on its provider: pi's
/// from everything but the missing output, which only the output check knows,
/// and Claude Code's from its non-zero exit. The family is tested and the
/// registry id never is; an invocation with no provider to key the wait on is
/// not recognized, and a timeout or interruption is classified first.
/// §FS-rhei-provider-unavailability
fn classify_provider_unavailable(
    resolved: &ResolvedAgent,
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    captured_lines: &[(rhei_tui::AgentStream, String)],
    observed: std::time::SystemTime,
) -> Option<ProviderUnavailable> {
    if timed_out || interrupted {
        return None;
    }
    // pi exits 0 on a give-up; Claude Code exits non-zero. §FS-rhei-provider-unavailability.1
    // §FS-rhei-provider-unavailability.2
    let family = match (resolved.family(), status.success()) {
        ("pi", true) => "pi",
        ("claude-code", false) => "claude-code",
        _ => return None,
    };
    // Resolved, or derived from pi's model reference; never for Claude Code.
    // §FS-rhei-provider-unavailability.2
    let identity = resolved_provider_identity(resolved)?;
    // Stderr and non-JSON stdout contribute nothing. §FS-rhei-provider-unavailability
    let events = captured_lines
        .iter()
        .filter(|(stream, _)| *stream == rhei_tui::AgentStream::Stdout)
        .filter_map(|(_, line)| serde_json::from_str::<serde_json::Value>(line).ok());
    let (signal, answered) = match family {
        "pi" => pi_gave_up_retrying(events)?,
        _ => claude_code_gave_up_retrying(events.collect())?,
    };
    Some(ProviderUnavailable { identity, family, signal, answered, observed })
}

/// Claude Code's own verdict: the last `api_retry` reached its own
/// `max_retries` on a transient status class (`null`, `429` or a `5xx`), and an
/// `is_api_error_message` event, and no answer from the model, came after it.
/// Returns that event's text and whether the model answered anything in the
/// session; a `<synthetic>` message never counts as an answer.
/// §FS-rhei-provider-unavailability.2 §FS-rhei-provider-unavailability.3
fn claude_code_gave_up_retrying(events: Vec<serde_json::Value>) -> Option<(String, bool)> {
    let is_assistant = |event: &serde_json::Value| event["type"] == "assistant";
    let answers = |event: &serde_json::Value| {
        is_assistant(event) && event["message"]["model"] != "<synthetic>"
    };
    let answered = events.iter().any(answers);
    let last_retry = events
        .iter()
        .rposition(|event| event["type"] == "system" && event["subtype"] == "api_retry")?;
    let retry = &events[last_retry];
    let used_up = retry["attempt"].as_u64().is_some_and(|attempt| {
        retry["max_retries"].as_u64() == Some(attempt)
    });
    // The status class is the protocol's; Claude Code's error names are never read.
    // §FS-rhei-provider-unavailability.2
    let transient = match &retry["error_status"] {
        serde_json::Value::Null => true,
        status => status.as_u64().is_some_and(|code| code == 429 || (500..600).contains(&code)),
    };
    if !used_up || !transient {
        return None;
    }
    let after = &events[last_retry + 1..];
    if after.iter().any(answers) {
        return None;
    }
    let give_up = after
        .iter()
        .rev()
        .find(|event| is_assistant(event) && event["is_api_error_message"] == true)?;
    let text: Vec<&str> = give_up["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["text"].as_str())
        .collect();
    let signal = if text.is_empty() { "no error message".to_string() } else { text.join(" ") };
    Some((signal, answered))
}

/// The last `auto_retry_end` failed at the `maxAttempts` of the last
/// `auto_retry_start` before it, and the last assistant `message_end` came
/// before it with `stopReason: "error"`. Returns pi's `finalError` and whether
/// any assistant message ended without error. §FS-rhei-provider-unavailability.1
fn pi_gave_up_retrying(
    events: impl Iterator<Item = serde_json::Value>,
) -> Option<(String, bool)> {
    let field = |event: &serde_json::Value, name: &str| event.get(name).cloned();
    let (mut max_attempts, mut last_turn, mut answered) = (None, None, false);
    // The last end, with the start and the turn it followed, and whether an
    // assistant turn came after it.
    let mut end: Option<(serde_json::Value, Option<serde_json::Value>, Option<serde_json::Value>)> =
        None;
    let mut turn_after_end = false;
    for event in events {
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("auto_retry_start") => max_attempts = field(&event, "maxAttempts"),
            Some("auto_retry_end") => {
                end = Some((event, max_attempts.clone(), last_turn.clone()));
                turn_after_end = false;
            }
            Some("message_end") if event["message"]["role"] == "assistant" => {
                let message = event["message"].clone();
                answered |= message["stopReason"] != "error";
                last_turn = Some(message);
                turn_after_end = end.is_some();
            }
            _ => {}
        }
    }
    let (end, max_attempts, turn) = end?;
    let turn = turn?;
    let gave_up = end["success"] == false
        && max_attempts.is_some_and(|max| end["attempt"] == max)
        && turn["stopReason"] == "error"
        && !turn_after_end;
    if !gave_up {
        return None;
    }
    let signal = [&end["finalError"], &turn["errorMessage"]]
        .into_iter()
        .find_map(|text| text.as_str())
        .unwrap_or("no error message")
        .to_string();
    Some((signal, answered))
}

/// Minutes the visit's `n`-th unavailable ending waits after it was observed:
/// 1, 2, 4, 8, 16 and 32, then 60 from the seventh on. §FS-rhei-run.3.3
fn unavailable_backoff_minutes(n: u64) -> i64 {
    match n {
        0 | 1 => 1,
        2..=FREE_UNAVAILABLE_ENDINGS => 1 << (n - 1),
        _ => 60,
    }
}

impl ProviderUnavailable {
    /// The wait the visit's `n`-th unavailable ending writes, keyed as a
    /// refusal's is and read by everything that reads one. §FS-rhei-run.3.3
    fn wait(&self, n: u64) -> Option<ProviderLimit> {
        let observed: DateTime<Utc> = self.observed.into();
        // Whole seconds, so the record's two instants differ by the backoff exactly.
        let observed = DateTime::from_timestamp(observed.timestamp(), 0)?;
        let deadline = observed + TimeDelta::minutes(unavailable_backoff_minutes(n));
        Some(ProviderLimit {
            identity: self.identity.clone(),
            kind: ProviderWaitKind::Unavailable,
            signal: self.signal.clone(),
            observed_at: observed.to_rfc3339_opts(SecondsFormat::Secs, true),
            next_attempt_at: deadline.to_rfc3339_opts(SecondsFormat::Secs, true),
        })
    }
}
