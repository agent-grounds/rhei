// The barrier itself: what one applied transition changes about supervision —
// the moving task's own phase, and the checkpoint its nearest supervising
// ancestor is owed — and which tasks the barrier admits to the ready set.
//
// Its own part because this reads the plan tree and decides, while the metadata
// block next door only stores what it decided.

// §AR-source-file-size.3 §FS-rhei-supervision.2 §FS-rhei-supervision.3

// ---------------------------------------------------------------------------
// The shared transition path
// ---------------------------------------------------------------------------

/// Whether a supervisor is working right now, so a move under it is its own
/// doing rather than news for it.
///
/// Two facts answer it: the `**Assignee:**` a manual claim writes, or an
/// explicit operation context naming this exact ancestor. The latter lives
/// only for one `rhei transition` command, so an independently launched nested
/// Rhei inherits no authority over this tree.
// §FS-rhei-supervision.2.1 §FS-rhei-supervision.3.2
fn supervisor_is_in_flight(
    supervisor: &rhei_core::ast::Task,
    operation_supervisor: Option<&TaskId>,
) -> bool {
    supervisor.assignee.is_some()
        || operation_supervisor.is_some_and(|active| active == &supervisor.id)
}

/// One applied transition, as supervision reads it.
struct SupervisionTransition<'a> {
    machine: &'a rhei_validator::StateMachine,
    /// The transitioning task, as re-read from the plan.
    task: &'a rhei_core::ast::Task,
    /// Owner selected by scope before the move's event is considered.
    supervising_owner: Option<&'a rhei_core::ast::Task>,
    /// `metadata.tasks.<id>` key of the transitioning task.
    metadata_key: &'a TaskId,
    /// What turns a rhei-local id into a metadata key. Empty outside the basin,
    /// whose metadata shares the project manifest under qualified ids.
    metadata_prefix: &'a str,
    /// The transitioning task's rhei-local id: what a checkpoint records.
    local_id: &'a str,
    from: &'a str,
    /// The *effective* target, after any callback redirect.
    to: &'a str,
    /// Visit number of `to` after the move.
    to_visit: u64,
    /// Supervisor authority carried by this one explicit transition command.
    operation_supervisor: Option<&'a TaskId>,
}

impl SupervisionTransition<'_> {
    fn to_is_terminal(&self) -> bool {
        self.machine.states.get(self.to).map(|def| def.terminal).unwrap_or(false)
    }

    /// The checkpoint this move produces for the nearest supervising ancestor,
    /// with the ancestor it is owed to. `None` when the move is not news.
    // §FS-rhei-supervision.2.1 §FS-rhei-supervision.2.2
    fn checkpoint(&self) -> Option<(&rhei_core::ast::Task, SupervisionCheckpoint)> {
        if self.from == self.to {
            // A poll attempt is a retry, and a supervisor's own release edge is
            // the supervisor waiting: neither is the subtree progressing.
            let from_def = self.machine.states.get(self.from);
            if from_def.map(|def| def.poll.is_some()).unwrap_or(false)
                || from_def.and_then(|def| def.execute_on()).is_some()
            {
                return None;
            }
        }
        let supervisor = self.supervising_owner?;
        let execute_on = execute_on_of(
            self.machine,
            &normalized_state_name(supervisor.state.as_str(), self.machine),
        )?;
        // §FS-rhei-supervision.2.1: the event narrows what the move has to be,
        // once the scope has said whose move it is.
        if execute_on.event() == rhei_validator::SupervisionEvent::Terminal && !self.to_is_terminal()
        {
            return None;
        }
        if supervisor_is_in_flight(supervisor, self.operation_supervisor) {
            return None;
        }
        Some((
            supervisor,
            SupervisionCheckpoint {
                task: self.local_id.to_string(),
                from: self.from.to_string(),
                to: self.to.to_string(),
                visit: self.to_visit.max(1),
            },
        ))
    }
}

/// The ancestor's id as its own task file spells it.
///
/// The transitioning task's rhei-local id is the qualified one minus a fixed
/// prefix, and every ancestor shares that prefix, so the ancestor's local id
/// falls out of the same subtraction.
fn supervisor_local_id(
    supervisor: &rhei_core::ast::Task,
    local_id: &str,
    task: &rhei_core::ast::Task,
) -> String {
    let qualified = task.id.to_string();
    let prefix = qualified.strip_suffix(local_id).unwrap_or("");
    supervisor.id.to_string().strip_prefix(prefix).unwrap_or(&supervisor.id.to_string()).to_string()
}

/// Fold this transition's supervision effects into the metadata about to be
/// written: the moving task's own phase, and the checkpoint its nearest
/// supervising ancestor is owed.
///
/// It runs on the shared path so `rhei run`'s auto-advance, `rhei transition`,
/// `rhei complete`, and a callback redirect maintain the barrier identically.
// §FS-rhei-supervision.2 §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.3
fn apply_supervision_transition(
    existing: Option<&Metadata>,
    move_: SupervisionTransition<'_>,
) -> Option<Metadata> {
    let from_supervises = execute_on_of(move_.machine, move_.from).is_some();
    let to_supervises = execute_on_of(move_.machine, move_.to).is_some();
    let to_is_gating =
        move_.machine.states.get(move_.to).map(|def| def.gating).unwrap_or(false);

    let mut updated: Option<Metadata> = None;

    if from_supervises && move_.from == move_.to {
        // §FS-rhei-supervision.3.1: the self-loop releases the subtree.
        updated =
            Some(record_supervision_release(updated.as_ref().or(existing), move_.metadata_key));
    } else if from_supervises && to_is_gating {
        // A supervisor parked at a human gate keeps its block, so its subtree
        // stays held; the visit that took this edge consumed its checkpoints,
        // so the list starts empty. §FS-rhei-supervision.3.1
        updated = Some(record_supervision_hold(
            updated.as_ref().or(existing),
            move_.metadata_key,
            None,
        ));
    } else if from_supervises {
        updated = clear_supervision_for_task(updated.as_ref().or(existing), move_.metadata_key);
    } else if move_.from != move_.to
        && recorded_supervision_phase(updated.as_ref().or(existing), move_.metadata_key)
            .is_some()
    {
        // The human moved a gate-parked supervisor on: the hold ends here,
        // wherever it is going. §FS-rhei-supervision.3.1
        updated = clear_supervision_for_task(updated.as_ref().or(existing), move_.metadata_key);
    }
    if to_supervises && move_.from != move_.to {
        // §FS-rhei-supervision.3.1: entry holds, with no checkpoints yet.
        updated = Some(record_supervision_hold(
            updated.as_ref().or(existing),
            move_.metadata_key,
            None,
        ));
    }

    if let Some((supervisor, checkpoint)) = move_.checkpoint() {
        let supervisor_key = parse_task_id(&format!(
            "{}{}",
            move_.metadata_prefix,
            supervisor_local_id(supervisor, move_.local_id, move_.task)
        ));
        updated = Some(record_supervision_hold(
            updated.as_ref().or(existing),
            &supervisor_key,
            Some(&checkpoint),
        ));
    }

    updated
}

/// Whether this edge ends a supervisor's visit.
///
/// The release self-loop is the one non-terminal edge that drops a
/// `**Assignee:**`: the visit it was claimed for is over, and a claim that
/// outlived it would read as "the supervisor is working right now" — every
/// later descendant exit taken for the supervisor's own doing (§2.1), and the
/// supervisor itself never scheduled again.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.4
fn transition_ends_supervisor_visit(
    machine: &rhei_validator::StateMachine,
    from: &str,
    to: &str,
) -> bool {
    from == to && execute_on_of(machine, from).is_some()
}

/// Say, once, that a supervisor left supervision for a human gate.
///
/// This is the one transition where the barrier outlives the supervising state,
/// and it is invisible otherwise: the subtree simply stops, and a reader who
/// does not know the rule sees a plan that stalled for no reason. Routed through
/// the run's diagnostic sink so it lands in the TUI journal rather than on top
/// of a rendered frame.
// §FS-rhei-supervision.3.1
fn announce_supervision_gate_handoff(
    machine: &rhei_validator::StateMachine,
    task_id: &str,
    from: &str,
    to: &str,
) {
    if from == to || execute_on_of(machine, from).is_none() {
        return;
    }
    if !machine.states.get(to).map(|def| def.gating).unwrap_or(false) {
        return;
    }
    emit_run_diag(
        rhei_tui::MessageLevel::Warn,
        format!(
            "Task {task_id} left supervision for human gate '{to}'; its subtree stays held \
             until a human moves it"
        ),
    );
}

/// The move itself, as the shared transition path knows it: which task moved,
/// under which metadata key, between which two states, and which visit of the
/// target this entry is. One value because no caller has any of these without
/// the rest, and supervision reads them only together.
// §FS-rhei-supervision.3.3
#[derive(Clone, Copy)]
struct AppliedTransition<'a> {
    /// `metadata.tasks.<id>` key of the transitioning task.
    metadata_key: &'a TaskId,
    /// The transitioning task's rhei-local id: what a checkpoint records.
    local_id: &'a str,
    from: &'a str,
    /// The *effective* target, after any callback redirect.
    to: &'a str,
    /// Visit number of `to` after the move.
    to_visit: u64,
}

/// Bind one applied transition on the shared path to the supervision rules.
///
/// The shared path knows the transitioning task, the files its rewrites land
/// in, and the move it is about to commit; everything supervision needs falls
/// out of those three.
// §FS-rhei-supervision.2 §FS-rhei-supervision.3.3
fn supervision_after_transition(
    existing: Option<&Metadata>,
    machine: &rhei_validator::StateMachine,
    task_info: &TransitionTaskInfo,
    files: TransitionFiles<'_>,
    applied: AppliedTransition<'_>,
    supervising_owner: Option<&rhei_core::ast::Task>,
    operation_supervisor: Option<&TaskId>,
) -> Option<Metadata> {
    let AppliedTransition { metadata_key, local_id, from, to, to_visit } = applied;
    announce_supervision_gate_handoff(machine, files.artifact_id, from, to);
    apply_supervision_transition(
        existing,
        SupervisionTransition {
            machine,
            task: &task_info.task,
            supervising_owner,
            metadata_key,
            metadata_prefix: files.metadata_id.strip_suffix(local_id).unwrap_or(""),
            local_id,
            from,
            to,
            to_visit,
            operation_supervisor,
        },
    )
}

// ---------------------------------------------------------------------------
// Readiness
// ---------------------------------------------------------------------------

/// What supervision says about scheduling one task. §FS-rhei-supervision.3.2
#[derive(Debug, Clone, PartialEq, Eq)]
enum SupervisionVerdict {
    /// Nothing on this task's path supervises: today's rules decide.
    Unsupervised,
    /// The task is in a supervising state and owed a visit, with nothing
    /// beneath it in flight. The "every descendant is terminal" rule does not
    /// apply to it.
    SupervisorReady,
    /// The task is in a supervising state but must not run: it has released
    /// its subtree, or something beneath it is still in flight.
    SupervisorWaiting,
    /// A supervising ancestor holds it.
    Held { supervisor: TaskId, state: String },
}

/// Index every task in the plan by id, for the ancestor walks readiness needs.
fn task_index<'a>(
    tasks: &[&'a rhei_core::ast::Task],
) -> std::collections::HashMap<TaskId, &'a rhei_core::ast::Task> {
    tasks.iter().map(|task| (task.id.clone(), *task)).collect()
}

/// Whether anything beneath `task` is being worked right now.
// §FS-rhei-supervision.3.2: in flight is a spawned-and-unexited run, or a claim.
fn any_descendant_in_flight(
    task: &rhei_core::ast::Task,
    in_flight: &dyn Fn(&rhei_core::ast::Task) -> bool,
) -> bool {
    task.children
        .iter()
        .any(|child| in_flight(child) || any_descendant_in_flight(child, in_flight))
}

/// Whether this task's `supervision` block holds its subtree right now.
///
/// The **block** is the hold, not the state. A task in a supervising state with
/// no block is held — the authored-initial case — but a task that left its
/// supervising state for a human gate keeps its block, and keeps its subtree
/// held, until a human moves it on. Reading the state alone un-supervised a
/// whole subtree the moment a budget ran out.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.2
fn supervision_holds_subtree(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
) -> bool {
    match recorded_supervision_phase(metadata, &task.id) {
        Some(phase) => phase == SupervisionPhase::Held,
        None => task_is_supervising(task, machine),
    }
}

/// Apply the hold/release rule to one task.
///
/// The ancestors are consulted first and all the way up: a supervisor higher in
/// the tree holds everything beneath it, nested supervisors included, so a
/// released inner supervisor cannot let its own children out from under a held
/// outer one.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.2
fn supervision_verdict(
    task: &rhei_core::ast::Task,
    index: &std::collections::HashMap<TaskId, &rhei_core::ast::Task>,
    machines: &rhei_validator::MachineSet,
    metadata: Option<&Metadata>,
    in_flight: &dyn Fn(&rhei_core::ast::Task) -> bool,
) -> SupervisionVerdict {
    let mut cursor = task.id.parent();
    while let Some(id) = cursor {
        let Some(ancestor) = index.get(&id) else { break };
        let machine = machines.for_task(&ancestor.id);
        if supervision_holds_subtree(ancestor, machine, metadata)
            || (task_is_supervising(ancestor, machine) && in_flight(ancestor))
        {
            return SupervisionVerdict::Held {
                supervisor: ancestor.id.clone(),
                state: normalized_state_name(ancestor.state.as_str(), machine),
            };
        }
        cursor = id.parent();
    }

    let machine = machines.for_task(&task.id);
    if !task_is_supervising(task, machine) {
        return SupervisionVerdict::Unsupervised;
    }
    // §FS-rhei-supervision.3.1: the drain — siblings already running finish
    // before the supervisor sees the checkpoints they produced.
    if any_descendant_in_flight(task, in_flight) {
        return SupervisionVerdict::SupervisorWaiting;
    }
    match supervision_phase(metadata, &task.id) {
        SupervisionPhase::Held => SupervisionVerdict::SupervisorReady,
        SupervisionPhase::Released => SupervisionVerdict::SupervisorWaiting,
    }
}

/// The verdict for a task whose in-flight set is whatever the plan shows: a
/// `**Assignee:**` claim, plus the ids a live run says it has spawned.
// §FS-rhei-supervision.3.2
fn supervision_verdict_for(
    task: &rhei_core::ast::Task,
    index: &std::collections::HashMap<TaskId, &rhei_core::ast::Task>,
    machines: &rhei_validator::MachineSet,
    metadata: Option<&Metadata>,
    spawned: &HashSet<String>,
) -> SupervisionVerdict {
    let in_flight = |candidate: &rhei_core::ast::Task| {
        candidate.assignee.is_some() || spawned.contains(&candidate.id.to_string())
    };
    supervision_verdict(task, index, machines, metadata, &in_flight)
}

/// Whether this task is work anyone can be handed right now, as far as the
/// subtree beneath it is concerned.
///
/// The ready set's subtree rule, named so the surfaces that *explain* a refusal
/// ask the same question the scan did: a supervisor owed a visit is work while
/// its subtree is open, a released or draining one is not, a held descendant is
/// not, and anything unsupervised keeps the non-leaf rule it always had.
// §FS-rhei-supervision.3.2
fn subtree_admits_to_ready_set(
    task: &rhei_core::ast::Task,
    index: &std::collections::HashMap<TaskId, &rhei_core::ast::Task>,
    machines: &rhei_validator::MachineSet,
    metadata: Option<&Metadata>,
    spawned: &HashSet<String>,
) -> bool {
    match supervision_verdict_for(task, index, machines, metadata, spawned) {
        SupervisionVerdict::SupervisorReady => true,
        SupervisionVerdict::SupervisorWaiting | SupervisionVerdict::Held { .. } => false,
        SupervisionVerdict::Unsupervised => descendants_are_terminal(task, machines),
    }
}

/// Who holds a ticket, and whether that holder can still release it on its own.
///
/// A supervisor parked at a human gate keeps its block (§3.1 rule 4), so "the
/// supervisor releases it on its next visit" stops being true: there is no next
/// visit until a human moves the supervisor.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.4
struct SupervisorHold {
    supervisor: TaskId,
    state: String,
    /// The holder is itself in a `gating: true` state.
    awaiting_human: bool,
}

/// The supervisor holding `task`, when one does — for the surfaces that explain
/// why a ticket is not moving. §FS-rhei-supervision.3.4
fn held_by_supervisor(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> Option<SupervisorHold> {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let index = task_index(&all);
    match supervision_verdict_for(
        task,
        &index,
        machines,
        rhei.metadata.as_ref(),
        &HashSet::new(),
    ) {
        SupervisionVerdict::Held { supervisor, state } => {
            let awaiting_human = machines
                .for_task(&supervisor)
                .states
                .get(&state)
                .map(|def| def.gating)
                .unwrap_or(false);
            Some(SupervisorHold { supervisor, state, awaiting_human })
        }
        _ => None,
    }
}
