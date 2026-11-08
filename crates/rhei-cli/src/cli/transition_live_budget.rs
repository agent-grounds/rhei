/// A successful gate release does not prove adoption of the disk machine.
/// Detection is best-effort and read-only: uncertainty cannot refuse a release.
/// §FS-rhei-transition-cmd.3.4
fn advise_live_gate_budget(
    input: &Path,
    route: &TaskRoute,
    machine: &rhei_validator::StateMachine,
    task_id: &str,
    from: &str,
    to: &str,
) {
    if !machine.states.get(from).is_some_and(|state| state.gating)
        || !machine.states.get(to).is_some_and(|state| state.visits.is_some())
    {
        return;
    }

    let project = workspace::panta_project_dir(input)
        .or_else(|| workspace::panta_member(input).map(|(project, _)| project));
    let in_project = project.is_some();
    let member = run_root_key(&route.execution_root);
    let roots = project.into_iter().map(|root| run_root_key(&root)).chain([member]);
    for root in roots {
        let descriptor = read_descriptor(&run_descriptor_path(&root)).filter(|descriptor| {
            run_root_key(&descriptor.workspace) == root
                && matches!(descriptor.liveness(), Liveness::Live)
        });
        if !matches!(probe_run_lock(&root), RunLockProbe::Held) && descriptor.is_none() {
            continue;
        }

        let run_id = descriptor.as_ref().map(|run| run.id.clone())
            .or_else(|| run_lock_owner_id(&root).ok().flatten());
        // Shared roots prove activity, not membership in the scheduler's selection.
        // Require this run's complete startup journal before advising a project release.
        // §FS-rhei-transition-cmd.3.4
        let scope = if in_project {
            let Some(scope) = run_id.as_deref().and_then(|id| live_budget_scope(&root, id)) else {
                continue;
            };
            if !task_in_rhei_scope(&scope, task_id) {
                continue;
            }
            scope
        } else {
            None
        };
        // A workspace path resolves directly without depending on registry publication.
        // An owner id alone is not a stop target; never invent a pid from lock bytes.
        // §FS-rhei-transition-cmd.3.4
        let recovery = if descriptor.is_some() && cfg!(unix) {
            format!("stop the relevant run with `rhei stop {}`", shell_quote(&root.display().to_string()))
        } else {
            // `rhei stop` needs POSIX signals; never offer it where it can only refuse.
            let unavailable = if descriptor.is_some() {
                "`rhei stop` is not supported on this platform"
            } else {
                "There is no resolvable live run descriptor"
            };
            format!(
                "stop the run at {} through its original launcher or process controls \
                 (interrupt the foreground run with Ctrl-C in its terminal; for a detached \
                 run, identify its supervisor process before stopping it). {unavailable}; \
                 once the run has stopped",
                root.display(),
            )
        };
        let restart = if in_project { root.clone() } else { absolutize(&route.metadata_file) };
        let mut selection = String::new();
        if let Some(members) = &scope {
            for id in members {
                selection.push_str(" --rhei ");
                selection.push_str(&shell_quote(id));
            }
        }
        eprintln!(
            "Advisory: Task {task_id} was released into '{to}' with a visits budget while a run \
             is active at {}. The live scheduler may still use its held machine and old visits \
             budget; successful validation and gate release do not guarantee adoption of an \
             edited machine. To use the edited budget reliably, {recovery}, release with \
             `rhei transition` if the task is still gated, then start a fresh \
             `rhei run {}{selection}` with the intended machine and options.",
            root.display(),
            shell_quote(&restart.display().to_string()),
        );
        return;
    }
}

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
