// What a Codex `--json` thread stream says: completed items carry the
// messages, reasoning, and tool work — each with its own execution result
// bundled in — and `turn.completed` carries the usage. Started and updated
// item events are deltas and are skipped, like Pi's streaming deltas.
//
// Its own part because parsing is per-extractor knowledge; the transcript it
// fills is the stream-agnostic one next door.

// §AR-source-file-size.3 §FS-rhei-session-reports.6.3

/// Fold one completed thread item into the transcript. A Codex item is the
/// call and its result in one event, so tool-shaped items push both the
/// `ToolCall` and the answer under the item's id.
/// §FS-rhei-session-reports.6.3
fn collect_codex_item(
    transcript: &mut SessionTranscript,
    event: &serde_json::Value,
    log_line: usize,
) {
    let Some(item) = event.get("item") else { return };
    let id = item.get("id").and_then(|i| i.as_str()).unwrap_or_default().to_string();
    let text_of = |key: &str| item.get(key).and_then(|t| t.as_str()).unwrap_or("");
    let status = item.get("status").and_then(|s| s.as_str()).unwrap_or("");
    match item.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "agent_message" => {
            let text = text_of("text");
            if !text.trim().is_empty() {
                transcript.events.push(SessionEvent::Text(text.to_string()));
            }
        }
        "reasoning" => {
            let text = text_of("text");
            if !text.trim().is_empty() {
                transcript.events.push(SessionEvent::Thinking(text.to_string()));
            }
        }
        "command_execution" => {
            transcript.events.push(SessionEvent::ToolCall {
                id: id.clone(),
                name: "command".to_string(),
                arguments: serde_json::json!({"command": text_of("command")}),
            });
            let exit_code = item.get("exit_code").and_then(|c| c.as_i64());
            transcript.results.insert(
                id,
                SessionToolResult {
                    text: text_of("aggregated_output").to_string(),
                    is_error: status != "completed" || exit_code.unwrap_or(0) != 0,
                    log_line,
                },
            );
        }
        "file_change" => {
            transcript.events.push(SessionEvent::ToolCall {
                id: id.clone(),
                name: "file_change".to_string(),
                arguments: serde_json::json!({
                    "changes": item.get("changes").cloned()
                        .unwrap_or(serde_json::Value::Null),
                }),
            });
            transcript.results.insert(
                id,
                SessionToolResult {
                    text: format!("patch {status}"),
                    is_error: status == "failed",
                    log_line,
                },
            );
        }
        "mcp_tool_call" => {
            transcript.events.push(SessionEvent::ToolCall {
                id: id.clone(),
                name: format!("{}.{}", text_of("server"), text_of("tool")),
                arguments: item.get("arguments").cloned().unwrap_or(serde_json::Value::Null),
            });
            transcript.results.insert(
                id,
                SessionToolResult {
                    text: codex_mcp_result_text(item),
                    is_error: status == "failed",
                    log_line,
                },
            );
        }
        "web_search" => {
            // The stream records the query, never the returned results, and
            // the report says only what the log says.
            // §FS-rhei-session-reports.5
            transcript.events.push(SessionEvent::ToolCall {
                id,
                name: "web_search".to_string(),
                arguments: serde_json::json!({"query": text_of("query")}),
            });
        }
        "error" => {
            let message = text_of("message");
            if !message.is_empty() {
                transcript.events.push(SessionEvent::Text(format!("codex error: {message}")));
            }
        }
        // The to-do list is planning churn, skipped like streaming deltas.
        _ => {}
    }
}

/// An MCP item's answer: the result's text items, or the reported error.
fn codex_mcp_result_text(item: &serde_json::Value) -> String {
    if let Some(message) = item
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return message.to_string();
    }
    item.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Usage of the last completed turn wins, mirroring how the other streams
/// keep the last reported usage. §FS-rhei-session-reports.6.3
fn collect_codex_turn_usage(transcript: &mut SessionTranscript, event: &serde_json::Value) {
    if let Some(usage) = event.get("usage") {
        transcript.usage = Some(usage.clone());
    }
}

/// A failed turn or a fatal stream error is an event in order, not a reason
/// to drop the transcript up to it. §FS-rhei-session-reports.5
fn collect_codex_failure(transcript: &mut SessionTranscript, event: &serde_json::Value) {
    let message = event
        .get("error")
        .and_then(|e| e.get("message"))
        .or_else(|| event.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if !message.is_empty() {
        transcript.events.push(SessionEvent::Text(format!("codex error: {message}")));
    }
}
