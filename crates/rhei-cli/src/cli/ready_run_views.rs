/// Which temporal view the shared readiness scan exposes. The scheduler sees
/// only work due now; [`ReadySetView::ClockAside`] sets the poll clock aside
/// and keeps every other eligibility rule. §FS-rhei-run.3
#[derive(Clone, Copy, Eq, PartialEq)]
enum ReadySetView {
    RunnableNow,
    /// The ready set once every `pollNextAttemptAt` has passed: what
    /// execution-mode selection reads for a program poll whose persisted
    /// deadline is its sole remaining delay, and what the deadline scan reads
    /// for every ticket whose deadline is. §FS-rhei-run.3 §FS-rhei-run.5.1.1
    ClockAside,
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
    runnable_tasks_in_view(rhei, machines, spawns, spawned, ReadySetView::RunnableNow)
}

/// The run scheduler's ready set in `view`, under the options `spawns` carries,
/// less every ticket the run will not take however ready it is. One function for
/// the scheduler and the deadline scan, so a ticket the scan holds back for a
/// reason no clock releases is one no deadline is ever taken for.
/// §FS-rhei-run.3 step 2 §FS-rhei-run.5.1.1
fn runnable_tasks_in_view<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    spawns: &SpawnSiteRoots<'_>,
    spawned: &HashSet<String>,
    view: ReadySetView,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks_in_view(rhei, machines, spawns, spawned, view, Some(spawns.opts))
        .into_iter()
        .filter(|task| task.assignee.is_none())
        // §FS-rhei-seal.8.4: a refused ticket holds no non-concurrent state from its siblings.
        .filter(|task| !strict_ticket_refused(&task.id.to_string()))
        .collect()
}

/// Whether something holds this ticket that no clock releases, so no poll
/// deadline or provider-limit record is its wait, its own or its execution
/// identity's: a required input the ready scan cannot find, a strict run's
/// refusal, or a spent attempt budget its spawn site refuses the next spawn for.
/// Each is read as the scan and the spawn site read it, never restated.
///
/// The one question every reader of a clock asks. The deadline scan asks it of
/// the ready set with the clock set aside, which has already answered the claim,
/// the gate, the supervisor, the subtree and the priors; the halt classification
/// and the deliberate-wait judgment ask it after their own rungs for those.
/// §FS-rhei-run.5.1.1 §FS-rhei-run-report.3.1
fn blocker_no_clock_releases(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    spawns: &SpawnSiteRoots<'_>,
) -> bool {
    let task_id = task.id.to_string();
    let machine = machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    // §FS-rhei-seal.8.4: only a person resumes a refused ticket.
    strict_ticket_refused(&task_id)
        || !state_inputs_exist_for_ready_set(
            spawns.tasks.workspace_root,
            spawns.tasks.artifact_root(&task_id),
            rhei,
            machine,
            task,
            &state,
            Some(spawns.opts),
        )
        || spawns.attempt_budget_spent(rhei, machine, task, &state)
}

/// The tickets a clock alone can be holding: the run's ready set with the poll
/// clock set aside, less every one a blocker no clock releases holds. Only these
/// contribute a deadline, in every mode the run executes in. §FS-rhei-run.5.1.1
fn tickets_only_a_clock_holds<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    spawns: &SpawnSiteRoots<'_>,
) -> Vec<&'a rhei_core::ast::Task> {
    // No live spawn set reaches a deadline scan, so only a claim is in flight
    // beneath a supervisor here. §FS-rhei-supervision.3.2
    runnable_tasks_in_view(rhei, machines, spawns, &HashSet::new(), ReadySetView::ClockAside)
        .into_iter()
        .filter(|task| !blocker_no_clock_releases(task, rhei, machines, spawns))
        .collect()
}

/// When `resolved`'s provider records let this ticket spawn: their deadline, or
/// "now" once they have run out, so a deadline that moved between a refill and
/// the outer check costs one rescan (§FS-rhei-run.3.3). Run-out records that
/// were all observed before this visit's last spawn of `resolved` started are no
/// deadline at all: that spawn ran past every one of them, so waking for them
/// would only release a stall. A record a spawn writes is observed after it
/// started, so a ticket that spawn parked or backed off keeps its "now".
// §FS-rhei-run.5.1.1
fn provider_wait_of(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    spawns: &SpawnSiteRoots<'_>,
    ticket: (&rhei_validator::StateMachine, &rhei_core::ast::Task, &str),
    resolved: &ResolvedAgent,
) -> Option<u64> {
    let eligible =
        resolved_provider_eligibility_deadline(rhei, machines, resolved, spawns.read_at)?;
    if eligible > spawns.read_at {
        return Some(eligible);
    }
    let identity = resolved_provider_identity(resolved)?;
    let Some(started) = last_spawn_started(spawns, rhei, ticket, resolved) else {
        return Some(eligible);
    };
    let mut tasks = Vec::new();
    visit_tasks(&rhei.tasks, &mut tasks);
    let met = tasks
        .into_iter()
        .filter_map(|task| task_provider_limit(rhei, machines, task))
        .filter(|limit| limit.identity == identity)
        .all(|limit| {
            let observed = chrono::DateTime::parse_from_rfc3339(&limit.observed_at);
            observed.is_ok_and(|observed| observed.timestamp() < started)
        });
    (!met).then_some(eligible)
}

/// When `agent`'s last spawn of this ticket's current visit started, in Unix
/// seconds, as its spawn record says; `None` when this visit has not spawned it.
// §FS-rhei-run.5.1.1 §FS-rhei-agents.8.4
fn last_spawn_started(
    spawns: &SpawnSiteRoots<'_>,
    rhei: &rhei_core::ast::Rhei,
    (machine, task, state_name): (&rhei_validator::StateMachine, &rhei_core::ast::Task, &str),
    agent: &ResolvedAgent,
) -> Option<i64> {
    let task_id = task.id.to_string();
    let task_root = spawns.tasks.artifact_root(&task_id);
    let (meta, state) = (rhei.metadata.as_ref(), task.state.as_str());
    let visits = render_visit_count(meta, &task.id, state_name, state, machine);
    let runtime_dir = spawns.machines.agent_runtime_dir(&task_id, task_root, spawns.runtime_dir);
    let identity = resolved_agent_log_identity(agent);
    let (plan, _) = plan_named_spawn(
        machine,
        task_root,
        &runtime_dir,
        &task_id,
        state_name,
        visits,
        identity.as_deref(),
        &spawns.machines.reset,
    )
    .ok()?;
    Some(chrono::DateTime::parse_from_rfc3339(&plan.previous?.started).ok()?.timestamp())
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
        ReadySetView::ClockAside,
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
/// It leaves out, too, every ticket a deadline sleep kept for a bound still
/// spent (`kept_spent`), which no pass the sleep opens could do anything but
/// halt again. §FS-rhei-run.3.6.1
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
    kept_spent: &HashSet<String>,
) -> Vec<&'a rhei_core::ast::Task> {
    let free: Vec<_> = ready
        .iter()
        .copied()
        .filter(|task| !kept_spent.contains(&task.id.to_string()))
        .filter(|task| provider_wait_holding_ticket(&loaded.rhei, &spawns.machines.set, task, spawns).is_none())
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

/// Whether the ticket `task_id` names is still halted before its next spawn,
/// read afresh from the plan `spawns` stands for: its bound is spent, or its
/// log name cannot be allocated (§FS-rhei-agents.8.1). The one question both
/// deadline-sleep sites and the sleeping run's ready filter ask, so they cannot
/// disagree about which stalls a sleep keeps. A ticket the plan no longer has
/// answers `false`. §FS-rhei-run.3.6.1
fn spawn_still_halted(loaded: &LoadedPlan, spawns: &SpawnSiteRoots<'_>, task_id: &str) -> bool {
    let Some(task) = find_task_by_id_str(&loaded.rhei.tasks, task_id) else { return false };
    let machine = spawns.machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    spawns.next_spawns_halt(&loaded.rhei, machine, task, &state).is_some()
}

/// The waits holding every invocation still to be spawned. A free pending arm
/// defeats the hold; an empty pending set holds nothing. Scheduling,
/// classification and deadline composition all ask this one question, with
/// their own reading of an identity's wait (active, or eligible for a rescan).
/// Resolution errors propagate so the deadline scan never mistakes an invalid
/// invocation for a ticket held only by its poll. A program or gating state has
/// no provider to wait on. §FS-rhei-run.5.1
fn provider_waits_holding_ticket<W>(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    task: &rhei_core::ast::Task,
    settings: &RheiSettings,
    spawns: &SpawnSiteRoots<'_>,
    wait: impl Fn(&ResolvedAgent) -> Option<W>,
) -> MietteResult<Option<Vec<W>>> {
    let machine = machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    let Some(state_def) = machine.states.get(&state) else { return Ok(None) };
    if state_def.program.is_some() || state_def.gating {
        return Ok(None);
    }
    let invocations =
        resolve_agent_invocations_for_task(machine, &state, settings, spawns.opts, Some(task))?;
    // Nothing held, nothing to drop: the completion check is read only for a wait.
    if !invocations.iter().any(|resolved| wait(resolved).is_some()) {
        return Ok(None);
    }
    let pending =
        spawns.invocations_still_to_spawn(rhei, task, machine, &state, state_def, invocations);
    if pending.is_empty() {
        return Ok(None);
    }
    Ok(pending.iter().map(wait).collect())
}

/// The effective active provider hold, represented by the first pending arm
/// that will free. Every pending arm must wait, and each identity reads its
/// latest record across the plan, retaining that record's kind for all halt
/// and idle surfaces. All clocks use `spawns.read_at`. §FS-rhei-run.5.1
fn provider_wait_holding_ticket(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    task: &rhei_core::ast::Task,
    spawns: &SpawnSiteRoots<'_>,
) -> Option<ProviderLimit> {
    let mut read = None;
    let settings = spawns.settings_or_merged(&mut read)?;
    provider_waits_holding_ticket(rhei, machines, task, settings, spawns, |resolved| {
        let identity = resolved_provider_identity(resolved)?;
        provider_limit_for_identity(rhei, machines, &identity)
            .filter(|limit| limit.deadline_epoch().is_some_and(|deadline| deadline > spawns.read_at))
    })
    .ok()
    .flatten()?
    .into_iter()
    .min_by_key(|limit| limit.deadline_epoch())
}
