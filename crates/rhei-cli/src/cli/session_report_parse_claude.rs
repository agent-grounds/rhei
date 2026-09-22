// What a Claude Code `stream-json` session log says: assistant messages carry
// the thinking, text, and tool-use events; `user` events carry the matched
// tool results; the final `result` envelope was already logged as its result
// text, so it arrives here only from captures that kept it as JSON.
//
// Its own part because parsing is per-extractor knowledge; the transcript it
// fills is the stream-agnostic one next door.

// §AR-source-file-size.3 §FS-rhei-session-reports.6.2

/// Fold one Claude stream event into the transcript: assistant content items
/// become events in order, user text becomes the prompt, and user tool
/// results answer the calls they name. §FS-rhei-session-reports.6.2
fn collect_claude_event(
    transcript: &mut SessionTranscript,
    event: &serde_json::Value,
    log_line: usize,
) {
    let Some(message) = event.get("message") else { return };
    match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "assistant" => collect_claude_assistant(transcript, message),
        "user" => collect_claude_user(transcript, message, log_line),
        _ => {}
    }
}

fn collect_claude_assistant(transcript: &mut SessionTranscript, message: &serde_json::Value) {
    for item in message.get("content").and_then(|c| c.as_array()).into_iter().flatten() {
        match item.get("type").and_then(|t| t.as_str()) {
            Some("thinking") => {
                if let Some(text) = item.get("thinking").and_then(|t| t.as_str()) {
                    if !text.trim().is_empty() {
                        transcript.events.push(SessionEvent::Thinking(text.to_string()));
                    }
                }
            }
            Some("text") => {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    if !text.trim().is_empty() {
                        transcript.events.push(SessionEvent::Text(text.to_string()));
                    }
                }
            }
            Some("tool_use") => {
                transcript.events.push(SessionEvent::ToolCall {
                    id: item
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: item
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("tool")
                        .to_string(),
                    arguments: item.get("input").cloned().unwrap_or(serde_json::Value::Null),
                });
            }
            _ => {}
        }
    }
    if let Some(usage) = message.get("usage") {
        transcript.usage = Some(usage.clone());
    }
}

/// A user event is either the delivered prompt (plain text — the stream does
/// not always echo one) or the results of earlier tool calls. Text blocks
/// riding along tool results are injected context (reminders, hook output),
/// never the prompt: fabricating one is exactly what the fidelity rules
/// forbid. §FS-rhei-session-reports.5 §FS-rhei-session-reports.6.2
fn collect_claude_user(
    transcript: &mut SessionTranscript,
    message: &serde_json::Value,
    log_line: usize,
) {
    if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
        if transcript.prompt.is_none() && !text.trim().is_empty() {
            transcript.prompt = Some(text.to_string());
        }
        return;
    }
    let items = message.get("content").and_then(|c| c.as_array());
    let carries_tool_result = items.into_iter().flatten().any(|item| {
        item.get("type").and_then(|t| t.as_str()) == Some("tool_result")
    });
    if !carries_tool_result {
        let plain: Vec<&str> = items
            .into_iter()
            .flatten()
            .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .collect();
        if transcript.prompt.is_none() && !plain.is_empty() {
            transcript.prompt = Some(plain.join("\n"));
        }
        return;
    }
    for item in items.into_iter().flatten() {
        if item.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
            continue;
        }
        let Some(id) = item.get("tool_use_id").and_then(|i| i.as_str()) else { continue };
        transcript.results.insert(
            id.to_string(),
            SessionToolResult {
                text: claude_tool_result_text(item),
                is_error: item.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false),
                log_line,
            },
        );
    }
}

/// A tool result's content is a plain string or a list of text items.
fn claude_tool_result_text(item: &serde_json::Value) -> String {
    match item.get("content") {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A raw `result` envelope: rhei logs it as its result text
/// (§FS-rhei-cost-accounting.4), so meeting one as JSON means the capture
/// kept it. Its usage supersedes the per-message usage as the session total;
/// its text normally repeats the final assistant message, so it becomes an
/// event only when the transcript does not already end with it — a
/// conclusion that lives only in the envelope is never dropped.
/// §FS-rhei-session-reports.6.2
fn collect_claude_result(transcript: &mut SessionTranscript, event: &serde_json::Value) {
    if let Some(usage) = event.get("usage") {
        transcript.usage = Some(usage.clone());
    }
    let last_text = transcript.events.iter().rev().find_map(|event| match event {
        SessionEvent::Text(text) => Some(text.as_str()),
        _ => None,
    });
    if let Some(text) = event.get("result").and_then(|r| r.as_str()) {
        if !text.trim().is_empty() && last_text != Some(text) {
            transcript.events.push(SessionEvent::Text(text.to_string()));
        }
    }
}
