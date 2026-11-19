// Which cause one non-terminal ticket has: the classification itself, read in
// the order the run report publishes, over the vocabulary and the rendering
// `ready_halt_causes.rs` declares.
//
// Its own part because choosing a cause and saying it are different questions:
// a new rung changes this file, and a new wording changes the other.

// §AR-source-file-size.3 §FS-rhei-run-report.3.1 §FS-rhei-run.4

/// What a ticket's last worker left unwritten, and the code it exited with.
///
/// The two travel together because the halt row names both, and the exit is
/// not always `0`: a program that took a declared route out of a non-zero exit
/// owes the ticket's result the same way. `exit_code` is `None` when the run
/// has no code for the worker at all.
// §FS-rhei-run-report.3.1 §FS-rhei-programs.3.2
#[derive(Debug, Clone)]
struct UnwrittenOutputs {
    entries: Vec<String>,
    exit_code: Option<i32>,
    /// What that worker reported it fell back from. §FS-rhei-configuration-fallbacks
    configuration_fallbacks: Vec<String>,
}

/// Classify why a non-terminal ticket did not advance. `worked` marks a ticket
/// the run actually spawned work for, whose failure is the ordinary stalled
/// case rather than a scheduling one; `missing` carries the required artifacts
/// its last worker left unwritten and the code it exited with, when the run
/// recorded any; `interrupted` marks one whose last invocation the run's
/// shutdown ended.
///
/// `spawns` carries the artifact roots the ready-set scan resolves against, so a
/// ticket refused for a file that is not there is explained by the file rather
/// than by never having been scheduled. Every caller has them — the run knows
/// the roots it just scanned with — so no surface can be short of them and
/// quietly give a halted ticket a reading of its own.
// §AR-rhei-panta.5 §FS-rhei-run-report.3.1: one classification, one set of
// roots, every surface of the run.
#[allow(clippy::too_many_arguments)]
fn classify_halt(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    state_map: &std::collections::HashMap<&TaskId, String>,
    scope: &RheiScope,
    worked: bool,
    missing: Option<UnwrittenOutputs>,
    interrupted: bool,
    plan_arg: &str,
    spawns: &SpawnSiteRoots<'_>,
) -> HaltCause {
    let machine = machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    // A held descendant is not stalled and not blocked; it is waiting on a
    // supervisor that has not been woken yet, and that outranks every other
    // reading of it. §FS-rhei-supervision.3.4
    if let Some(hold) = held_by_supervisor(task, rhei, machines) {
        return HaltCause::HeldBySupervisor {
            supervisor: hold.supervisor.to_string(),
            state: hold.state,
            awaiting_human: hold.awaiting_human,
        };
    }
    let open = open_descendant_tasks(task, machines);
    // A gate that kept its block is *holding* the subtree, not waiting on it —
    // and "waiting on descendants" would point the reader at tickets nobody can
    // work. §FS-rhei-supervision.3.1
    if !open.is_empty()
        && machine.states.get(&state).map(|def| def.gating).unwrap_or(false)
        && recorded_supervision_phase(rhei.metadata.as_ref(), &task.id)
            == Some(SupervisionPhase::Held)
    {
        return HaltCause::GateHoldingSubtree { open: format_open_descendants(&open, machines) };
    }
    // A parent is not schedulable at all until its subtree closes, so that
    // outranks anything about its own state. §FS-rhei-plan-language.3
    if !open.is_empty() && !task_is_supervising(task, machine) {
        return HaltCause::WaitingOnDescendants { open: format_open_descendants(&open, machines) };
    }
    // A supervisor that ran its whole subtree and has no edge left is not
    // stalled work: the machine is missing a line. §FS-rhei-supervision.4.1
    if task_is_supervising(task, machine)
        && open.is_empty()
        // The one predicate the warning uses, so the two surfaces cannot drift.
        // §FS-rhei-supervision.1.2
        && !rhei_validator::supervising_state_can_finish(machine, &state)
    {
        return HaltCause::SupervisorHasNoTerminalEdge {
            suggested_final: suggested_final_state(machine, &state),
        };
    }
    // A supervisor that released a subtree nothing can move is not "not
    // scheduled": it is unwakeable, and a rerun cannot change that.
    // §FS-rhei-supervision.3.6
    if task_is_supervising(task, machine) {
        let held = SupervisedSubtree {
            rhei,
            machines,
            roots: ReadySetRoots {
                workspace_root: spawns.tasks.workspace_root,
                task_roots: spawns.tasks.task_roots,
            },
            supervisor: task,
        };
        if let Some(open) = stranded_released_supervisor(&held, &open) {
            // §FS-rhei-reset.5.1: every surface of the row names the same command.
            let reset = spawns.machines.reset.for_ticket(&task.id.to_string());
            return HaltCause::SupervisorStrandedBySelfLoop { open, reset };
        }
    }
    if machine.states.get(&state).map(|def| def.gating).unwrap_or(false) {
        return HaltCause::Gate;
    }
    // A claim outranks a prior: releasing it is the one action that unblocks
    // the scheduler, and a claimed ticket is skipped before priors are read.
    if let Some(assignee) = task.assignee.as_deref() {
        return HaltCause::Claimed { assignee: assignee.to_string() };
    }
    // Only a released supervisor answers through its subtree: a held one is the ready
    // ticket its descendants read back at. Every ticket reaching this with an open
    // subtree is a supervisor, by the parent rung above. §FS-rhei-run-report.3.1
    if !open.is_empty()
        && recorded_supervision_phase(rhei.metadata.as_ref(), &task.id)
            == Some(SupervisionPhase::Released)
    {
        return HaltCause::WaitingOnDescendants { open: format_open_descendants(&open, machines) };
    }
    if let Some(BlockingPrior { prior, cancelled }) =
        first_blocking_prior_reading(task, state_map, machines, scope)
    {
        return HaltCause::BlockedByPrior { prior, cancelled };
    }
    // Below here a clock is the wait only of a ticket no other blocker holds. §FS-rhei-run.5.1.1
    let clock_can_wait = !blocker_no_clock_releases(task, rhei, machines, spawns);
    // Only a hold across every pending invocation names a provider in the
    // explanation, as in the idle blocker and scheduler. §FS-rhei-run.5.1
    if let Some(limit) = provider_wait_holding_ticket(rhei, machines, task, spawns)
        .filter(|_| clock_can_wait)
    {
        return HaltCause::ProviderLimited {
            provider: limit.identity.provider,
            next_attempt_at: limit.next_attempt_at,
            kind: limit.kind,
        };
    }
    // Behind the claim and the prior, unlike the gate above: both really do
    // stop a poll from ever coming back, so "it resumes itself" would be a
    // false promise. §FS-rhei-run-report.3.1 §FS-rhei-states.2.5
    let waiting_on = machine.states.get(&state).and_then(|def| def.waiting_on_person());
    if let Some(label) = waiting_on.filter(|_| clock_can_wait) {
        return HaltCause::WaitingOnPerson { label: label.to_string() };
    }
    // The scan's own poll skip, so only a deadline that alone holds the ticket — still ahead, bound
    // unspent, no blocker a clock cannot release — earns the row.
    // §FS-rhei-run-report.3.1 §FS-rhei-run.5.1 §FS-rhei-run.5.1.1
    if let Some(deadline) = spawns
        .pending_poll_deadline(rhei, machine, task, &state, spawns.pending_since())
        .filter(|_| clock_can_wait)
    {
        return HaltCause::WaitingOnPoll { next_attempt_at: epoch_rfc3339(deadline) };
    }
    // Ahead of every work-shaped cause below: an interrupted worker explains
    // both an unwritten artifact and a bare stall. §FS-rhei-run-report.3.1
    if interrupted {
        return HaltCause::Interrupted;
    }
    // The spawn planner knows what rerunning cannot release, even when this
    // run consumed the last attempt. §FS-rhei-run-report.3.1
    if let Some((attempts, entries)) = spent_visit_halt(task, rhei, machine, &state, spawns) {
        return HaltCause::SpentVisit {
            attempts,
            entries,
            // Preserve this invocation's diagnostics beside its unmet artifacts.
            // §FS-rhei-configuration-fallbacks
            configuration_fallbacks: missing.as_ref()
                .map(|unwritten| unwritten.configuration_fallbacks.clone())
                .unwrap_or_default(),
        };
    }
    if worked {
        // The run knows exactly what the worker did not write; say so instead
        // of pointing at logs. §FS-rhei-run-report.3.1
        return match missing {
            Some(unwritten) if !unwritten.entries.is_empty() => HaltCause::MissingOutputs {
                entries: unwritten.entries,
                exit_code: unwritten.exit_code,
                plan: plan_arg.to_string(),
                configuration_fallbacks: unwritten.configuration_fallbacks,
            },
            _ => HaltCause::Stalled,
        };
    }
    // Only infer manual-only work when this run neither worked nor interrupted
    // the ticket, and no spent visit proves its hold. §FS-rhei-run-report.3.1
    if let Ok(Some(to)) = manual_initial_terminal_transition(task, rhei, machine) {
        return HaltCause::ManualOnly { to };
    }
    // Readiness refused this ticket because a file it needs is not there — but
    // only say so when the input is what the scan stopped at, never over a wait
    // no file can end. §FS-rhei-run-report.3.1 §FS-rhei-states.3
    if !ready_scan_stops_before_inputs(task, rhei, machines, machine) {
        let entries = missing_state_inputs_for_ready_set(&spawns.tasks, rhei, machine, task, &state);
        if !entries.is_empty() {
            return HaltCause::MissingInputs { entries };
        }
    }
    match find_next_transition(task, rhei, machine) {
        Ok(None) => HaltCause::NoTransition,
        // Nothing ran against this ticket, so nothing about it stalled. It was
        // simply never reached. §FS-rhei-run-report.3.1
        _ => HaltCause::NotScheduled,
    }
}

/// Explain the same spent allowance the clock and spawn site read, without
/// counting dormant identities, past visits, or capacity a retry grant restored.
/// The completion reader supplies paths for each active invocation; no settings
/// are reloaded to guess its identity, and the result uses the same member root.
/// §FS-rhei-run-report.3.1
fn spent_visit_halt(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    state: &str,
    spawns: &SpawnSiteRoots<'_>,
) -> Option<(u64, Vec<String>)> {
    let plans = spawns.next_spawn_plans(rhei, machine, task, state)?;
    // Poll bounds keep their own exhaustion/recovery semantics. §FS-rhei-retry.3
    if !matches!(plans.budget, AttemptBudget::Visit(_)) || plans.halt() != Some(SpawnHalt::Spent) {
        return None;
    }
    let root = spawns.tasks.artifact_root(&task.id.to_string());
    let visit = render_visit_count(rhei.metadata.as_ref(), &task.id, state, task.state.as_str(), machine);
    let mut attempts = 0;
    let mut entries = Vec::new();
    for (agent, plan) in &plans.invocations {
        // Fan-out allowances are per invocation: report the largest spent count,
        // never sum them into an allowance no invocation had. §FS-rhei-run-report.3.1
        let plan = plan.as_ref()?;
        attempts = attempts.max(plan.charged);
        let missing = match agent {
            Some(agent) => missing_required_outputs_for_invocation(
                root, machine, task, state, visit,
                selected_forward_transition(rhei, machine, task).as_deref(),
                invocation_context(agent),
            ),
            // Program debt follows its recorded exit, including on zero-spawn
            // runs; declaration order cannot select it. §FS-rhei-programs.3.2
            None => missing_previous_program_outputs(
                root, machine, rhei.metadata.as_ref(), task, state, plan,
            ),
        };
        for missing in missing {
            // Show the checked root, preserving the reader's template diagnosis.
            // §FS-rhei-run-report.3.1
            let path = std::path::absolute(&missing.path).unwrap_or(missing.path);
            let marker = if missing.unresolved_template { ", unresolved template" } else { "" };
            let entry = format!("{} ({}{marker})", missing.name, path.display());
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    Some((attempts, entries))
}
