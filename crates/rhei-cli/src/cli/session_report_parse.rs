// What one agent session log says, read back as structure: the header and
// exit footer rhei wrote, and the agent CLI's own event stream between them.
//
// Its own part because parsing is per-extractor knowledge (the Pi session
// stream here), while the Markdown rendering next door is stream-agnostic.
// The parser never invents: a log without an exit footer stays a log without
// an exit footer, and an unsupported stream is reported as unsupported.

// §AR-source-file-size.3 §FS-rhei-session-reports.5 §FS-rhei-session-reports.6

/// One assistant-authored item, in stream order.
// §FS-rhei-session-reports.2
enum SessionEvent {
    /// A thinking block, rendered collapsed.
    Thinking(String),
    /// An inline text block.
    Text(String),
    /// One tool call: name, identifying argument summary, full arguments.
    ToolCall { id: String, name: String, arguments: serde_json::Value },
}

/// The recorded execution result of one tool call.
struct SessionToolResult {
    text: String,
    is_error: bool,
    /// 1-based line in the log where the result event starts, so a truncated
    /// rendering can point at the full output. §FS-rhei-session-reports.3
    log_line: usize,
}

/// A session log parsed for rendering.
struct SessionTranscript {
    /// `key: value` pairs of the `=== rhei agent log v1 ===` header, in order.
    header: Vec<(String, String)>,
    /// `key: value` pairs of the `=== exit ===` footer; empty when the session
    /// ended without an exit record. §FS-rhei-session-reports.5
    exit: Vec<(String, String)>,
    /// The first user message: the full prompt, verbatim.
    prompt: Option<String>,
    events: Vec<SessionEvent>,
    results: HashMap<String, SessionToolResult>,
    /// Token usage of the last assistant message that reported any.
    usage: Option<serde_json::Value>,
    /// Set when the body is not a stream this renderer reads; the report then
    /// carries the header, the footer, and this notice instead of a wrongly
    /// rendered body. §FS-rhei-session-reports.6
    unsupported_stream: Option<String>,
}

/// Event types that identify the Pi session stream.
const PI_STREAM_MARKER_TYPES: &[&str] = &["session", "agent_start"];

fn parse_session_log(log_path: &Path) -> MietteResult<SessionTranscript> {
    let raw = fs::read_to_string(log_path).map_err(|err| {
        miette!(help = session_report_help(), "failed to read session log '{}': {err}", log_path.display())
    })?;
    let mut transcript = SessionTranscript {
        header: Vec::new(),
        exit: Vec::new(),
        prompt: None,
        events: Vec::new(),
        results: HashMap::new(),
        usage: None,
        unsupported_stream: None,
    };

    #[derive(PartialEq)]
    enum Section {
        Body,
        Header,
        Exit,
    }
    let mut section = Section::Body;
    let mut saw_pi_marker = false;
    let mut saw_json_event = false;

    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        match trimmed {
            "=== rhei agent log v1 ===" => {
                section = Section::Header;
                continue;
            }
            "=== exit ===" => {
                section = Section::Exit;
                continue;
            }
            "===" => {
                section = Section::Body;
                continue;
            }
            _ => {}
        }
        match section {
            Section::Header | Section::Exit => {
                if let Some((key, value)) = trimmed.split_once(": ") {
                    let pair = (key.to_string(), value.to_string());
                    if section == Section::Header {
                        transcript.header.push(pair);
                    } else {
                        transcript.exit.push(pair);
                    }
                }
            }
            Section::Body => {
                if !trimmed.starts_with('{') {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                    continue;
                };
                saw_json_event = true;
                let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if PI_STREAM_MARKER_TYPES.contains(&event_type) {
                    saw_pi_marker = true;
                }
                match event_type {
                    "message_end" => collect_message_end(&mut transcript, &event),
                    "tool_execution_end" => {
                        collect_tool_result(&mut transcript, &event, index + 1)
                    }
                    _ => {}
                }
            }
        }
    }

    if saw_json_event && !saw_pi_marker {
        transcript.unsupported_stream = Some(
            "this log's event stream is not a Pi session stream; rendering it \
             is not supported yet"
                .to_string(),
        );
    }
    Ok(transcript)
}

/// Fold one full message into the transcript: the first user message is the
/// prompt; assistant content items are events in order.
fn collect_message_end(transcript: &mut SessionTranscript, event: &serde_json::Value) {
    let Some(message) = event.get("message") else { return };
    let role = message.get("role").and_then(|r| r.as_str()).unwrap_or("");
    let content = message.get("content").and_then(|c| c.as_array());
    match role {
        "user" if transcript.prompt.is_none() => {
            let text: Vec<&str> = content
                .into_iter()
                .flatten()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text.is_empty() {
                transcript.prompt = Some(text.join("\n"));
            }
        }
        "assistant" => {
            for item in content.into_iter().flatten() {
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
                    Some("toolCall") => {
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
                            arguments: item
                                .get("arguments")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                        });
                    }
                    _ => {}
                }
            }
            if let Some(usage) = message.get("usage") {
                transcript.usage = Some(usage.clone());
            }
        }
        _ => {}
    }
}

/// Record one tool execution result, keyed by the call it answers.
fn collect_tool_result(
    transcript: &mut SessionTranscript,
    event: &serde_json::Value,
    log_line: usize,
) {
    let Some(id) = event.get("toolCallId").and_then(|i| i.as_str()) else { return };
    let text: Vec<&str> = event
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
        .collect();
    transcript.results.insert(
        id.to_string(),
        SessionToolResult {
            text: text.join("\n"),
            is_error: event.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
            log_line,
        },
    );
}

/// The argument that identifies a tool call, preferred over dumping the
/// whole argument object. §FS-rhei-session-reports.2
fn tool_argument_summary(arguments: &serde_json::Value) -> String {
    const IDENTIFYING_KEYS: &[&str] = &["command", "path", "file_path", "pattern", "url"];
    if let Some(object) = arguments.as_object() {
        for key in IDENTIFYING_KEYS {
            if let Some(value) = object.get(*key).and_then(|v| v.as_str()) {
                return value.to_string();
            }
        }
        return object.keys().map(|k| format!("{k}=…")).collect::<Vec<_>>().join(", ");
    }
    String::new()
}
