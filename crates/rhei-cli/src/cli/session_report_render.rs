// One readable Markdown report per agent session log: pure formatting over
// the transcript the parser read, with the metrics strip and summary next
// door.
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

/// One editing step on one path, as the files recap names it.
// §FS-rhei-session-reports.2
struct FileOperation {
    /// The lowercase tool name, or a Codex change kind.
    kind: String,
    arguments: serde_json::Value,
    step: usize,
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
    // The prompt this spawn was given, never a rebuild; a log written before
    // records existed falls back to its stream's echo. §FS-rhei-session-reports.1.1
    let recorded_prompt = read_session_prompt_record(log_path);
    let prompt = recorded_prompt.as_deref().or(transcript.prompt.as_deref());
    let mut out = String::new();

    render_report_header(&mut out, &transcript, log_path, runtime_dir, &stem);
    render_metrics_strip(&mut out, runtime_dir, log_path);

    if let Some(notice) = &transcript.unsupported_stream {
        render_prompt(&mut out, prompt, false);
        out.push_str(&format!("> {notice}\n"));
        return write_session_report(runtime_dir, &stem, &out);
    }

    // A body with no event stream is the agent's plain output as captured;
    // it renders verbatim, not as an empty report.
    // §FS-rhei-session-reports.6.4
    if let Some(output) = &transcript.plain_output {
        render_prompt(&mut out, prompt, false);
        out.push_str("## Session output\n\n");
        out.push_str(&fenced(&truncate_output(output, full, None)));
        return write_session_report(runtime_dir, &stem, &out);
    }

    render_prompt(&mut out, prompt, true);
    out.push_str("## Agent actions\n\n");
    let root = session_root(&transcript);
    let mut files: Vec<(String, Vec<FileOperation>)> = Vec::new();
    let mut step = 0usize;
    for event in &transcript.events {
        match event {
            SessionEvent::Thinking(text) => {
                out.push_str("<details><summary><i>Agent thinking</i></summary>\n\n");
                out.push_str(&fenced(&truncate_output(text, full, None)));
                out.push_str("\n</details>\n\n");
            }
            SessionEvent::Text(text) => out.push_str(&quote_agent_text(text)),
            SessionEvent::ToolCall { id, name, arguments } => {
                step += 1;
                render_tool_step(&mut out, &transcript, full, step, id, name, arguments);
                note_written_file(&mut files, name, arguments, step);
            }
        }
    }

    render_files_changed(&mut out, &files, root);
    // The last usage the stream reported: one message's or turn's for most
    // streams, the session total only where a result envelope carried it.
    // §FS-rhei-session-reports.6.2
    if let Some(usage) = &transcript.usage {
        out.push_str(&format!("## Outcome\n\n**Last reported usage**: `{usage}`\n"));
    }
    write_session_report(runtime_dir, &stem, &out)
}

/// The prompt section. An event-stream report always has one, saying so when
/// no prompt was recorded; a report of plain output or of an unsupported
/// stream shows it only when there is one. §FS-rhei-session-reports.2
fn render_prompt(out: &mut String, prompt: Option<&str>, always: bool) {
    let Some(prompt) = prompt.or(always.then_some("(no prompt recorded)")) else { return };
    out.push_str("## Prompt\n\n<details><summary>full prompt</summary>\n\n");
    out.push_str(&fenced(prompt));
    out.push_str("\n</details>\n\n");
}

/// The agent's own words, set apart from tool traffic as a labelled quote.
/// §FS-rhei-session-reports.2
fn quote_agent_text(text: &str) -> String {
    let mut out = String::from("> **Agent**\n>\n");
    for line in text.trim_end().lines() {
        if line.is_empty() {
            out.push_str(">\n");
        } else {
            out.push_str(&format!("> {line}\n"));
        }
    }
    out.push('\n');
    out
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

/// One tool call as a numbered step: which tool and how it ended, the
/// argument that identifies it, labelled, and its output, labelled with the
/// same step number so no output can be read as another call's.
/// §FS-rhei-session-reports.2
fn render_tool_step(
    out: &mut String,
    transcript: &SessionTranscript,
    full: bool,
    step: usize,
    id: &str,
    name: &str,
    arguments: &serde_json::Value,
) {
    let result = transcript.results.get(id);
    let status = match result {
        Some(result) if result.is_error => "error",
        Some(_) => "ok",
        None => "no result recorded",
    };
    out.push_str(&format!("### Step {step} · {name} — {status}\n\n"));
    let (label, value) = tool_argument_identity(arguments);
    let value = relative_to_root(&value, session_root(transcript));
    if value.is_empty() {
        out.push_str(&format!("**{label}:** (none)\n\n"));
    } else if value.contains('\n') || value.contains('`') {
        out.push_str(&format!("**{label}:**\n\n"));
        out.push_str(&fenced(&value));
        out.push('\n');
    } else {
        out.push_str(&format!("**{label}:** `{value}`\n\n"));
    }
    let Some(result) = result else { return };
    let size = match result.text.lines().count() {
        0 => "empty".to_string(),
        1 => "1 line".to_string(),
        lines => format!("{lines} lines"),
    };
    out.push_str(&format!(
        "<details><summary><b>Output of step {step}</b> · {size}</summary>\n\n"
    ));
    out.push_str(&fenced(&truncate_output(&result.text, full, Some(result.log_line))));
    out.push_str("\n</details>\n\n");
}

/// Track paths written through editing tools, in first-write order, with the
/// step that wrote them. Tool names arrive as each CLI spells them (`write`,
/// `Write`, `MultiEdit`), so recording compares the lowercase spelling.
fn note_written_file(
    files: &mut Vec<(String, Vec<FileOperation>)>,
    name: &str,
    arguments: &serde_json::Value,
    step: usize,
) {
    const WRITING_TOOLS: &[&str] = &["write", "create", "edit", "multiedit"];
    let name = name.to_ascii_lowercase();
    // A Codex file-change item names the changed paths and the change kind,
    // and carries no content. A deletion is not a changed file to recap: it
    // stays in the actions timeline only. §FS-rhei-session-reports.6.3
    if name == "file_change" {
        for change in
            arguments.get("changes").and_then(|c| c.as_array()).into_iter().flatten()
        {
            let Some(path) = change.get("path").and_then(|p| p.as_str()) else { continue };
            let kind = change.get("kind").and_then(|k| k.as_str()).unwrap_or("change");
            if kind == "delete" {
                continue;
            }
            written_file_entry(files, path).push(FileOperation {
                kind: kind.to_string(),
                arguments: serde_json::Value::Null,
                step,
            });
        }
        return;
    }
    if !WRITING_TOOLS.contains(&name.as_str()) {
        return;
    }
    let Some(path) = arguments
        .get("path")
        .or_else(|| arguments.get("file_path"))
        .and_then(|p| p.as_str())
    else {
        return;
    };
    written_file_entry(files, path).push(FileOperation {
        kind: name,
        arguments: arguments.clone(),
        step,
    });
}

fn written_file_entry<'a>(
    files: &'a mut Vec<(String, Vec<FileOperation>)>,
    path: &str,
) -> &'a mut Vec<FileOperation> {
    match files.iter().position(|(known, _)| known == path) {
        Some(index) => &mut files[index].1,
        None => {
            files.push((path.to_string(), Vec::new()));
            &mut files.last_mut().expect("just pushed").1
        }
    }
}

/// How the recap names what a step did to a file: `written in step 5`,
/// `written in step 2, edited in steps 4, 6`.
fn file_operation_summary(operations: &[FileOperation]) -> String {
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    for operation in operations {
        let verb = match operation.kind.as_str() {
            "write" => "written",
            "create" => "created",
            "edit" | "multiedit" => "edited",
            "add" => "added",
            "update" => "updated",
            other => other,
        };
        match groups.last_mut() {
            Some((last, steps)) if *last == verb => steps.push(operation.step),
            _ => groups.push((verb, vec![operation.step])),
        }
    }
    groups
        .iter()
        .map(|(verb, steps)| {
            let noun = if steps.len() == 1 { "step" } else { "steps" };
            let list: Vec<String> = steps.iter().map(ToString::to_string).collect();
            format!("{verb} in {noun} {}", list.join(", "))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The files-changed recap: what this session wrote, from its tool-call
/// arguments — never from the current filesystem — each file naming the
/// steps above that wrote it. §FS-rhei-session-reports.2
fn render_files_changed(
    out: &mut String,
    files: &[(String, Vec<FileOperation>)],
    root: Option<&str>,
) {
    if files.is_empty() {
        return;
    }
    out.push_str("## Files changed by this session\n\n");
    out.push_str("_A recap of the editing steps above, grouped by file._\n\n");
    for (path, operations) in files {
        out.push_str(&format!(
            "### `{}` — {}\n\n",
            relative_to_root(path, root),
            file_operation_summary(operations)
        ));
        let last_write = operations
            .iter()
            .rev()
            .find(|operation| operation.kind == "write" || operation.kind == "create");
        if let Some(operation) = last_write {
            if let Some(content) = operation.arguments.get("content").and_then(|c| c.as_str()) {
                out.push_str(&format!(
                    "<details><summary>content written in step {}</summary>\n\n",
                    operation.step
                ));
                out.push_str(&fenced(content));
                out.push_str("\n</details>\n\n");
                continue;
            }
        }
        let pairs: Vec<(usize, &str, &str)> = operations
            .iter()
            .flat_map(|operation| {
                edit_pairs(&operation.arguments)
                    .into_iter()
                    .map(move |(old, new)| (operation.step, old, new))
            })
            .collect();
        // Operations without recorded content — a Codex file change — list
        // the path and the steps alone. §FS-rhei-session-reports.6.3
        if pairs.is_empty() {
            continue;
        }
        out.push_str("<details><summary>edits</summary>\n\n");
        for (step, old, new) in pairs {
            out.push_str(&format!("Step {step} replaced:\n"));
            out.push_str(&fenced(old));
            out.push_str("with:\n");
            out.push_str(&fenced(new));
        }
        out.push_str("\n</details>\n\n");
    }
}

/// The old/new pairs one editing call records: a MultiEdit-style `edits`
/// list, or the single pair the arguments carry directly.
fn edit_pairs(arguments: &serde_json::Value) -> Vec<(&str, &str)> {
    fn one_pair(edit: &serde_json::Value) -> Option<(&str, &str)> {
        let old = edit
            .get("oldText")
            .or_else(|| edit.get("old_string"))
            .and_then(|t| t.as_str());
        let new = edit
            .get("newText")
            .or_else(|| edit.get("new_string"))
            .and_then(|t| t.as_str());
        if old.is_none() && new.is_none() {
            return None;
        }
        Some((old.unwrap_or(""), new.unwrap_or("")))
    }
    match arguments.get("edits").and_then(|edits| edits.as_array()) {
        Some(edits) => edits.iter().filter_map(one_pair).collect(),
        None => one_pair(arguments).into_iter().collect(),
    }
}

/// The directory the session worked in, as the log header records it: the
/// worktree when the session had one, the checkout otherwise.
fn session_root(transcript: &SessionTranscript) -> Option<&str> {
    let header = |key: &str| {
        transcript.header.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    };
    header("worktree_root").or_else(|| header("checkout_root"))
}

/// A path the agent spelled absolutely, shown relative to the session's own
/// root so a report stays readable and portable; anything else is unchanged.
/// §FS-rhei-session-reports.1
fn relative_to_root(path: &str, root: Option<&str>) -> String {
    let Some(root) = root.map(|root| root.trim_end_matches('/')).filter(|r| !r.is_empty()) else {
        return path.to_string();
    };
    match path.strip_prefix(root).and_then(|rest| rest.strip_prefix('/')) {
        Some(relative) if !relative.is_empty() => relative.to_string(),
        _ => path.to_string(),
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
