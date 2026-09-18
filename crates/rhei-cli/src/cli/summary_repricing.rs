// Completed-run selection for the read-only alternate-price summary. The
// accounting reader owns record/root identity; this layer adds durable
// completion evidence and keeps project-wide discovery bounded to the already
// resolved Panta project.
//
// §FS-rhei-summary.1 §FS-rhei-summary.5 §FS-rhei-panta.6.5

/// Aggregate stored measurements after restating them and applying one
/// explicit replacement book. Durable pricing and historical books are unused.
/// §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.5.2
fn summarize_records_with_price_book<'a>(
    records: impl IntoIterator<Item = ScopedRecord<'a>>,
    price_book: &PriceBook,
) -> Option<rhei_tui::AccountingRunSummary> {
    let readings: Vec<RecordReading> = records
        .into_iter()
        .map(|held| read_stored_record_with_price_book(held.record, price_book))
        .collect();
    let usages: Vec<rhei_tui::UsageSummary> =
        readings.iter().map(usage_summary_from_reading).collect();
    let summary = rhei_tui::summarize_usage_summaries(usages.iter())?;
    Some(demote_if(summary, readings.iter().any(|reading| reading.convention_unknown)))
}

/// Select one exact completed run and reproject its measurements through the
/// caller's book. The returned inspection is the single source for the lead,
/// steps, and accounting table.
/// §FS-rhei-summary.2 §FS-rhei-cost-accounting.6.1
fn select_completed_run_for_summary(
    loaded: &LoadedPlan,
    run_root: &Path,
    scope: &RheiScope,
    roots: &[AccountingRoot],
    mut inspection: CostInspection,
    run_id: &str,
    price_book: &PriceBook,
) -> MietteResult<CostInspection> {
    if !inspection_has_run(&inspection, run_id) {
        let project_roots = accounting_roots(loaded, run_root, &None);
        let project = if scope.is_none() {
            inspection.clone()
        } else {
            read_cost_inspection_over(&project_roots, &None)
        };
        if inspection_has_run(&project, run_id) {
            let selected = scope
                .as_ref()
                .map(|ids| ids.iter().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "the resolved project".to_string());
            return Err(miette!(
                help = "select the owning rhei with --rhei, or summarize the whole project",
                "run '{run_id}' is outside the selected rhei scope ({selected})"
            ));
        }
        if project.invocations.iter().any(|held| {
            held.record.run_id.as_deref().is_some_and(|candidate| candidate.starts_with(run_id))
        }) {
            return Err(miette!(
                help = "copy the complete id from `rhei cost --by run` or `rhei runs --all`",
                "run id '{run_id}' is not an exact match"
            ));
        }
        return Err(miette!(
            help = "list durable ids with `rhei cost --by run` or `rhei runs --all`",
            "run '{run_id}' is absent from the resolved project"
        ));
    }

    if selected_roots_show_activity(roots, run_id) {
        return Err(miette!(
            help = "wait for the run to finish before applying an alternate price book",
            "run '{run_id}' is active"
        ));
    }

    let reports = immutable_reports_for_run(roots, run_id);
    match reports.len() {
        0 => {
            return Err(miette!(
                help = "an immutable timestamped report under runtime/run-reports must record this exact run",
                "completion is not established for run '{run_id}'"
            ));
        }
        1 => {}
        count => {
            return Err(miette!(
                help = "retain one authoritative immutable report for this exact run id",
                "run '{run_id}' is ambiguous: {count} immutable reports claim it"
            ));
        }
    }

    // Reuse the ordinary Run-axis selection so unattributed records and an
    // unreadable root demote coverage exactly as they do for `rhei cost`.
    // §FS-rhei-cost-accounting.6.1 §FS-rhei-cost-accounting.6.2
    let summary = {
        let selection = CostSelection::resolve(Some(run_id), None, None)?
            .apply(inspection.scoped(), inspection.unreadable_root);
        summarize_records_with_price_book(selection.records.iter().copied(), price_book)
            .map(|summary| demote_if(summary, selection.is_uncertain()))
    };

    inspection
        .invocations
        .retain(|held| held.record.run_id.as_deref() == Some(run_id));
    inspection.summary = summary;
    Ok(inspection)
}

/// Whether the scoped accounting union contains the exact durable id.
/// §FS-rhei-cost-accounting.6.1
fn inspection_has_run(inspection: &CostInspection, run_id: &str) -> bool {
    inspection
        .invocations
        .iter()
        .any(|held| held.record.run_id.as_deref() == Some(run_id))
}

/// A descriptor still recorded as running is active evidence. A held lock
/// carrying this exact id is independent evidence, including when a terminal
/// descriptor sits beside it.
/// §FS-rhei-summary.1 §FS-rhei-summary.5
fn selected_roots_show_activity(roots: &[AccountingRoot], run_id: &str) -> bool {
    execution_roots(roots).into_iter().any(|root| {
        let descriptor = read_descriptor(&run_descriptor_path(&root));
        let live_descriptor = descriptor.as_ref().is_some_and(|descriptor| {
            descriptor.id == run_id && !descriptor.status.is_terminal()
        });
        live_descriptor || held_lock_names_run(&root, run_id)
    })
}

/// Require both exact-id ownership text and a contended lock; an unlocked
/// historical owner record is not active evidence.
/// §FS-rhei-summary.1 §FS-rhei-summary.5
fn held_lock_names_run(root: &Path, run_id: &str) -> bool {
    let path = root.join(".rhei/run.lock");
    let names_run = fs::read_to_string(&path)
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|value| value.get("id").and_then(serde_json::Value::as_str).map(str::to_string))
        .as_deref()
        == Some(run_id);
    names_run && matches!(probe_run_lock(root), RunLockProbe::Held)
}

/// Find timestamped history reports that explicitly claim the exact run id.
/// The overwriteable `run-report.md` is never inspected.
/// §FS-rhei-run-report.1 §FS-rhei-summary.1
fn immutable_reports_for_run(roots: &[AccountingRoot], run_id: &str) -> Vec<PathBuf> {
    let mut reports = Vec::new();
    let mut seen = BTreeSet::new();
    for root in execution_roots(roots) {
        let directory = root.join("runtime/run-reports");
        let Ok(entries) = fs::read_dir(directory) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(OsStr::to_str) != Some("md") {
                continue;
            }
            let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.contains(&key) {
                continue;
            }
            let claims_run = fs::read_to_string(&path)
                .ok()
                .is_some_and(|body| report_claims_run(&body, run_id));
            if claims_run {
                seen.insert(key);
                reports.push(path);
            }
        }
    }
    reports
}

/// Match the report's authoritative `Run:` header, never its filename prefix.
/// §FS-rhei-run-report.1
fn report_claims_run(body: &str, run_id: &str) -> bool {
    body.lines().any(|line| {
        line.strip_prefix("Run: ")
            .and_then(|run| run.rsplit_once(" / "))
            .is_some_and(|(_, id)| id.trim() == run_id)
    })
}

/// Recover selected execution roots from their `runtime/accounting` paths and
/// deduplicate aliases before inspecting completion or liveness artifacts.
/// §FS-rhei-panta.6.5
fn execution_roots(roots: &[AccountingRoot]) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    roots
        .iter()
        .filter_map(|root| root.path.parent().and_then(Path::parent).map(Path::to_path_buf))
        .filter(|root| seen.insert(fs::canonicalize(root).unwrap_or_else(|_| root.clone())))
        .collect()
}
