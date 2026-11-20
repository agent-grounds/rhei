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
        let descriptor = live_descriptor_at(&root);
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
        let recovery = live_run_stop_advice(&root, descriptor.as_ref(), false);
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

include!("live_run_evidence.rs");
