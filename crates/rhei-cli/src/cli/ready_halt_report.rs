// Every halted ticket of a run classified at once, and the lines the run's
// halt message and dry run print from those classifications. The
// classification of one ticket is `ready_halt_causes.rs`.

// §AR-source-file-size.3 §FS-rhei-run-report.3.1 §FS-rhei-run.4

/// Every in-scope, non-terminal ticket with why it is not moving, in plan
/// order — the shared basis for the run's halt diagnostics and the report.
///
/// `worked` reports whether the run actually spawned an invocation for a
/// ticket; those failed at their work rather than at scheduling, so they keep
/// the generic stalled reading unless `missing` names what the work left
/// unwritten, which is a halt with a concrete remedy. `interrupted` reports
/// whether the run's shutdown ended the ticket's last invocation, which
/// outranks both.
// §FS-rhei-run-report.3.1 §FS-rhei-run.3.2: non-leaf tickets are classified
// alongside leaves, so a parent nobody can advance is nameable as the reason a
// dependent is stuck; an interrupted worker explains its ticket before its work does.
#[allow(clippy::too_many_arguments)]
fn classify_halted_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
    worked: &dyn Fn(&str) -> bool,
    missing: &dyn Fn(&str, &str) -> Option<UnwrittenOutputs>,
    interrupted: &dyn Fn(&str) -> bool,
    plan_arg: &str,
    roots: &ReadySetRoots<'_>,
) -> Vec<(&'a rhei_core::ast::Task, HaltCause)> {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let state_map = plan_state_map(&all, machines);
    all.iter()
        .copied()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .map(|task| {
            let id = task.id.to_string();
            // `missing` is asked about the state the ticket is in now, so a
            // stall it left behind two states ago cannot explain this halt.
            // §FS-rhei-run-report.3.1
            let state = normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
            let cause = classify_halt(
                task,
                rhei,
                machines,
                &state_map,
                scope,
                worked(&id),
                missing(&id, &state),
                interrupted(&id),
                plan_arg,
                roots,
            );
            (task, cause)
        })
        .collect()
}

/// One `Task <id> (<state>): <reason> — <next action>` line per halted ticket,
/// plus whether any of them needs a human to act — which is what makes a run,
/// real or dry, end non-zero. The caller emits the lines through its own run
/// journal.
///
/// The lines and the verdict answer different questions. A line explains one
/// ticket from its own vantage point; the verdict asks whether the plan as a
/// whole is waiting on a human decision, which only the walk over subtrees and
/// priors can answer. Reading the verdict off the per-ticket causes instead was
/// a second judgment, and it disagreed with the real run's: a `BlockedByPrior`
/// whose chain ends in a gate is no pause by variant, so a dry run exited one
/// on a plan the run itself exits zero on.
// §FS-rhei-run.4 §FS-rhei-run-report.3.1: the live halt message and `--dry-run`
// must not disagree, so both derive from `remaining_work_is_only_gating_or_poll_blocked`.
fn halted_task_report(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
    plan_path: &Path,
    roots: &ReadySetRoots<'_>,
) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    // Suggested commands carry the plan, so they run from wherever the operator
    // is reading them. §FS-rhei-errors.2
    let plan_arg = plan_arg_for_help(plan_path);
    // Pre-launch diagnostics: no run has happened yet, so nothing worked and
    // nothing is known missing. §FS-rhei-run.4
    for (task, cause) in classify_halted_tasks(
        rhei,
        machines,
        scope,
        &|_| false,
        &|_, _| None,
        &|_| false,
        &plan_arg,
        roots,
    ) {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        let id = task.id.to_string();
        let (reason, next) = cause.describe(&id, &state);
        lines.push(format!("Task {id} ({state}): {reason} \u{2014} {next}"));
    }
    let needs_human = !remaining_work_is_only_gating_or_poll_blocked(rhei, machines, scope);
    (lines, needs_human)
}
