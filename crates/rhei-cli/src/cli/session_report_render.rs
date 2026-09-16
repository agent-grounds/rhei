// One readable Markdown report per agent session log, and the run-level
// metrics summary: pure formatting over the transcript the parser read and
// the iteration records the ledger appended.
//
// Its own part because rendering is a derived, regenerable view — it never
// invents, softens, or reorders what the log and the records say, and failing
// to render never fails a run.

// §AR-source-file-size.3 §FS-rhei-session-reports.2 §FS-rhei-session-reports.3
// §FS-rhei-metrics.4

/// Default per-tool-output truncation. Rendering, never capture.
// §FS-rhei-session-reports.3
const SESSION_REPORT_OUTPUT_LIMIT: usize = 10 * 1024;

fn session_reports_dir(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("reports")
}

/// Render one session log to `runtime/reports/<log stem>.md`.
// §FS-rhei-session-reports.1
fn render_session_report(
    log_path: &Path,
    runtime_dir: &Path,
    full: bool,
) -> MietteResult<PathBuf> {
    let transcript = parse_session_log(log_path)?;
    let stem = log_path.file_stem().unwrap_or_default().to_string_lossy();
    let mut out = String::new();

    render_report_header(&mut out, &transcript, log_path, runtime_dir, &stem);
    render_metrics_strip(&mut out, runtime_dir, log_path);

    if let Some(notice) = &transcript.unsupported_stream {
        out.push_str(&format!("> {notice}\n"));
        return write_session_report(runtime_dir, &stem, &out);
    }

    out.push_str("## Prompt\n\n<details><summary>full prompt</summary>\n\n");
    out.push_str(&fenced(transcript.prompt.as_deref().unwrap_or("(no prompt recorded)")));
    out.push_str("\n</details>\n\n## Agent actions\n\n");

    let mut files: Vec<(String, Vec<(String, serde_json::Value)>)> = Vec::new();
    for event in &transcript.events {
        match event {
            SessionEvent::Thinking(text) => {
                out.push_str("<details><summary><i>thinking</i></summary>\n\n");
                out.push_str(&fenced(&truncate_output(text, full, None)));
                out.push_str("\n</details>\n\n");
            }
            SessionEvent::Text(text) => {
                out.push_str(text);
                out.push_str("\n\n");
            }
            SessionEvent::ToolCall { id, name, arguments } => {
                render_tool_call(&mut out, &transcript, full, id, name, arguments);
                note_written_file(&mut files, name, arguments);
            }
        }
    }

    render_files_produced(&mut out, &files);
    if let Some(usage) = &transcript.usage {
        out.push_str(&format!("**Final usage**: `{usage}`\n"));
    }
    write_session_report(runtime_dir, &stem, &out)
}

fn render_report_header(
    out: &mut String,
    transcript: &SessionTranscript,
    log_path: &Path,
    runtime_dir: &Path,
    stem: &str,
) {
    let header = |key: &str| {
        transcript.header.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    };
    let exit = |key: &str| {
        transcript.exit.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    };
    let task = header("task").unwrap_or(stem);
    let state = header("state").unwrap_or("?");
    out.push_str(&format!("# {task} — {state}\n\n"));
    let identity = header("target").or_else(|| header("agent")).unwrap_or("?");
    out.push_str(&format!("**Agent**: {identity}"));
    if let Some(model) = header("model_name").or_else(|| header("model")) {
        out.push_str(&format!(" · **Model**: {model}"));
    }
    out.push('\n');
    if let Some(started) = header("started") {
        out.push_str(&format!("**Started**: {started}"));
    }
    match (exit("duration"), exit("code")) {
        (Some(duration), Some(code)) => {
            out.push_str(&format!(" · **Duration**: {duration} · **Exit**: {code}\n"));
        }
        // The log is the record; a missing footer is said, not inferred.
        // §FS-rhei-session-reports.5
        _ => out.push_str(" · **Exit**: session ended without an exit record\n"),
    }
    out.push_str(&format!(
        "**Log**: `{}`\n\n",
        session_log_reference(runtime_dir, log_path)
    ));
}

fn render_tool_call(
    out: &mut String,
    transcript: &SessionTranscript,
    full: bool,
    id: &str,
    name: &str,
    arguments: &serde_json::Value,
) {
    out.push_str(&format!("**{name}** `{}`\n\n", tool_argument_summary(arguments)));
    match transcript.results.get(id) {
        Some(result) => {
            let error_mark = if result.is_error { " (error)" } else { "" };
            out.push_str(&format!("<details><summary>output{error_mark}</summary>\n\n"));
            out.push_str(&fenced(&truncate_output(&result.text, full, Some(result.log_line))));
            out.push_str("\n</details>\n\n");
        }
        None => out.push_str("(no execution result recorded)\n\n"),
    }
    out.push_str("---\n\n");
}

/// Track paths written through editing tools, in first-write order.
fn note_written_file(
    files: &mut Vec<(String, Vec<(String, serde_json::Value)>)>,
    name: &str,
    arguments: &serde_json::Value,
) {
    const WRITING_TOOLS: &[&str] = &["write", "create", "edit", "multiedit"];
    if !WRITING_TOOLS.contains(&name) {
        return;
    }
    let Some(path) = arguments.get("path").and_then(|p| p.as_str()) else { return };
    let entry = match files.iter_mut().find(|(known, _)| known == path) {
        Some(entry) => entry,
        None => {
            files.push((path.to_string(), Vec::new()));
            files.last_mut().expect("just pushed")
        }
    };
    entry.1.push((name.to_string(), arguments.clone()));
}

/// The files-produced section: what this session wrote, from its tool-call
/// arguments — never from the current filesystem. §FS-rhei-session-reports.2
fn render_files_produced(out: &mut String, files: &[(String, Vec<(String, serde_json::Value)>)]) {
    if files.is_empty() {
        return;
    }
    out.push_str("## Files produced\n\n");
    for (path, operations) in files {
        let sequence: Vec<&str> =
            operations.iter().map(|(name, _)| name.as_str()).collect();
        out.push_str(&format!(
            "### `{path}`\n\n{} operation(s): {}\n\n",
            operations.len(),
            sequence.join(", ")
        ));
        let last_write = operations
            .iter()
            .rev()
            .find(|(name, _)| name == "write" || name == "create");
        if let Some((_, arguments)) = last_write {
            if let Some(content) = arguments.get("content").and_then(|c| c.as_str()) {
                out.push_str("<details><summary>final written content</summary>\n\n");
                out.push_str(&fenced(content));
                out.push_str("\n</details>\n\n");
                continue;
            }
        }
        out.push_str("<details><summary>edits</summary>\n\n");
        for (_, arguments) in operations {
            for edit in arguments
                .get("edits")
                .and_then(|edits| edits.as_array())
                .into_iter()
                .flatten()
            {
                let old = edit
                    .get("oldText")
                    .or_else(|| edit.get("old_string"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                let new = edit
                    .get("newText")
                    .or_else(|| edit.get("new_string"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                out.push_str("Replace:\n");
                out.push_str(&fenced(old));
                out.push_str("With:\n");
                out.push_str(&fenced(new));
            }
        }
        out.push_str("\n</details>\n\n");
    }
}

fn write_session_report(
    runtime_dir: &Path,
    stem: &str,
    content: &str,
) -> MietteResult<PathBuf> {
    let dir = session_reports_dir(runtime_dir);
    fs::create_dir_all(&dir)
        .map_err(|err| miette!(help = session_report_help(), "failed to create '{}': {err}", dir.display()))?;
    let path = dir.join(format!("{stem}.md"));
    fs::write(&path, content)
        .map_err(|err| miette!(help = session_report_help(), "failed to write '{}': {err}", path.display()))?;
    Ok(path)
}

fn fenced(text: &str) -> String {
    format!("````\n{}\n````\n", text.trim_end_matches('\n'))
}

fn truncate_output(text: &str, full: bool, log_line: Option<usize>) -> String {
    if full || text.len() <= SESSION_REPORT_OUTPUT_LIMIT {
        return text.to_string();
    }
    let mut cut = SESSION_REPORT_OUTPUT_LIMIT;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let source = match log_line {
        Some(line) => format!(" — full output in the log at line {line}"),
        None => " — full text in the log".to_string(),
    };
    format!("{}\n… [{} bytes truncated{source}]", &text[..cut], text.len() - cut)
}

/// The metrics strip: this session's place in a recorded iteration, when the
/// ledger has one. Reads records only. §FS-rhei-metrics.4
fn render_metrics_strip(out: &mut String, runtime_dir: &Path, log_path: &Path) {
    let log_reference = session_log_reference(runtime_dir, log_path);
    let metrics_dir = metrics_runtime_dir(runtime_dir);
    let Ok(entries) = fs::read_dir(&metrics_dir) else { return };
    let mut rows = String::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl")
            || path.file_name().and_then(|n| n.to_str()) == Some("pending-sessions.jsonl")
        {
            continue;
        }
        let records = read_metric_records(&path);
        for (index, record) in records.iter().enumerate() {
            if !record.sessions.iter().any(|session| session.log == log_reference) {
                continue;
            }
            let previous = index.checked_sub(1).and_then(|i| records.get(i));
            let label = record
                .label
                .clone()
                .or_else(|| {
                    path.file_stem().map(|stem| stem.to_string_lossy().into_owned())
                })
                .unwrap_or_default();
            let shared: Vec<String> = record
                .sessions
                .iter()
                .filter(|session| session.log != log_reference)
                .map(|session| format!("{} (visit {})", session.state, session.moves))
                .collect();
            let shared = if shared.is_empty() { "—".to_string() } else { shared.join(", ") };
            rows.push_str(&format!(
                "| {label} | {} | {} | {} | {shared} | `{}` |\n",
                previous.map_or("—".to_string(), format_metric_value),
                format_metric_value(record),
                format_metric_delta(previous, record),
                record.artifact
            ));
        }
    }
    if !rows.is_empty() {
        out.push_str("| Metric | Before | After | Δ | Shared with | Read from |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        out.push_str(&rows);
        out.push('\n');
    }
}

fn format_metric_value(record: &MetricIterationRecord) -> String {
    let unit = record.unit.as_deref().unwrap_or("");
    let value = match &record.value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    match &record.detail {
        Some(detail) => format!("{value}{unit} ({detail})"),
        None => format!("{value}{unit}"),
    }
}

fn format_metric_delta(
    previous: Option<&MetricIterationRecord>,
    current: &MetricIterationRecord,
) -> String {
    let (Some(previous), Some(before), Some(after)) =
        (previous, previous.and_then(|p| p.value.as_f64()), current.value.as_f64())
    else {
        return "—".to_string();
    };
    let _ = previous;
    let delta = ((after - before) * 10_000.0).round() / 10_000.0;
    let unit = current.unit.as_deref().unwrap_or("");
    if delta == 0.0 {
        return "= 0".to_string();
    }
    let improving = match current.goal.as_deref() {
        Some("decrease") => delta < 0.0,
        _ => delta > 0.0,
    };
    let arrow = if improving { "▲" } else { "▼" };
    let sign = if delta > 0.0 { "+" } else { "" };
    format!("{arrow} {sign}{delta}{unit}")
}

/// Regenerate `runtime/reports/metrics-summary.md` from the iteration records
/// alone; the records carry their own presentation facts. §FS-rhei-metrics.4
fn render_metrics_summary(runtime_dir: &Path) -> MietteResult<()> {
    let metrics_dir = metrics_runtime_dir(runtime_dir);
    let Ok(entries) = fs::read_dir(&metrics_dir) else { return Ok(()) };
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && path.file_name().and_then(|n| n.to_str())
                    != Some("pending-sessions.jsonl")
        })
        .collect();
    names.sort();
    let mut out = String::from("# Metrics\n\n");
    let mut any = false;
    for path in names {
        let records = read_metric_records(&path);
        let Some(first) = records.first() else { continue };
        any = true;
        let key = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let label = first.label.clone().unwrap_or_else(|| key.clone());
        out.push_str(&format!("## {label} — `{}`\n\n", first.task));
        out.push_str(&format!("Measured by: `{}`", first.measure_state));
        if let Some(goal) = &first.goal {
            out.push_str(&format!(" · goal: {goal}"));
        }
        out.push_str("\n\n| Pass | Sessions in window | Value | Δ | Read from |\n");
        out.push_str("|---|---|---|---|---|\n");
        for (index, record) in records.iter().enumerate() {
            let previous = index.checked_sub(1).and_then(|i| records.get(i));
            let who = if record.sessions.is_empty() {
                if record.iteration == 0 { "(baseline)".to_string() } else { "(none)".to_string() }
            } else {
                record
                    .sessions
                    .iter()
                    .map(|session| {
                        let stem = Path::new(&session.log)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        let emphasis = if session.driver { "**" } else { "" };
                        format!(
                            "{emphasis}[{} (visit {})](./{stem}.md){emphasis}",
                            session.state, session.moves
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!(
                "| {} | {who} | {} | {} | `{}` |\n",
                record.iteration,
                format_metric_value(record),
                format_metric_delta(previous, record),
                record.artifact
            ));
        }
        out.push('\n');
    }
    if !any {
        return Ok(());
    }
    let dir = session_reports_dir(runtime_dir);
    fs::create_dir_all(&dir)
        .map_err(|err| miette!(help = session_report_help(), "failed to create '{}': {err}", dir.display()))?;
    let path = dir.join("metrics-summary.md");
    fs::write(&path, &out)
        .map_err(|err| miette!(help = session_report_help(), "failed to write '{}': {err}", path.display()))
}
