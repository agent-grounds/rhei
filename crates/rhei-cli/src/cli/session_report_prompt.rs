// The prompt record: the exact prompt `rhei run` composed for one agent spawn,
// written beside its log so the session report can show it verbatim.
//
// Its own part because it is the one capture the report feature adds: written
// on the spawn path, read on the render path, and never rebuilt — the prompt
// builder reads state later sessions change.

// §AR-source-file-size.3 §FS-rhei-session-reports.1.1

/// `runtime/prompts/<log stem>.md`: one record per log, attempt suffix
/// included, beside `runtime/logs/` rather than in it, so the logs directory
/// holds transcripts and nothing else. §FS-rhei-session-reports.1.1
fn session_prompt_record_path(log_path: &Path) -> PathBuf {
    let stem = log_path.file_stem().unwrap_or_default().to_string_lossy();
    let runtime_dir = log_path.parent().and_then(Path::parent).unwrap_or(Path::new("."));
    runtime_dir.join("prompts").join(format!("{stem}.md"))
}

/// Record the prompt this spawn composed, before the subprocess starts. The
/// log is the record of what ran; a failed write warns and never fails the
/// spawn. §FS-rhei-session-reports.1.1
fn write_session_prompt_record(log_path: &Path, prompt: &str) {
    let path = session_prompt_record_path(log_path);
    let written = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&path, prompt));
    if let Err(err) = written {
        diag_warn!("could not write the prompt record '{}': {err}", path.display());
    }
}

/// The recorded prompt of one log; `None` for a log written before records
/// existed. §FS-rhei-session-reports.2
fn read_session_prompt_record(log_path: &Path) -> Option<String> {
    fs::read_to_string(session_prompt_record_path(log_path)).ok()
}
