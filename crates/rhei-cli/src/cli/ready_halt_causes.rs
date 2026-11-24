// Why one non-terminal ticket is not moving: the one classification every
// surface uses — `rhei run`'s halt message, its dry run, and the durable
// report's Attention table — and the lines it renders. Choosing the cause is
// `ready_halt_classify.rs`, included at the end.
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
    /// Account verification refused work before a program could start.
    /// §FS-rhei-budgets.9
    AccountRefused { diagnosis: String, remedy: String },
    /// A spent bound refused this ticket's last start. `blocker` and
    /// `next_action` are already rendered from the run's own `budget_halt`, so
    /// the row cannot disagree with that record. Never a Waiting reading: nothing
    /// but the bound releases it, whatever else the ticket waits on.
    /// §FS-rhei-run-report.3.1 §FS-rhei-budgets.8
    BoundHalted { blocker: String, next_action: String },
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
    /// A poll that declares no person, held only by its own next attempt, which
    /// rhei takes by itself once that instant passes. Without this it read as
    /// never scheduled, under a `!` and remedies for a ticket that was fine.
    // §FS-rhei-run-report.3.1 §FS-rhei-run.5.1
    WaitingOnPoll { next_attempt_at: String },
    /// A supported provider has supplied a durable safe retry deadline. This
    /// is deliberate waiting, not attention. §FS-rhei-run-report.3.1
    /// `kind` says whether a refusal or an unavailability set it. §FS-rhei-run.3.3
    ProviderLimited { provider: String, next_attempt_at: String, kind: ProviderWaitKind },
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
    /// `reset` is the one that always can, as §FS-rhei-reset.5.1 spells it.
    // §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
    SupervisorStrandedBySelfLoop { open: String, reset: String },
    /// A live `**Assignee:**`; the scheduler never schedules a claimed ticket.
    Claimed { assignee: String },
    /// An unsatisfied `**Prior:**`, already formatted as `Task <id> (<state>)`.
    /// `cancelled` when the prior is in a cancellation state, so it can never
    /// finish and "finish the prior first" would be advice nobody can follow.
    // §FS-rhei-run-report.3.1 §FS-rhei-states.1.4
    BlockedByPrior { prior: String, cancelled: bool },
    /// Manual-only initial state: `rhei run` must not advance it.
    ManualOnly { to: String },
    /// Non-terminal with no declared outgoing transition to take.
    NoTransition,
    /// A finished worker's selected forward edge, refused by its destination: a
    /// spent `visits:` budget or a missing required input.
    /// §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1
    RefusedDestination { refusal: DestinationRefusal },
    /// The run was interrupted while this ticket's worker was in flight.
    /// Nothing about the ticket failed and nothing about it is waiting on a
    /// human: the run was stopped, and re-running it is the whole recovery.
    /// Reported ahead of `MissingOutputs` because it explains it — a worker
    /// the run killed had no chance to write what it owed.
    // §FS-rhei-run-report.3.1 §FS-rhei-run.3.2
    Interrupted,
    /// Every pending invocation has spent its effective current-visit allowance.
    /// The checked artifacts are rooted at the owning rhei; only retry releases
    /// this hold, including in the run that consumed its last attempt.
    /// §FS-rhei-run-report.3.1
    SpentVisit {
        attempts: u64,
        entries: Vec<String>,
        /// When the visit's work met its completion condition, the destination's
        /// refusal of the edge it selected, named instead. §FS-rhei-errors.1.5.2
        refused: Option<DestinationRefusal>,
        /// Diagnostics belong to the same invocation, even when it spent the visit.
        /// §FS-rhei-configuration-fallbacks
        configuration_fallbacks: Vec<String>,
    },
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
    ///
    /// `configuration_fallbacks` are the Codex configuration fallbacks that
    /// same invocation reported, which say why it wrote nothing.
    /// §FS-rhei-configuration-fallbacks
    // §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1 §FS-rhei-errors.2
    MissingOutputs {
        entries: Vec<String>,
        exit_code: Option<i32>,
        plan: String,
        configuration_fallbacks: Vec<String>,
    },
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
    /// Only when the missing file is what readiness stopped at, though: a
    /// draining supervisor is refused before inputs are read. A clock never is.
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
    /// Whether this reading can be true of a ticket a bound refused, so a recorded
    /// halt may stand in for it.
    ///
    /// Only a ticket the scheduler was starting reaches a bound, so the first group
    /// is what the classifier says of one it merely saw not move: a poll's person,
    /// a poll's own deadline or a manual claim would not release it — at that
    /// deadline the bound refuses it again. A manual-only ticket with no agent
    /// configured is never started, so it has no halt and keeps its row.
    ///
    /// The second group means something moved the ticket after the halt, or that a
    /// wait the run honours now holds it, and it is the truer row. One of them is
    /// different: a supervisor refused on its wake-up does read
    /// `SupervisorHasNoTerminalEdge`, with nothing having moved it. That names a
    /// machine defect that outranks the bound and that no renewal cures, and
    /// `renews automatically` would promise a resumption that cannot finish the
    /// ticket, so its row stands and `## Bounds` lists the halt.
    /// Spelled out so a new cause has to choose. §FS-rhei-run-report.3.1
    fn yields_to_bound_halt(&self) -> bool {
        match self {
            HaltCause::WaitingOnPerson { .. }
            | HaltCause::WaitingOnPoll { .. }
            | HaltCause::ManualOnly { .. }
            | HaltCause::MissingOutputs { .. }
            | HaltCause::NoTransition
            | HaltCause::NotScheduled
            | HaltCause::Stalled => true,
            HaltCause::AccountRefused { .. }
            | HaltCause::BoundHalted { .. }
            | HaltCause::WaitingOnDescendants { .. }
            | HaltCause::Gate
            | HaltCause::GateHoldingSubtree { .. }
            | HaltCause::ProviderLimited { .. }
            | HaltCause::HeldBySupervisor { .. }
            | HaltCause::SupervisorHasNoTerminalEdge { .. }
            | HaltCause::SupervisorStrandedBySelfLoop { .. }
            | HaltCause::Claimed { .. }
            | HaltCause::BlockedByPrior { .. }
            | HaltCause::Interrupted
            | HaltCause::SpentVisit { .. }
            | HaltCause::RefusedDestination { .. }
            | HaltCause::MissingInputs { .. } => false,
        }
    }

    /// The reason and the next action, for the report table and the halt
    /// diagnostics. Both name concrete commands wherever one exists.
    fn describe(&self, id: &str, state: &str) -> (String, String) {
        match self {
            // Preserve the account's diagnosis, paths and guidance. §FS-rhei-budgets.9
            HaltCause::AccountRefused { diagnosis, remedy } => (diagnosis.clone(), remedy.clone()),
            // §FS-rhei-run-report.3.1: the bound, and the one remedy for it.
            HaltCause::BoundHalted { blocker, next_action } => {
                (blocker.clone(), next_action.clone())
            }
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
            HaltCause::SupervisorStrandedBySelfLoop { open, reset } => (
                format!(
                    "released its subtree on a visit that changed nothing, and nothing beneath \
                     it can move: {open}"
                ),
                format!(
                    "a released supervisor is woken only by a descendant that moves: unblock \
                     one of the tickets above and rerun, or run {reset}"
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
            // §FS-rhei-run-report.3.1: the instant, and nothing for anyone to do.
            HaltCause::WaitingOnPoll { next_attempt_at } => (
                format!("waiting on its poll until {next_attempt_at}"),
                format!("nothing to do on Task {id}; rhei resumes it itself"),
            ),
            HaltCause::ProviderLimited { provider, next_attempt_at, kind } => (
                match kind {
                    ProviderWaitKind::Limit => format!("provider {provider} limited until {next_attempt_at}"),
                    ProviderWaitKind::Unavailable => {
                        format!("provider {provider} unavailable until {next_attempt_at}")
                    }
                },
                format!("nothing to do on Task {id}; rhei resumes it automatically"),
            ),
            HaltCause::Claimed { assignee } => (
                format!("claimed by {assignee}"),
                format!(
                    "`rhei release {id}` to hand it back, or `rhei complete {id} --result …` \
                     to finish it"
                ),
            ),
            // A cancelled prior is final and never satisfies a dependent, so
            // the row names the remedies that exist. §FS-rhei-run-report.3.1
            HaltCause::BlockedByPrior { prior, cancelled: true } => (
                format!("waiting on {prior}, a cancelled prior that can never satisfy it"),
                format!(
                    "re-point `**Prior:**` (and any `**Consumes:**` naming it) at a completed \
                     step, or cancel Task {id}"
                ),
            ),
            HaltCause::BlockedByPrior { prior, cancelled: false } => (
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
            // The same measured refusal as the successful worker's warning.
            // §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
            HaltCause::RefusedDestination { refusal } => refusal.attention(),
            // Not "stalled", and emphatically not "mark the task cancelled":
            // the operator stopped the run, so the run says so and asks for
            // nothing else. §FS-rhei-run-report.3.1
            HaltCause::Interrupted => (
                format!("run interrupted while its worker was in state {state}"),
                "re-run to continue".to_string(),
            ),
            // §FS-rhei-run-report.3.1: one cause and recovery on every halt surface.
            HaltCause::SpentVisit { attempts, entries, refused, configuration_fallbacks } => {
                // The run tried the edge on the visit's finished work, so its refusal
                // is the hold, released before the grant. §FS-rhei-errors.1.5.2
                let (held, release) = match refused {
                    Some(refusal) => (
                        format!("its work finished, but {}", refusal.edge()),
                        format!("{}, then ", refusal.release()),
                    ),
                    None => {
                        (format!("still unmet: {}", completion_debt_label(entries)), String::new())
                    }
                };
                (
                    format!("{attempts} attempts spent on this visit; {held}")
                        + &configuration_fallback_clauses(configuration_fallbacks),
                    format!("{release}`rhei retry {id}`, then rerun"),
                )
            }
            // Name the files. The whole point of this cause is that the operator
            // does not have to go read a log to learn which one is missing.
            // §FS-rhei-run-report.3.1
            HaltCause::MissingOutputs { entries, exit_code, plan, configuration_fallbacks } => (
                match exit_code {
                    Some(code) => format!("worker exited {code} without {}", entries.join(", ")),
                    None => format!("worker exited without {}", entries.join(", ")),
                } + &configuration_fallback_clauses(configuration_fallbacks),
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

include!("ready_halt_classify.rs");
