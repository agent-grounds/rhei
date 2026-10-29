// A narrowed reset keeps the evidence store and records what it removed, so
// verify can tell a reset's removal from a loss. §FS-rhei-evidence.10.1

const EVIDENCE_EVENT_SCHEMA: &str = "rhei.evidence.event.v1";

/// The run ids with a directory under `runs/` now, sorted.
fn evidence_run_ids(root: &Path) -> Vec<String> {
    let mut runs: Vec<String> = fs::read_dir(evidence_dir(root).join("runs"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    runs.sort();
    runs
}

/// The reset event document. Ordering comes from `runs`, never from `at`.
/// §FS-rhei-evidence.10.1
fn reset_event_value(
    event_id: &str,
    rheis: Vec<String>,
    tickets: Vec<String>,
    runs: Vec<String>,
    removed: Vec<String>,
) -> serde_json::Value {
    serde_json::json!({
        "schema": EVIDENCE_EVENT_SCHEMA,
        "event_id": event_id,
        "kind": "reset",
        "at": rhei_tui::format_rfc3339(std::time::SystemTime::now()),
        "rheis": rheis,
        "tickets": tickets,
        "runs": runs,
        "removed": removed,
    })
}

/// Write one reset event into the run root's store, when there is a store to
/// keep; returns its id. §FS-rhei-evidence.10.1
fn write_reset_event(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    removed: &[PathBuf],
) -> Option<String> {
    let root = execution_workspace_root(input);
    let root = rhei_core::platform::canonical_path(&root).unwrap_or(root);
    if !evidence_dir(&root).is_dir() {
        return None;
    }
    let mut tickets: Vec<String> =
        loaded.task_roots.keys().filter(|id| task_in_rhei_scope(scope, id)).cloned().collect();
    tickets.sort();
    let rheis: Vec<String> = scope.iter().flatten().cloned().collect();
    let removed: BTreeSet<String> = removed
        .iter()
        .map(|path| evidence_relative(&root, path))
        .filter(|path| !Path::new(path).is_absolute())
        .collect();
    let event_id = uuid::Uuid::new_v4().to_string();
    let event = reset_event_value(
        &event_id,
        rheis,
        tickets,
        evidence_run_ids(&root),
        removed.into_iter().collect(),
    );
    let path = evidence_dir(&root).join("events").join(format!("{event_id}.json"));
    match write_evidence_exclusively(&path, &event) {
        Ok(()) => Some(event_id),
        Err(err) => {
            eprintln!("warning: could not write the reset event {}: {err}", path.display());
            None
        }
    }
}
