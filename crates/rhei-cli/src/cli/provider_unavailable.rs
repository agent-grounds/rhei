// What a `pi` session's own events say about a provider that stopped answering,
// and the wait that verdict earns.
//
// Its own part because it is the refusal's sibling, not one of its grammars:
// the session exits `0`, prints no reset instant, and is judged from pi's
// `--mode json` events rather than from whole lines. Nothing here decides a
// charge or prints a line; the output check does, once a required output is
// missing.

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

/// pi's verdict that its own retries gave up on the provider, with what the
/// wait is keyed on. It decides nothing until the output check finds a required
/// output missing. §FS-rhei-agents.2.5
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderUnavailable {
    identity: ProviderIdentity,
    /// pi's `finalError`. §FS-rhei-agents.8.4
    signal: String,
    /// Whether any assistant message of the session ended without error: the
    /// provider answered, so the worker had begun the work. §FS-rhei-agents.3.2.3
    answered: bool,
    /// When the session ended, the instant the backoff counts from. §FS-rhei-run.3.3
    observed: std::time::SystemTime,
}

/// Recognize a `pi` session that lost its provider, from everything but the
/// missing output, which only the output check knows. The family is tested and
/// the registry id never is; a target that resolves no provider has nothing to
/// key the wait on and is not recognized. §FS-rhei-agents.2.5
fn classify_provider_unavailable(
    resolved: &ResolvedAgent,
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    captured_lines: &[(rhei_tui::AgentStream, String)],
    observed: std::time::SystemTime,
) -> Option<ProviderUnavailable> {
    if resolved.family() != "pi" || !status.success() || timed_out || interrupted {
        return None;
    }
    let identity = resolved_provider_identity(resolved)?;
    // Stderr and non-JSON stdout contribute nothing. §FS-rhei-agents.2.5
    let events = captured_lines
        .iter()
        .filter(|(stream, _)| *stream == rhei_tui::AgentStream::Stdout)
        .filter_map(|(_, line)| serde_json::from_str::<serde_json::Value>(line).ok());
    let (signal, answered) = pi_gave_up_retrying(events)?;
    Some(ProviderUnavailable { identity, signal, answered, observed })
}

/// The last `auto_retry_end` failed at the `maxAttempts` of the last
/// `auto_retry_start` before it, and the last assistant `message_end` came
/// before it with `stopReason: "error"`. Returns pi's `finalError` and whether
/// any assistant message ended without error. §FS-rhei-agents.2.5
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
