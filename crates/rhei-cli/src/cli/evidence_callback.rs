// The evidence of one callback process: written only into a live run, under a
// fresh `callback_id`, naming the process it ran inside. §FS-rhei-evidence.2.2

impl EvidenceRun {
    fn routed_exits(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        self.routed.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

/// The run a transition on `workspace_root` records into: this process's own
/// run when it records that workspace's evidence root, else that root's live
/// run, else none. The flag says whether the run is this process.
/// §FS-rhei-evidence.2.2
fn callback_evidence_run(workspace_root: &Path) -> Option<(Arc<EvidenceRun>, bool)> {
    let root = evidence_root_for(workspace_root);
    if let Some(run) = evidence_run().filter(|run| run.root == root) {
        return Some((run, true));
    }
    let descriptor = read_descriptor(&run_descriptor_path(&root))?;
    if !matches!(descriptor.liveness(), Liveness::Live) {
        return None;
    }
    let settings = load_merged_settings(&root).ok()?;
    let provenance = rhei_tui::RunProvenance::default();
    let run = EvidenceRun::new(&root, &descriptor.id, &descriptor.started_at, &provenance, settings);
    Some((Arc::new(run), false))
}

/// Holds the exit a transition routes until the transition ends, so every
/// callback on the edge names it and no later transition does.
/// §FS-rhei-evidence.2.2
struct RoutedExitScope {
    task_id: String,
}

impl RoutedExitScope {
    fn new(task_id: &str) -> Self {
        RoutedExitScope { task_id: task_id.to_string() }
    }
}

impl Drop for RoutedExitScope {
    fn drop(&mut self) {
        if let Some(run) = evidence_run() {
            run.routed_exits().remove(&self.task_id);
        }
    }
}

/// Every start record of `task_id` in the run's directory with no end
/// record, sorted. §FS-rhei-evidence.2.2
fn open_invocation_ids(run: &EvidenceRun, task_id: &str) -> Vec<String> {
    let dir = run.run_dir();
    let mut open: Vec<String> = fs::read_dir(&dir)
        .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.strip_suffix(".start.json")
                .is_some_and(|key| !dir.join(format!("{key}.end.json")).exists())
        })
        .filter_map(|path| serde_json::from_slice::<serde_json::Value>(&fs::read(path).ok()?).ok())
        .filter(|record| record["kind"] != "callback" && record["task_id"] == task_id)
        .filter_map(|record| record["invocation_id"].as_str().map(str::to_string))
        .collect();
    open.sort();
    open.dedup();
    open
}

/// What a callback's start record is placed by. The firing id is not here:
/// it is never written. §FS-rhei-evidence.2.2 §FS-rhei-transitions.1.2
struct CallbackEvidenceStart<'a> {
    workspace_root: &'a Path,
    artifact_root: &'a Path,
    task_id: &'a str,
    from: &'a str,
    to: &'a str,
    phase: &'static str,
    callback: &'a rhei_core::ast::CallbackRef,
    bound: Option<&'a rhei_core::callback::CallbackBound>,
    cwd: &'a Path,
}

/// Publish a callback's start record when a live run covers the transition;
/// `None` when none does. A record that cannot be written refuses the
/// callback. §FS-rhei-evidence.2.2 §FS-rhei-evidence.3.3
fn begin_callback_evidence(start: CallbackEvidenceStart<'_>) -> MietteResult<Option<EvidenceInvocation>> {
    let Some(command) = start.callback.0.strip_prefix("cli:") else { return Ok(None) };
    let Some((run, in_process)) = callback_evidence_run(start.workspace_root) else { return Ok(None) };
    // The orchestrator routes an exit in its own process; every callback of
    // that transition names it. §FS-rhei-evidence.2.2
    let routed = if in_process { run.routed_exits().get(start.task_id).cloned() } else { None };
    let owners = routed.map_or_else(|| open_invocation_ids(&run, start.task_id), |id| vec![id]);
    let mut cmd = rhei_core::platform::system_shell_command(command);
    cmd.current_dir(start.cwd);
    let moves = ticket_move_count(start.artifact_root, &start.workspace_root.join("runtime"), start.task_id);
    let config = serde_json::json!({
        "declaration": start.callback.0,
        "callback_timeout": start.bound.map(|bound| bound.authored.clone()),
    });
    let callback_id = uuid::Uuid::new_v4().to_string();
    let record = serde_json::json!({
        "kind": "callback",
        "callback_id": callback_id,
        "owner_invocation_ids": owners,
        "task_id": start.task_id,
        "state": if start.phase == "on_leave" { start.from } else { start.to },
        "from": start.from,
        "to": start.to,
        "callback_phase": start.phase,
        "moves": moves,
        "argv": evidence_argv(&cmd, None),
        "cwd": evidence_cwd(&cmd),
        "config": evidence_config(&run, config),
        "seal": evidence_seal(&run, start.task_id),
    });
    EvidenceInvocation::publish(run, "callback", "callback_id", callback_id, record).map(Some)
}

/// Close a callback's record with its exit; a callback has no log.
/// §FS-rhei-evidence.4
fn finish_callback_evidence(
    evidence: Option<EvidenceInvocation>,
    result: Option<&rhei_core::callback::CallbackResult>,
) {
    let Some(evidence) = evidence else { return };
    let exit_code = result.and_then(|result| result.exit_code);
    let timed_out = result.is_some_and(|result| {
        exit_code.is_none() && result.error.as_deref().is_some_and(|e| e.contains("callback_timeout"))
    });
    evidence.finish(exit_code, if timed_out { "timed out" } else { "exited" }, None, None);
}
