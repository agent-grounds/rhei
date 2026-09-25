// What one agent session log says, read back as structure: the header and
// exit footer rhei wrote, and the agent CLI's own event stream between them.
//
// Its own part because parsing is per-extractor knowledge (the Pi collectors
// here, the Claude Code and Codex collectors in their sibling files), while
// the Markdown rendering next door is stream-agnostic. The parser never
// invents: a log without an exit footer stays a log without an exit footer,
// and an unsupported stream is reported as unsupported.

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
    /// The body verbatim, when it carries no event stream: the agent's plain
    /// output as captured, not an unknown stream.
    /// §FS-rhei-session-reports.6.4
    plain_output: Option<String>,
}

/// The framed pieces of one session log: rhei's header and exit footer, and
/// the agent CLI's body between them, 1-based line numbers preserved.
struct FramedLog<'a> {
    header: Vec<(String, String)>,
    exit: Vec<(String, String)>,
    body: Vec<(usize, &'a str)>,
}

/// The stream one supported extractor reads. §FS-rhei-session-reports.6
#[derive(Clone, Copy)]
enum SessionStream {
    Pi,
    Claude,
    Codex,
}

/// What the body's stream turned out to be. §FS-rhei-session-reports.6.1
enum StreamDetection {
    /// A marker event named the stream.
    Stream(SessionStream),
    /// A JSON event stream without a marker, read as the agent the log
    /// header records; a fallback that collects nothing is reported as
    /// unsupported, never rendered as a confidently empty report.
    HeaderFallback(SessionStream),
    Unsupported,
    /// No event stream at all: the body is plain output.
    PlainOutput,
}

/// Marker event types that identify a stream on their own. The Claude Code
/// marker is its `system` init event, checked by `stream_marker`, because a
/// bare `system` type is too generic to claim. §FS-rhei-session-reports.6.1
const PI_STREAM_MARKER_TYPES: &[&str] = &["session", "agent_start"];
const CODEX_STREAM_MARKER_TYPES: &[&str] = &[
    "thread.started",
    "turn.started",
    "turn.completed",
    "turn.failed",
    "item.started",
    "item.updated",
    "item.completed",
];

fn parse_session_log(log_path: &Path) -> MietteResult<SessionTranscript> {
    let raw = fs::read_to_string(log_path).map_err(|err| {
        miette!(help = session_report_help(), "failed to read session log '{}': {err}", log_path.display())
    })?;
    let framed = frame_log(&raw);
    let mut transcript = SessionTranscript {
        header: framed.header.clone(),
        exit: framed.exit.clone(),
        prompt: None,
        events: Vec::new(),
        results: HashMap::new(),
        usage: None,
        unsupported_stream: None,
        plain_output: None,
    };
    match detect_stream(&framed) {
        StreamDetection::Stream(stream) => {
            collect_stream(&mut transcript, stream, &framed.body);
        }
        StreamDetection::HeaderFallback(stream) => {
            collect_stream(&mut transcript, stream, &framed.body);
            if transcript_collected_nothing(&transcript) {
                transcript.unsupported_stream = Some(unsupported_stream_notice());
            }
        }
        StreamDetection::Unsupported => {
            transcript.unsupported_stream = Some(unsupported_stream_notice());
        }
        StreamDetection::PlainOutput => {
            let body: Vec<&str> = framed.body.iter().map(|(_, line)| *line).collect();
            let body = body.join("\n");
            let body = body.trim_matches('\n');
            if !body.trim().is_empty() {
                transcript.plain_output = Some(body.to_string());
            }
        }
    }
    Ok(transcript)
}

fn unsupported_stream_notice() -> String {
    "this log's event stream is not one this renderer reads (pi, claude-code, \
     or codex); rendering it is not supported yet"
        .to_string()
}

/// Split one log into rhei's header, the body, and the exit footer. The
/// footer is the last well-formed `=== exit ===` block — `key: value` lines
/// closed by `===` — so body text quoting the markers never steals the real
/// footer, and a bare `===` in the body stays body text. Output a detached
/// reader appended after the footer is body again, in log order.
/// §FS-rhei-session-reports.5 §FS-rhei-session-reports.6.4
fn frame_log(raw: &str) -> FramedLog<'_> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut header = Vec::new();
    let mut body_start = 0usize;
    if lines.first().map(|line| line.trim()) == Some("=== rhei agent log v1 ===") {
        let mut index = 1;
        while index < lines.len() {
            let trimmed = lines[index].trim();
            index += 1;
            if trimmed == "===" {
                break;
            }
            if let Some((key, value)) = trimmed.split_once(": ") {
                header.push((key.to_string(), value.to_string()));
            }
        }
        body_start = index;
    }
    let mut exit = Vec::new();
    // The footer's marker line and the line after its closing `===`; with
    // no footer, the body runs to the end of the log.
    let mut footer = (lines.len(), lines.len());
    for marker in (body_start..lines.len()).rev() {
        if lines[marker].trim() != "=== exit ===" {
            continue;
        }
        if let Some(after) = footer_block_end(&lines, marker) {
            exit = lines[marker + 1..after]
                .iter()
                .filter_map(|line| line.trim().split_once(": "))
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            footer = (marker, after);
            break;
        }
    }
    let body = (body_start..footer.0)
        .chain(footer.1..lines.len())
        .map(|index| (index + 1, lines[index]))
        .collect();
    FramedLog { header, exit, body }
}

/// Where the footer block opened at `marker` ends — the index just past its
/// closing `===`, or the log's end when the write was cut short — or `None`
/// when a line inside it is not `key: value`, so the marker is body text.
fn footer_block_end(lines: &[&str], marker: usize) -> Option<usize> {
    for (index, line) in lines.iter().enumerate().skip(marker + 1) {
        let trimmed = line.trim();
        if trimmed == "===" {
            return Some(index + 1);
        }
        if !trimmed.is_empty() && !trimmed.contains(": ") {
            return None;
        }
    }
    Some(lines.len())
}

/// The marker a single event carries, if any. Marker vocabularies are
/// disjoint across the three streams. §FS-rhei-session-reports.6.1
fn stream_marker(event: &serde_json::Value) -> Option<SessionStream> {
    let event_type = event.get("type").and_then(|t| t.as_str())?;
    if PI_STREAM_MARKER_TYPES.contains(&event_type) {
        return Some(SessionStream::Pi);
    }
    if event_type == "system"
        && event.get("subtype").and_then(|s| s.as_str()) == Some("init")
    {
        return Some(SessionStream::Claude);
    }
    if CODEX_STREAM_MARKER_TYPES.contains(&event_type) {
        return Some(SessionStream::Codex);
    }
    None
}

/// What the body is: the first marker names the stream; a body that is
/// mostly JSON objects without one is an event stream read as the header's
/// agent; a body that is mostly prose is plain output, even when it quotes
/// the odd JSON line. §FS-rhei-session-reports.6.1
fn detect_stream(framed: &FramedLog<'_>) -> StreamDetection {
    let mut json_objects = 0usize;
    let mut nonblank = 0usize;
    for (_, line) in &framed.body {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        nonblank += 1;
        if !trimmed.starts_with('{') {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if !event.is_object() {
            continue;
        }
        json_objects += 1;
        if let Some(stream) = stream_marker(&event) {
            return StreamDetection::Stream(stream);
        }
    }
    if json_objects == 0 || json_objects.saturating_mul(2) <= nonblank {
        return StreamDetection::PlainOutput;
    }
    let agent = framed.header.iter().find(|(key, _)| key == "agent").map(|(_, v)| v.as_str());
    match agent {
        Some("pi") => StreamDetection::HeaderFallback(SessionStream::Pi),
        Some("claude-code") => StreamDetection::HeaderFallback(SessionStream::Claude),
        Some("codex") => StreamDetection::HeaderFallback(SessionStream::Codex),
        _ => StreamDetection::Unsupported,
    }
}

/// Fold the body through the detected stream's collectors alone, so one
/// stream's generic event names (`error`, `result`) never leak into
/// another's report. §FS-rhei-session-reports.6.1
fn collect_stream(
    transcript: &mut SessionTranscript,
    stream: SessionStream,
    body: &[(usize, &str)],
) {
    for (line_number, line) in body {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match stream {
            SessionStream::Pi => match event_type {
                "message_end" => collect_message_end(transcript, &event),
                "tool_execution_end" => {
                    collect_tool_result(transcript, &event, *line_number)
                }
                _ => {}
            },
            SessionStream::Claude => match event_type {
                "assistant" | "user" => {
                    collect_claude_event(transcript, &event, *line_number)
                }
                "result" => collect_claude_result(transcript, &event),
                _ => {}
            },
            SessionStream::Codex => match event_type {
                "item.completed" => collect_codex_item(transcript, &event, *line_number),
                "turn.completed" => collect_codex_turn_usage(transcript, &event),
                "turn.failed" | "error" => collect_codex_failure(transcript, &event),
                _ => {}
            },
        }
    }
}

/// Whether a parse produced nothing a report could show.
fn transcript_collected_nothing(transcript: &SessionTranscript) -> bool {
    transcript.prompt.is_none()
        && transcript.events.is_empty()
        && transcript.results.is_empty()
        && transcript.usage.is_none()
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

/// The argument that identifies a tool call, labelled by what it is and
/// preferred over dumping the whole argument object. A search's pattern
/// outranks the directory it searched. §FS-rhei-session-reports.2
fn tool_argument_identity(arguments: &serde_json::Value) -> (&'static str, String) {
    const IDENTIFYING_KEYS: &[(&str, &str)] = &[
        ("command", "Command"),
        ("pattern", "Pattern"),
        ("query", "Query"),
        ("url", "URL"),
        ("file_path", "File"),
        ("path", "Path"),
    ];
    let Some(object) = arguments.as_object() else { return ("Arguments", String::new()) };
    for (key, label) in IDENTIFYING_KEYS {
        if let Some(value) = object.get(*key).and_then(|v| v.as_str()) {
            return (label, value.to_string());
        }
    }
    // A Codex file change identifies itself by the paths it touched.
    // §FS-rhei-session-reports.6.3
    if let Some(changes) = object.get("changes").and_then(|c| c.as_array()) {
        let paths: Vec<&str> = changes
            .iter()
            .filter_map(|change| change.get("path").and_then(|p| p.as_str()))
            .collect();
        if !paths.is_empty() {
            return ("Files", paths.join(", "));
        }
    }
    ("Arguments", object.keys().map(|k| format!("{k}=…")).collect::<Vec<_>>().join(", "))
}
