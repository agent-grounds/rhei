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
    spawns: &SpawnSiteRoots<'_>,
    spawned: &HashSet<String>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_runnable_tasks_with_options(rhei, machines, spawns, spawned)
}

/// The run scheduler's ready view, including the run-level identity used to
/// render required input artifacts: the options `spawns` carries.
/// §FS-rhei-agents.1.4
fn find_runnable_tasks_with_options<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    spawns: &SpawnSiteRoots<'_>,
    spawned: &HashSet<String>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks_in_view(
        rhei,
        machines,
        spawns,
        spawned,
        ReadySetView::RunnableNow,
        Some(spawns.opts),
    )
        .into_iter()
        .filter(|task| task.assignee.is_none())
        // §FS-rhei-seal.8.4: a refused ticket holds no non-concurrent state from its siblings.
        .filter(|task| !strict_ticket_refused(&task.id.to_string()))
        .collect()
}

/// Program polls that retain every autonomous-run eligibility constraint but
/// may still be waiting for a persisted retry deadline. This is an engine
/// selection probe only; the scheduler continues to use [`find_runnable_tasks`].
/// §FS-rhei-run.3
fn find_runnable_program_polls_for_mode_selection<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    spawns: &SpawnSiteRoots<'_>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks_in_view(
        rhei,
        machines,
        spawns,
        &HashSet::new(),
        ReadySetView::ProgramModeProbe,
        Some(spawns.opts),
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

/// The continuous run's ready set, less every ticket a provider wait holds, so
/// a run whose every ticket waits reaches its deadline sleep, which opens no
/// pass, instead of opening one on every slice of it.
/// §FS-rhei-run.3 step 2 §FS-rhei-run.5.1
///
/// That sleep alone decides whether a wait is worth sleeping for. When it would
/// find nothing to wake for, the scan comes back as it was, so a held ticket
/// never turns a wait into a halt the unfiltered pass would not have reached.
fn without_provider_held_tickets<'a>(
    ready: Vec<&'a rhei_core::ast::Task>,
    loaded: &LoadedPlan,
    settings: &RheiSettings,
    scope: &RheiScope,
    spawns: &SpawnSiteRoots<'_>,
) -> Vec<&'a rhei_core::ast::Task> {
    let free: Vec<_> = ready
        .iter()
        .copied()
        .filter(|task| !provider_holds_ticket(task, loaded, settings, spawns))
        .collect();
    if free.is_empty()
        && !ready.is_empty()
        && earliest_pending_agent_deadline(&loaded.rhei, &spawns.machines.set, settings, scope, spawns)
            .is_none()
    {
        return ready;
    }
    free
}

/// Whether a provider wait holds this ticket at the instant `spawns` was read:
/// it has an invocation still to be spawned, and each such invocation waits on
/// an active deadline of its execution identity. One on a free identity keeps
/// the ticket ready; one the pre-spawn completion check finds satisfied keeps
/// nothing ready. A program or gating state has no provider to wait on.
/// §FS-rhei-run.3 step 2 §FS-rhei-run.3.3
fn provider_holds_ticket(
    task: &rhei_core::ast::Task,
    loaded: &LoadedPlan,
    settings: &RheiSettings,
    spawns: &SpawnSiteRoots<'_>,
) -> bool {
    let machines = spawns.machines;
    let machine = machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    let Some(state_def) = machine.states.get(&state) else { return false };
    if state_def.program.is_some() || state_def.gating {
        return false;
    }
    let resolved =
        resolve_agent_invocations_for_task(machine, &state, settings, spawns.opts, Some(task));
    let Ok(invocations) = resolved else { return false };
    let held = |resolved: &ResolvedAgent| {
        resolved_provider_deadline(&loaded.rhei, &machines.set, resolved, spawns.read_at).is_some()
    };
    // Nothing held, nothing to drop: the completion check is read only for a wait.
    if !invocations.iter().any(held) {
        return false;
    }
    let task_id = task.id.to_string();
    let task_root = spawns.tasks.artifact_root(&task_id);
    let runtime_dir = machines.agent_runtime_dir(&task_id, task_root, spawns.runtime_dir);
    let pending = agent_invocations_to_spawn(
        loaded,
        spawns.tasks.workspace_root,
        &runtime_dir,
        task,
        machine,
        &state,
        state_def,
        invocations,
        None,
    );
    !pending.is_empty() && pending.iter().all(held)
}
