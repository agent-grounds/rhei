// `rhei report`: render session reports on demand — including for runs that
// predate the feature, since everything the renderer needs is in the logs and
// the recorded runtime artifacts.
//
// Its own part because the command is a thin walk over `runtime/logs/`; the
// parsing and rendering it calls live with their own concerns next door.

// §AR-source-file-size.3 §FS-rhei-session-reports.4

/// Render the matching session reports of one workspace, and refresh the
/// metrics summary when iteration records exist. Byte-identical output for
/// the same logs, whether triggered here or at session end.
// §FS-rhei-session-reports.4
fn report_command(
    input: &Path,
    task_filter: Option<&str>,
    state_filter: Option<&str>,
    full: bool,
) -> MietteResult<()> {
    let runtime_dir = resolve_report_runtime_dir(input)?;
    let logs_dir = runtime_dir.join("logs");
    let mut names: Vec<PathBuf> = fs::read_dir(&logs_dir)
        .map_err(|err| miette!(help = session_report_help(), "failed to read '{}': {err}", logs_dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("log"))
        .collect();
    names.sort();

    let mut rendered = 0usize;
    let mut skipped_programs = 0usize;
    for log_path in names {
        if !report_filters_match(&log_path, task_filter, state_filter) {
            continue;
        }
        // Program logs carry no event stream and are not rendered.
        // §FS-rhei-session-reports.1
        if !log_is_agent_session(&log_path) {
            skipped_programs += 1;
            continue;
        }
        let report = render_session_report(&log_path, &runtime_dir, full)?;
        println!("rendered {}", report.display());
        rendered += 1;
    }
    render_metrics_summary(&runtime_dir)?;

    if rendered == 0 {
        let filters = task_filter.is_some() || state_filter.is_some();
        return Err(miette!(
            help = "session reports render agent logs from runtime/logs/; program logs \
                    have no event stream and are skipped",
            "no agent session logs {} in '{}'{}",
            if filters { "matched" } else { "found" },
            logs_dir.display(),
            if skipped_programs > 0 {
                format!(" ({skipped_programs} program logs skipped)")
            } else {
                String::new()
            }
        ));
    }
    println!(
        "{rendered} session report(s) in {}",
        session_reports_dir(&runtime_dir).display()
    );
    Ok(())
}

/// The runtime tree a report request means: the workspace's `runtime/`, or
/// the given directory itself when it already is one.
fn resolve_report_runtime_dir(input: &Path) -> MietteResult<PathBuf> {
    let workspace_runtime = input.join("runtime");
    if workspace_runtime.join("logs").is_dir() {
        return Ok(workspace_runtime);
    }
    if input.join("logs").is_dir() {
        return Ok(input.to_path_buf());
    }
    Err(miette!(
        help = "pass a workspace directory holding runtime/logs/, or the runtime \
                directory itself",
        "no runtime/logs under '{}'",
        input.display()
    ))
}

fn report_filters_match(
    log_path: &Path,
    task_filter: Option<&str>,
    state_filter: Option<&str>,
) -> bool {
    let name = log_path.file_name().unwrap_or_default().to_string_lossy();
    task_filter.is_none_or(|task| name.contains(task))
        && state_filter.is_none_or(|state| name.contains(state))
}

/// Whether a log's first line marks an agent session rather than a program.
fn log_is_agent_session(log_path: &Path) -> bool {
    let Ok(file) = fs::File::open(log_path) else { return false };
    let mut first_line = String::new();
    let mut reader = BufReader::new(file);
    if reader.read_line(&mut first_line).is_err() {
        return false;
    }
    first_line.trim() == "=== rhei agent log v1 ==="
}
