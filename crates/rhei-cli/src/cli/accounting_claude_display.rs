// What a Claude Code `stream-json` line shows on the live surfaces: the
// session start and the assistant's text, as Pi's display shows; every other
// event (thinking, tool traffic, rate-limit notices) is suppressed, and the
// raw line still reaches the log for the session report.
//
// Its own part because accounting.rs is at its size register's cap and this
// is per-agent display knowledge, not record writing.

// §AR-source-file-size.2 §FS-rhei-run-tui.1.2 §FS-rhei-cost-accounting.4

/// The display of one non-result Claude stream line. The final result is
/// displayed by the caller, which owns the result envelope.
fn display_claude_stream_line(line: &str) -> AgentOutputLine {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return AgentOutputLine::Passthrough;
    };
    let Some(object) = value.as_object() else {
        return AgentOutputLine::Passthrough;
    };
    match object.get("type").and_then(serde_json::Value::as_str) {
        Some("system") if object.get("subtype").and_then(serde_json::Value::as_str) == Some("init") => {
            object
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .map(|id| AgentOutputLine::Replace(format!("claude-code session started: {id}")))
                .unwrap_or(AgentOutputLine::Suppress)
        }
        Some("assistant") => {
            let text = object
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(serde_json::Value::as_str))
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                AgentOutputLine::Suppress
            } else {
                AgentOutputLine::Replace(text)
            }
        }
        // A JSON line with no stream type is not a stream event; show it.
        None => AgentOutputLine::Passthrough,
        Some(_) => AgentOutputLine::Suppress,
    }
}
