/// Which temporal view the shared readiness scan exposes. The scheduler sees
/// only work due now; execution-mode selection may also see a program poll
/// whose persisted deadline is its sole remaining delay. §FS-rhei-run.3
#[derive(Clone, Copy, Eq, PartialEq)]
enum ReadySetView {
    RunnableNow,
    ProgramModeProbe,
}

/// Find tasks that `rhei run` may schedule autonomously.
///
/// This keeps the readiness semantics used by the run loop, but skips
/// tasks that already carry an assignee so a manual claim cannot be stolen by
/// the orchestrator.
// §AR-rhei-panta.5: inputs resolve against the owning rhei's execution root.
#[cfg(test)]
fn find_runnable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
    spawned: &HashSet<String>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_runnable_tasks_with_options(rhei, machines, roots, spawned, &default_run_options())
}

/// The run scheduler's ready view, including the run-level identity used to
/// render required input artifacts. §FS-rhei-agents.1.4
fn find_runnable_tasks_with_options<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
    spawned: &HashSet<String>,
    opts: &RunOptions,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks_in_view(
        rhei,
        machines,
        roots,
        spawned,
        ReadySetView::RunnableNow,
        Some(opts),
    )
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .collect()
}

/// Program polls that retain every autonomous-run eligibility constraint but
/// may still be waiting for a persisted retry deadline. This is an engine
/// selection probe only; the scheduler continues to use [`find_runnable_tasks`].
/// §FS-rhei-run.3
fn find_runnable_program_polls_for_mode_selection<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
    opts: &RunOptions,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks_in_view(
        rhei,
        machines,
        roots,
        &HashSet::new(),
        ReadySetView::ProgramModeProbe,
        Some(opts),
    )
    .into_iter()
    .filter(|task| task.assignee.is_none())
    .filter(|task| {
        let machine = machines.for_task(&task.id);
        let state_name = normalized_state_name(task.state.as_str(), machine);
        machine
            .states
            .get(&state_name)
            .is_some_and(|def| def.program.is_some() && def.poll.is_some())
    })
    .collect()
}
