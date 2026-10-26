// A Codex worker may be told after it starts that a setting Rhei asked for is
// not allowed, carry on with a weaker one, exit 0 and write nothing. What it
// reported is kept as a diagnostic of the invocation, informational only and
// outside the provider-refusal grammar.

// §FS-rhei-configuration-fallbacks

/// What a fallback message begins with, either side of its backtick-quoted
/// configuration key. §FS-rhei-configuration-fallbacks
const CODEX_FALLBACK_HEAD: &str = "Configured value for `";
const CODEX_FALLBACK_TAIL: &str = "` is disallowed by requirements; falling back";

/// The configuration fallbacks one invocation's own stdout carried, each the
/// whole trimmed message, distinct and in the order first seen. Only the
/// resolved `codex` family is read; stderr never is.
/// §FS-rhei-configuration-fallbacks
fn codex_configuration_fallbacks(
    family: &str,
    captured_lines: &[(rhei_tui::AgentStream, String)],
) -> Vec<String> {
    if family != "codex" {
        return Vec::new();
    }
    let mut fallbacks: Vec<String> = Vec::new();
    for (stream, line) in captured_lines {
        if *stream != rhei_tui::AgentStream::Stdout {
            continue;
        }
        if let Some(message) = codex_fallback_message(line) {
            if !fallbacks.contains(&message) {
                fallbacks.push(message);
            }
        }
    }
    fallbacks
}

/// The fallback one stdout line reports: a single JSON object that is a
/// top-level `error` event, or an `item.completed` event whose item is an
/// `error`, with a textual message that begins as a fallback does. Any other
/// event quoting the text — an `agent_message` above all — reports none.
/// §FS-rhei-configuration-fallbacks
fn codex_fallback_message(line: &str) -> Option<String> {
    let event = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let error = match event.get("type")?.as_str()? {
        "error" => &event,
        "item.completed" => event
            .get("item")
            .filter(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("error"))?,
        _ => return None,
    };
    let message = error.get("message")?.as_str()?.trim();
    is_codex_configuration_fallback(message).then(|| message.to_string())
}

/// Whether a trimmed message begins
/// ``Configured value for `<field>` is disallowed by requirements; falling back``
/// for some non-empty key. §FS-rhei-configuration-fallbacks
fn is_codex_configuration_fallback(message: &str) -> bool {
    let Some(rest) = message.strip_prefix(CODEX_FALLBACK_HEAD) else { return false };
    rest.find('`').is_some_and(|end| end > 0 && rest[end..].starts_with(CODEX_FALLBACK_TAIL))
}

/// Say what a finished spawn fell back from, one warning per message as its
/// slot is released and before any missing-output warning for it, whether or
/// not it succeeded; the messages then travel on the release.
/// §FS-rhei-run-report.3.4 §FS-rhei-configuration-fallbacks
fn announce_configuration_fallbacks(
    result: &MietteResult<AgentSpawnOutcome>,
    task_id: &str,
    state_name: &str,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> Vec<String> {
    let fallbacks =
        result.as_ref().map(|outcome| outcome.configuration_fallbacks.clone()).unwrap_or_default();
    for message in &fallbacks {
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Warn,
            configuration_fallback_warning(task_id, state_name, message),
        );
    }
    fallbacks
}

/// The console line for one fallback. §FS-rhei-run-report.3.4
fn configuration_fallback_warning(task_id: &str, state_name: &str, message: &str) -> String {
    format!(
        "  warning: Codex configuration fallback for task {task_id} in state '{state_name}': \
         {message}"
    )
}

/// `; Codex configuration fallback: <message>` per message, in order, for a
/// halt row or an invocation's exit. §FS-rhei-run-report.3.1
fn configuration_fallback_clauses(fallbacks: &[String]) -> String {
    let mut clauses = String::new();
    for message in fallbacks {
        clauses.push_str("; Codex configuration fallback: ");
        clauses.push_str(message);
    }
    clauses
}
