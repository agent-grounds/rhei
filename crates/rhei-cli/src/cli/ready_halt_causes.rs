// Why one non-terminal ticket is not moving: the one classification every
// surface uses — `rhei run`'s halt message, its dry run, and the durable
// report's Attention table — and the lines it renders.
//
// Its own part because classifying a halt asks a different question from
// finding the ready set next door: not "what may be scheduled" but "why was
// this one not", and the answer has to carry the next action.

// §AR-source-file-size.3 §FS-rhei-run-report.3.1 §FS-rhei-run.4

/// Why one non-terminal ticket is not moving.
///
/// `rhei next` already tells a worker exactly which of these applies. Every
/// `rhei run` surface — the halt message, the dry
/// run, and the durable report's Attention table — collapsed all of them into
/// "stalled in non-terminal state <s>" with "inspect logs or mark the task
/// cancelled" as the advice: wrong for a claimed ticket, wrong for one waiting
/// on a prior, and pointing at logs a run that spawned nothing never wrote.
// §FS-rhei-run-report.3.1 §FS-rhei-run.4: one classification, every surface.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HaltCause {
    /// A non-leaf ticket whose own subtree is still open. It is a task in its
    /// own right, so it is reported — but the work is in the descendants.
    /// §FS-rhei-plan-language.3
    WaitingOnDescendants { open: String },
    /// A gating state deliberately waiting for a human decision.
    Gate,
    /// A gating state that is *also* still holding a subtree: the ticket left a
    /// supervising state by its exhaustion edge, so its `supervision` block
    /// survived the move and nothing beneath it runs until a human moves it on.
    // §FS-rhei-supervision.3.1
    GateHoldingSubtree { open: String },
    /// A poll state that declares the person it waits on. Nothing about it is
    /// stuck and nothing is anyone's to do: the poll resumes itself when the
    /// answer lands. Without this the ticket's only reading was work in
    /// flight, which is what let one approval wait occupy a concurrency slot
    /// for hours looking exactly like a running agent.
    // §FS-rhei-states.2.5 §FS-rhei-run-report.3.1
    WaitingOnPerson { label: String },
    /// A supported provider has supplied a durable safe retry deadline. This
    /// is deliberate waiting, not attention. §FS-rhei-run-report.3.1
    ProviderLimited { provider: String, next_attempt_at: String },
    /// A supervising ancestor is owed a visit or is working, so nothing beneath
    /// it is dispatched. Named so a held subtree is never read as a stall.
    // §FS-rhei-supervision.3.4
    HeldBySupervisor { supervisor: String, state: String, awaiting_human: bool },
    /// A supervisor whose subtree is closed and whose machine gives no
    /// `openDescendants` edge out of the supervising state a path to a final
    /// state it can finish in. The run did everything right — it ran the whole
    /// subtree — and then had nowhere to put the parent; `rhei validate` warns
    /// about exactly this machine, by the same predicate, so the halt names the
    /// missing line rather than pointing at logs.
    // §FS-rhei-supervision.1.2 §FS-rhei-supervision.4.1
    SupervisorHasNoTerminalEdge { suggested_final: String },
    /// A supervisor that released its subtree and can never be woken again:
    /// every non-terminal descendant beneath it is blocked, so no checkpoint
    /// can arrive and a released supervisor is scheduled by nothing else.
    ///
    /// Only a workspace stranded before §FS-rhei-supervision.3.6 withheld that
    /// self-loop reaches this, or one whose supervisor steered its subtree into
    /// a corner. Either way "not scheduled … rerun to pick it up" was the one
    /// remedy that provably does nothing, and this row names the ones that do.
    // §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
    SupervisorStrandedBySelfLoop { open: String },
    /// A live `**Assignee:**`; the scheduler never schedules a claimed ticket.
    Claimed { assignee: String },
    /// An unsatisfied `**Prior:**`, already formatted as `Task <id> (<state>)`.
    BlockedByPrior { prior: String },
    /// Manual-only initial state: `rhei run` must not advance it.
    ManualOnly { to: String },
    /// Non-terminal with no declared outgoing transition to take.
    NoTransition,
    /// The run was interrupted while this ticket's worker was in flight.
    /// Nothing about the ticket failed and nothing about it is waiting on a
    /// human: the run was stopped, and re-running it is the whole recovery.
    /// Reported ahead of `MissingOutputs` because it explains it — a worker
    /// the run killed had no chance to write what it owed.
    // §FS-rhei-run-report.3.1 §FS-rhei-run.3.2
    Interrupted,
    /// A worker ran and left required artifacts unwritten, so the completion
    /// condition refused to advance the ticket. `entries` are the
    /// `name (path)` renderings the run already produced — the ticket's terminal
    /// result appears among them under the name `result`.
    ///
    /// Distinct from `Stalled` because the remedy is concrete: write these
    /// files, or record the outcome by hand. "Inspect logs or mark the task
    /// cancelled" is advice for a halt nobody can name, and this one is named.
    ///
    /// `exit_code` is what that worker exited with, `None` when the run has no
    /// code for it. Usually `0`, but a program that took a declared route out
    /// of a non-zero exit owes the result the same way, and a row saying `0`
    /// would contradict the ledger two sections below it.
    /// §FS-rhei-programs.3.2
    ///
    /// `plan` is the plan argument as the operator would type it, carried here
    /// so the suggested `rhei transition` runs from wherever they are reading
    /// the report rather than only from the plan's own directory.
    // §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1 §FS-rhei-errors.2
    MissingOutputs { entries: Vec<String>, exit_code: Option<i32>, plan: String },
    /// A required `inputs:` artifact of the ticket's current state is not on
    /// disk, so readiness refused to schedule it. `entries` are the
    /// `name (path)` renderings of the files that were looked for.
    ///
    /// Named ahead of `NotScheduled` because it *is* why the ticket was never
    /// scheduled: "rerun to pick it up" sent the operator back to a run that
    /// halts identically, with nothing said about the file being waited on —
    /// and a Panta member looks for it under its own execution root, not the
    /// project's, so the path is the whole answer.
    ///
    /// Only when the missing file is what readiness stopped at, though: a poll
    /// deadline still ahead and a supervisor draining its subtree are refused
    /// before inputs are read, and writing the file would not release either.
    // §FS-rhei-run-report.3.1 §FS-rhei-states.3 §AR-rhei-panta.5
    MissingInputs { entries: Vec<String> },
    /// The run never scheduled this ticket: nothing about it is known to have
    /// failed, so it must not borrow the stalled reading.
    // §FS-rhei-run-report.3.1
    NotScheduled,
    /// None of the above — work was possible and the ticket is still here.
    Stalled,
}

impl HaltCause {
    /// The reason and the next action, for the report table and the halt
    /// diagnostics. Both name concrete commands wherever one exists.
    fn describe(&self, id: &str, state: &str) -> (String, String) {
        match self {
            HaltCause::WaitingOnDescendants { open } => (
                format!("waiting on open descendant {open}"),
                "finish the descendants; the parent is claimable once its subtree is terminal"
                    .to_string(),
            ),
            HaltCause::Gate => (
                "gating state awaiting review".to_string(),
                "transition manually when reviewed".to_string(),
            ),
            // §FS-rhei-supervision.3.1: the gate is holding more than itself.
            HaltCause::GateHoldingSubtree { open } => (
                format!(
                    "left supervision for human gate '{state}'; its subtree stays held until a \
                     human moves it (open: {open})"
                ),
                format!(
                    "move Task {id} back into its supervising state to resume supervision, or \
                     anywhere else to release the subtree"
                ),
            ),
            // §FS-rhei-supervision.3.4: the same reason every surface uses.
            HaltCause::HeldBySupervisor { supervisor, state: supervisor_state, awaiting_human } => (
                format!("held by supervisor Task {supervisor} ({supervisor_state})"),
                if *awaiting_human {
                    // §FS-rhei-supervision.3.1: a gate-parked supervisor has no
                    // next visit to release it on.
                    format!(
                        "Task {supervisor} is at a human gate and still holds this subtree; \
                         move it on to release Task {id}"
                    )
                } else {
                    format!(
                        "the supervisor releases it on its next visit; nothing to do on Task {id}"
                    )
                },
            ),
            // §FS-rhei-supervision.4.1: the missing line, verbatim, and beside
            // it the repair that leaves a deliberate gate where it is.
            HaltCause::SupervisorHasNoTerminalEdge { suggested_final } => (
                format!(
                    "no `openDescendants` transition out of '{state}' reaches a final state, \
                     cancellation aside; its subtree is closed and nothing can finish it"
                ),
                format!(
                    "add `- {{from: {state}, to: {suggested_final}, condition: \
                     openDescendants < 1}}` to the machine's transitions, or point an existing \
                     `openDescendants` edge at a state that reaches a final one"
                ),
            ),
            // §FS-rhei-supervision.3.6: the checkpoint is the only wake, so
            // the remedy is whatever lets a descendant produce one.
            HaltCause::SupervisorStrandedBySelfLoop { open } => (
                format!(
                    "released its subtree on a visit that changed nothing, and nothing beneath \
                     it can move: {open}"
                ),
                format!(
                    "a released supervisor is woken only by a descendant that moves: unblock \
                     one of the tickets above and rerun, or `rhei reset` to start Task {id} over"
                ),
            ),
            // §FS-rhei-states.2.5: the label, and the fact that waiting is all
            // there is to do — the poll comes back on its own.
            HaltCause::WaitingOnPerson { label } => (
                format!("waiting on {label}"),
                format!(
                    "nothing to do on Task {id}; the poll resumes itself when {label} answers"
                ),
            ),
            HaltCause::ProviderLimited { provider, next_attempt_at } => (
                format!("provider {provider} limited until {next_attempt_at}"),
                format!("nothing to do on Task {id}; rhei resumes it automatically"),
            ),
            HaltCause::Claimed { assignee } => (
                format!("claimed by {assignee}"),
                format!(
                    "`rhei release {id}` to hand it back, or `rhei complete {id} --result …` \
                     to finish it"
                ),
            ),
            HaltCause::BlockedByPrior { prior } => (
                format!("waiting on {prior}"),
                "finish the prior first".to_string(),
            ),
            HaltCause::ManualOnly { to } => (
                format!("manual-only initial state '{state}' with terminal transition to '{to}'"),
                format!("`rhei next` to claim, do the work, then `rhei complete {id} --result …`"),
            ),
            HaltCause::NoTransition => (
                format!("no forward transition available from '{state}'"),
                "declare a transition out of this state, or cancel the ticket".to_string(),
            ),
            // Not "stalled", and emphatically not "mark the task cancelled":
            // the operator stopped the run, so the run says so and asks for
            // nothing else. §FS-rhei-run-report.3.1
            HaltCause::Interrupted => (
                format!("run interrupted while its worker was in state {state}"),
                "re-run to continue".to_string(),
            ),
            // Name the files. The whole point of this cause is that the operator
            // does not have to go read a log to learn which one is missing.
            // §FS-rhei-run-report.3.1
            HaltCause::MissingOutputs { entries, exit_code, plan } => (
                match exit_code {
                    Some(code) => format!("worker exited {code} without {}", entries.join(", ")),
                    None => format!("worker exited without {}", entries.join(", ")),
                },
                format!(
                    "write the file(s) above and rerun, or record the outcome with \
                     `rhei transition{} --task {id} --from {state} --to <state> --result …`",
                    if plan.is_empty() { String::new() } else { format!(" {plan}") }
                ),
            ),
            // Name the path, not just the fact: the file the operator wrote and
            // the file readiness looked for can sit under different roots.
            // §FS-rhei-run-report.3.1
            HaltCause::MissingInputs { entries } => (
                format!("required input(s) not found: {}", entries.join(", ")),
                "write the file(s) at the path(s) above — that is where this state looks for \
                 them — then rerun, or mark the input `optional: true` in the state machine"
                    .to_string(),
            ),
            HaltCause::NotScheduled => (
                format!("not scheduled before the run halted, still in '{state}'"),
                format!(
                    "rerun to pick it up; if it never starts, claim it with `rhei next` or check \
                     the agent or program configured for '{state}'"
                ),
            ),
            HaltCause::Stalled => (
                if state.is_empty() {
                    "no forward transition available".to_string()
                } else {
                    format!("stalled in non-terminal state {state}")
                },
                "inspect logs or mark the task cancelled".to_string(),
            ),
        }
    }
}

/// The terminal state a suggested `openDescendants` edge should aim at: one the
/// supervising state already reaches if there is one, otherwise the machine's
/// first success terminal in declaration order.
// §FS-rhei-supervision.4.1 §FS-rhei-states.1.4
fn suggested_final_state(machine: &rhei_validator::StateMachine, state: &str) -> String {
    let is_success_terminal = |name: &str| {
        machine.states.get(name).map(|def| def.terminal).unwrap_or(false)
            && !machine.is_cancellation(name)
    };
    machine
        .transitions()
        .iter()
        .find(|rule| rule.from.0 == state && is_success_terminal(&rule.to.0))
        .map(|rule| rule.to.0.clone())
        .or_else(|| {
            machine.states.keys().find(|name| is_success_terminal(name)).cloned()
        })
        .unwrap_or_else(|| "completed".to_string())
}

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
}

/// Classify why a non-terminal ticket did not advance. `worked` marks a ticket
/// the run actually spawned work for, whose failure is the ordinary stalled
/// case rather than a scheduling one; `missing` carries the required artifacts
/// its last worker left unwritten and the code it exited with, when the run
/// recorded any; `interrupted` marks one whose last invocation the run's
/// shutdown ended.
///
/// `roots` are the artifact roots the ready-set scan resolves against, so a
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
    roots: &ReadySetRoots<'_>,
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
                workspace_root: roots.workspace_root,
                task_roots: roots.task_roots,
            },
            supervisor: task,
        };
        if let Some(open) = stranded_released_supervisor(&held, &open) {
            return HaltCause::SupervisorStrandedBySelfLoop { open };
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
    if let Some(prior) = first_blocking_prior(task, state_map, machines, scope) {
        return HaltCause::BlockedByPrior { prior };
    }
    if let Some(limit) = task_provider_limit(rhei, machines, task)
        .filter(|limit| limit.deadline_epoch().is_some_and(|deadline| deadline > current_unix_secs()))
    {
        return HaltCause::ProviderLimited {
            provider: limit.identity.provider,
            next_attempt_at: limit.next_attempt_at,
        };
    }
    // Behind the claim and the prior, unlike the gate above: both really do
    // stop a poll from ever coming back, so "it resumes itself" would be a
    // false promise. §FS-rhei-run-report.3.1 §FS-rhei-states.2.5
    if let Some(label) = machine.states.get(&state).and_then(|def| def.waiting_on_person()) {
        return HaltCause::WaitingOnPerson { label: label.to_string() };
    }
    if let Ok(Some(to)) = manual_initial_terminal_transition(task, rhei, machine) {
        return HaltCause::ManualOnly { to };
    }
    // Ahead of every work-shaped cause below: an interrupted worker explains
    // both an unwritten artifact and a bare stall. §FS-rhei-run-report.3.1
    if interrupted {
        return HaltCause::Interrupted;
    }
    if worked {
        // The run knows exactly what the worker did not write; say so instead
        // of pointing at logs. §FS-rhei-run-report.3.1
        return match missing {
            Some(unwritten) if !unwritten.entries.is_empty() => HaltCause::MissingOutputs {
                entries: unwritten.entries,
                exit_code: unwritten.exit_code,
                plan: plan_arg.to_string(),
            },
            _ => HaltCause::Stalled,
        };
    }
    // Readiness refused this ticket because a file it needs is not there — but
    // only say so when the input is what the scan stopped at, never over a wait
    // no file can end. §FS-rhei-run-report.3.1 §FS-rhei-states.3
    if !ready_scan_stops_before_inputs(task, rhei, machines, machine, &state) {
        let entries = missing_state_inputs_for_ready_set(roots, rhei, machine, task, &state);
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
