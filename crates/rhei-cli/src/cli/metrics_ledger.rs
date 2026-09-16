// The engine-owned record of declared metrics: which measurement produced
// which value, and which sessions ran inside that measurement's window.
//
// Its own part because this is durable fact recorded at execution time — the
// same durability family as the transition ledger next door — while every
// presentation surface is a pure reader of the JSONL it appends. No reader
// re-derives the binding from transition history or file timestamps.

// §AR-source-file-size.3 §AR-agent-orchestrator-workflow.3.4 §FS-rhei-metrics.2
// §FS-rhei-metrics.3

/// One agent session noted for later attribution, as appended at session end.
// §FS-rhei-metrics.2
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct MetricPendingSession {
    task: String,
    state: String,
    moves: u64,
    attempt: u64,
    log: String,
    code: Option<i32>,
    ending: String,
}

/// One session inside an iteration's window, as the record stores it.
// §FS-rhei-metrics.3
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct MetricSessionRef {
    state: String,
    moves: u64,
    attempt: u64,
    driver: bool,
    log: String,
    ending: String,
    code: Option<i32>,
}

/// One confirmed measurement, one line of `runtime/metrics/<key>.jsonl`.
///
/// Presentation facts (`label`, `unit`, `goal`) ride along so the summary can
/// be regenerated from the record alone, without loading the machine.
// §FS-rhei-metrics.3
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct MetricIterationRecord {
    iteration: u64,
    value: serde_json::Value,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    artifact: String,
    measure_state: String,
    task: String,
    sessions: Vec<MetricSessionRef>,
    recorded: String,
    /// How many pending-ledger lines this metric has attributed so far; the
    /// next confirmation reads from here, so no session is counted twice.
    pending_consumed: u64,
}

fn metrics_runtime_dir(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("metrics")
}

fn metric_record_path(runtime_dir: &Path, metric_key: &str) -> PathBuf {
    metrics_runtime_dir(runtime_dir).join(format!("{metric_key}.jsonl"))
}

fn metric_pending_path(runtime_dir: &Path) -> PathBuf {
    metrics_runtime_dir(runtime_dir).join("pending-sessions.jsonl")
}

/// Serialize every metrics writer — pending appenders from worker threads and
/// the confirmation on the transition path — behind one lock file.
fn locked_metrics_dir(runtime_dir: &Path) -> std::io::Result<fs::File> {
    let dir = metrics_runtime_dir(runtime_dir);
    fs::create_dir_all(&dir)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.join(".lock"))?;
    lock.lock_exclusive()?;
    Ok(lock)
}

/// Note one finished agent session for later window attribution. Called only
/// when the machine declares metrics, and best-effort: a failed note is a
/// warning, never a failed run. §FS-rhei-metrics.2
#[allow(clippy::too_many_arguments)]
fn note_metric_pending_session(
    runtime_dir: &Path,
    task_id: &str,
    state: &str,
    moves: u64,
    attempt: u64,
    log_path: &Path,
    code: Option<i32>,
    ending: &str,
) {
    let note = MetricPendingSession {
        task: task_id.to_string(),
        state: state.to_string(),
        moves,
        attempt,
        log: session_log_reference(runtime_dir, log_path),
        code,
        ending: ending.to_string(),
    };
    let append = || -> std::io::Result<()> {
        let _lock = locked_metrics_dir(runtime_dir)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(metric_pending_path(runtime_dir))?;
        writeln!(file, "{}", serde_json::to_string(&note)?)?;
        file.flush()
    };
    if let Err(err) = append() {
        diag_warn!("could not note session for metrics attribution: {err}");
    }
}

/// After one agent session ended: render its report, and note it for metrics
/// attribution when the machine declares any. Best-effort epilogues — the log
/// is the record. §FS-rhei-session-reports.4 §FS-rhei-metrics.2
fn finish_agent_session_artifacts(
    metrics_declared: bool,
    runtime_dir: &Path,
    task_id: &str,
    state: &str,
    plan: &SpawnPlan,
    log_path: &Path,
    outcome: Option<&AgentSpawnOutcome>,
) {
    if log_path.is_file() {
        if let Err(err) = render_session_report(log_path, runtime_dir, false) {
            diag_warn!("could not render session report for {task_id}@{state}: {err}");
        }
    }
    let Some(outcome) = outcome else { return };
    if !metrics_declared {
        return;
    }
    let ending = if outcome.provider_limit.is_some() {
        "provider_limited"
    } else if outcome.timed_out {
        "timed out"
    } else if outcome.interrupted {
        "interrupted"
    } else {
        "exited"
    };
    note_metric_pending_session(
        runtime_dir,
        task_id,
        state,
        plan.moves,
        plan.attempt,
        log_path,
        outcome.status.code(),
        ending,
    );
}

/// A log path as records and reports spell it: relative to the workspace the
/// runtime tree belongs to, so the record survives moves and commits.
fn session_log_reference(runtime_dir: &Path, log_path: &Path) -> String {
    let workspace_root = runtime_dir.parent().unwrap_or(runtime_dir);
    log_path
        .strip_prefix(workspace_root)
        .unwrap_or(log_path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read_metric_records(path: &Path) -> Vec<MetricIterationRecord> {
    let Ok(raw) = fs::read_to_string(path) else { return Vec::new() };
    raw.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

fn read_pending_sessions(path: &Path) -> Vec<MetricPendingSession> {
    let Ok(raw) = fs::read_to_string(path) else { return Vec::new() };
    raw.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// Confirm measurements after a task left `from_state`: an existing boundary
/// artifact for the next iteration records it with its window; a missing one
/// is a failed measurement. Exit codes are never consulted. §FS-rhei-metrics.2
fn confirm_metric_iterations(
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    task_id: &str,
    from_state: &str,
) {
    if machine.metrics.is_empty() {
        return;
    }
    let runtime_dir = artifact_root.join("runtime");
    for (key, metric) in &machine.metrics {
        if !metric.measured_by.iter().any(|state| state == from_state) {
            continue;
        }
        if let Err(err) =
            confirm_one_metric(artifact_root, &runtime_dir, key, metric, task_id, from_state)
        {
            diag_warn!("could not record metric '{key}' iteration: {err}");
        }
    }
}

fn confirm_one_metric(
    artifact_root: &Path,
    runtime_dir: &Path,
    metric_key: &str,
    metric: &rhei_validator::MetricDef,
    task_id: &str,
    from_state: &str,
) -> MietteResult<()> {
    let _lock = locked_metrics_dir(runtime_dir)
        .map_err(|err| miette!(help = metric_value_help(), "failed to lock runtime/metrics: {err}"))?;
    let record_path = metric_record_path(runtime_dir, metric_key);
    let records = read_metric_records(&record_path);
    let next_iteration = records.last().map_or(0, |last| last.iteration + 1);
    let pending_cursor = records.last().map_or(0, |last| last.pending_consumed);

    let boundary_rel = render_iteration_template(&metric.artifact, next_iteration);
    let boundary = artifact_root.join(&boundary_rel);
    if !boundary.is_file() {
        // A failed measurement: no artifact, no iteration, no advance.
        return Ok(());
    }

    let (value, value_doc) =
        resolve_metric_value(artifact_root, metric, next_iteration, &boundary)?;
    let detail = metric.detail.as_ref().and_then(|template| {
        value_doc.as_ref().map(|doc| render_detail_template(template, doc, next_iteration))
    });

    let pending = read_pending_sessions(&metric_pending_path(runtime_dir));
    let pending_end = pending.len() as u64;
    let sessions = pending
        .iter()
        .skip(pending_cursor as usize)
        .filter(|session| session.task == task_id)
        .filter(|session| !metric.measured_by.iter().any(|state| state == &session.state))
        .map(|session| MetricSessionRef {
            state: session.state.clone(),
            moves: session.moves,
            attempt: session.attempt,
            driver: metric.drivers.iter().any(|state| state == &session.state),
            log: session.log.clone(),
            ending: session.ending.clone(),
            code: session.code,
        })
        .collect();

    let record = MetricIterationRecord {
        iteration: next_iteration,
        value,
        detail,
        unit: metric.unit.clone(),
        label: metric.label.clone(),
        goal: metric.goal.map(|goal| {
            match goal {
                rhei_validator::MetricGoal::Increase => "increase",
                rhei_validator::MetricGoal::Decrease => "decrease",
            }
            .to_string()
        }),
        artifact: boundary_rel,
        measure_state: from_state.to_string(),
        task: task_id.to_string(),
        sessions,
        recorded: format_iso8601_utc(std::time::SystemTime::now()),
        pending_consumed: pending_end,
    };
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&record_path)
        .map_err(|err| miette!(help = metric_value_help(), "failed to open '{}': {err}", record_path.display()))?;
    let line = serde_json::to_string(&record)
        .map_err(|err| miette!(help = metric_value_help(), "failed to serialize metric record: {err}"))?;
    writeln!(file, "{line}")
        .map_err(|err| miette!(help = metric_value_help(), "failed to append '{}': {err}", record_path.display()))?;
    file.flush().map_err(|err| miette!(help = metric_value_help(), "failed to flush '{}': {err}", record_path.display()))?;

    // Derived views refresh best-effort: the record is the fact, the views
    // are regenerable. §FS-rhei-session-reports.1 §FS-rhei-metrics.4
    if let Err(err) = render_metrics_summary(runtime_dir) {
        diag_warn!("could not refresh metrics summary: {err}");
    }
    for session in &record.sessions {
        let log_path = artifact_root.join(&session.log);
        if log_path.is_file() {
            if let Err(err) = render_session_report(&log_path, runtime_dir, false) {
                diag_warn!("could not refresh session report: {err}");
            }
        }
    }
    Ok(())
}

/// The iteration a run of `state` would confirm, when the machine has exactly
/// one metric measured by it; two make the number ambiguous, and the
/// environment then says nothing rather than guessing. §FS-rhei-metrics.2
fn next_metric_iteration(
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    state: &str,
) -> Option<u64> {
    let mut measuring = machine
        .metrics
        .iter()
        .filter(|(_, metric)| metric.measured_by.iter().any(|name| name == state));
    let (key, _) = measuring.next()?;
    if measuring.next().is_some() {
        return None;
    }
    let runtime_dir = workspace_root.join("runtime");
    let records = read_metric_records(&metric_record_path(&runtime_dir, key));
    Some(records.last().map_or(0, |last| last.iteration + 1))
}

fn render_iteration_template(template: &str, iteration: u64) -> String {
    template.replace(
        rhei_validator::METRIC_ITERATION_PLACEHOLDER,
        &iteration.to_string(),
    )
}

/// Resolve a metric's value: a JSON Pointer into the value document, or a
/// declared program's stdout. Returns the JSON document too when there is
/// one, for `detail` templating. §FS-rhei-metrics.1
fn resolve_metric_value(
    artifact_root: &Path,
    metric: &rhei_validator::MetricDef,
    iteration: u64,
    boundary: &Path,
) -> MietteResult<(serde_json::Value, Option<serde_json::Value>)> {
    if let Some(pointer) = &metric.pointer {
        let value_path = match &metric.value_artifact {
            Some(template) => artifact_root.join(render_iteration_template(template, iteration)),
            None => boundary.to_path_buf(),
        };
        let raw = fs::read_to_string(&value_path)
            .map_err(|err| miette!(help = metric_value_help(), "failed to read '{}': {err}", value_path.display()))?;
        let doc: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|err| miette!(help = metric_value_help(), "'{}' is not JSON: {err}", value_path.display()))?;
        let rendered_pointer = render_iteration_template(pointer, iteration);
        let value = doc.pointer(&rendered_pointer).cloned().ok_or_else(|| {
            miette!(help = metric_value_help(), "pointer '{rendered_pointer}' matches nothing in '{}'", value_path.display())
        })?;
        return Ok((value, Some(doc)));
    }
    let program = metric.program.as_ref().expect("validation kept one value source");
    let rendered = render_iteration_template(program, iteration);
    let output = rhei_core::platform::system_shell_command(&rendered)
        .current_dir(artifact_root)
        .env("RHEI_ITERATION", iteration.to_string())
        .output()
        .map_err(|err| miette!(help = metric_value_help(), "metric program failed to start: {err}"))?;
    if !output.status.success() {
        return Err(miette!(
            help = metric_value_help(),
            "metric program exited {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let value = match metric.kind {
        rhei_validator::MetricKind::Number => stdout
            .parse::<f64>()
            .ok()
            .and_then(|number| serde_json::Number::from_f64(number).map(serde_json::Value::Number))
            .ok_or_else(|| miette!(help = metric_value_help(), "metric program printed '{stdout}', not a number"))?,
        rhei_validator::MetricKind::String => serde_json::Value::String(stdout),
    };
    Ok((value, None))
}

/// Replace `{/json/pointer}` segments with values from the document.
fn render_detail_template(
    template: &str,
    doc: &serde_json::Value,
    iteration: u64,
) -> String {
    let mut out = String::new();
    let mut rest = render_iteration_template(template, iteration);
    while let Some(start) = rest.find("{/") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let pointer = &rest[start + 1..start + end];
        match doc.pointer(pointer) {
            Some(serde_json::Value::String(text)) => out.push_str(text),
            Some(value) => out.push_str(&value.to_string()),
            None => out.push_str("(unresolved)"),
        }
        rest = rest[start + end + 1..].to_string();
    }
    out.push_str(&rest);
    out
}
