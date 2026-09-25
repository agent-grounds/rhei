// The metric surfaces a session report shares a feature with: the strip in
// each session report and the run-level metrics summary — pure readers of the
// iteration records the ledger appended.
//
// Its own part because metric presentation is owned by §FS-rhei-metrics.4 and
// changes with the metric record, not with how a session's log is laid out.

// §AR-source-file-size.3 §FS-rhei-metrics.4

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
                .map(metric_session_label)
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

/// A session as the metric surfaces name it: by its visit identity, the
/// number its log name carries (`cover #2`), never by the iteration it is
/// bound to. A record written before visits were recorded names the ledger
/// move instead, and says so. §FS-rhei-metrics.4
fn metric_session_label(session: &MetricSessionRef) -> String {
    match session.visit {
        Some(visit) => format!("{} #{visit}", session.state),
        None => format!("{} (move {})", session.state, session.moves),
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
                            "{emphasis}[{}](./{stem}.md){emphasis}",
                            metric_session_label(session)
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
