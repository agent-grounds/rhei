/// The existing journal announces narrowed selection before scheduling or waiting.
/// Accept only a contiguous startup prefix identifying this active run; a missing,
/// torn, stale or oversized prefix supplies no scope evidence. Read at most 64 KiB.
/// §FS-rhei-transition-cmd.3.4
fn live_budget_scope(root: &Path, run_id: &str) -> Option<RheiScope> {
    use std::io::{BufRead, Read};
    use rhei_tui::RunEvent;

    let file = fs::File::open(rhei_tui::event_log_path(root)).ok()?;
    let mut reader = std::io::BufReader::new(file.take(64 * 1024));
    let mut scope = None;
    let mut line = String::new();
    for index in 0.. {
        line.clear();
        reader.read_line(&mut line).ok()?;
        if !line.ends_with('\n') {
            return None;
        }
        let record = rhei_tui::decode_event(&line)?;
        if record.seq != Some(index as u64 + 1) {
            return None;
        }
        match record.event {
            RunEvent::RunStarted { run_id: id, workspace, .. } if index == 0 => {
                if id != run_id || run_root_key(&workspace) != run_root_key(root) {
                    return None;
                }
            }
            _ if index == 0 => return None,
            RunEvent::Message { level: rhei_tui::MessageLevel::Info, text } => {
                if let Some(names) = text.strip_prefix("Scope: narrowed to ") {
                    scope = Some(names.split(", ").map(str::to_string).collect());
                }
                // A gate-only run can wait without ever starting a pass.
                if text.starts_with("Waiting for human gate decisions —")
                    || text.starts_with("No ready tasks; sleeping ")
                {
                    return Some(scope);
                }
            }
            RunEvent::PassStarted { .. } => return Some(scope),
            RunEvent::RunStarted { .. } | RunEvent::RunFinished { .. } => return None,
            _ => {}
        }
    }
    None
}

/// Read the authoritative descriptor without registry sweeps or root-lock writes.
/// The advisory retains its existing best-effort liveness rule. §FS-rhei-transition-cmd.3.4
fn live_descriptor_at(root: &Path) -> Option<RunDescriptor> {
    read_descriptor(&run_descriptor_path(root)).filter(|descriptor|
        run_root_key(&descriptor.workspace) == run_root_key(root)
            && matches!(descriptor.liveness(), Liveness::Live))
}

/// Operator diagnostics require ownership and descriptor identity to agree, as well
/// as the platform's existing process/lock liveness proof. §FS-rhei-transition-cmd.6
fn verified_operator_run(root: &Path) -> Option<RunDescriptor> {
    let owner = run_lock_owner(root).ok().flatten()?;
    let descriptor = live_descriptor_at(root)?;
    (owner.id == descriptor.id && owner.pid == descriptor.pid
        && run_root_key(&owner.workspace) == run_root_key(&descriptor.workspace))
        .then_some(descriptor)
}

/// Prove the offered path resolves through stop's registry-first access checks to
/// this exact live run, without pruning, guards or unsupported platform actions.
/// §FS-rhei-transition-cmd.3.4
fn live_run_stop_advice(root: &Path, descriptor: Option<&RunDescriptor>, wait: bool) -> String {
    let resolvable = descriptor.is_some_and(|run| {
        let Some(reference) = root.to_str() else { return false; };
        resolve_run_with_access(Some(reference), RunReadAccess::Observe).is_ok_and(|resolved|
            resolved.id == run.id && resolved.pid == run.pid
                && resolved.started_at == run.started_at
                && run_root_key(&resolved.workspace) == run_root_key(&run.workspace)
                && matches!(resolved.liveness(), Liveness::Live))
    });
    if resolvable && cfg!(unix) {
        let wait = if wait { " --wait" } else { "" };
        format!("stop the relevant run with `rhei stop {}{wait}`", shell_word(root))
    } else {
        let unavailable = if resolvable {
            "`rhei stop` is not supported on this platform"
        } else {
            "There is no resolvable live run descriptor"
        };
        format!("stop the run at {} through its original launcher or process controls \
            (interrupt the foreground run with Ctrl-C in its terminal; for a detached \
            run, identify its supervisor process before stopping it). {unavailable}; \
            once the run has stopped", root.display())
    }
}
