// What a finished supervising visit released: the test the engine applies
// before it lets that visit's self-loop fire, and the warning it prints when the
// answer is "nothing".
//
// Its own part because it is the one supervision question that reads the whole
// subtree at once. The barrier next door decides what a single applied
// transition means; this decides whether a transition may be applied at all,
// and it is asked wherever agent mode fires the edge — after a visit, and at the
// advance that spawns nothing because the outputs are already there.

// §AR-source-file-size.3 §FS-rhei-supervision.3.6

/// A supervisor's subtree as one visit found it: every descendant's id with the
/// normalized state it was in, in preorder.
///
/// Captured at the spawn and compared after the exit, because "the visit moved
/// the subtree" is a statement about the span the subprocess ran for and about
/// nothing else. A checkpoint cannot answer it: a move the supervisor's own
/// worker makes is its own doing and delivers none (§FS-rhei-supervision.2.1).
// §FS-rhei-supervision.3.6
type SubtreeShape = Vec<(String, String)>;

/// The shape of `task`'s subtree right now. §FS-rhei-supervision.3.6
fn subtree_shape(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> SubtreeShape {
    let mut all = Vec::new();
    collect_plan_tasks(&task.children, &mut all);
    all.iter()
        .map(|child| {
            (
                child.id.to_string(),
                normalized_state_name(child.state.as_str(), machines.for_task(&child.id)),
            )
        })
        .collect()
}

/// The subtree shape of the ticket a spawn is about to run, when that ticket is
/// a supervisor. `None` for every other ticket, so a non-supervising state
/// carries nothing and costs nothing.
// §FS-rhei-supervision.3.6
fn subtree_shape_before_visit(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
    state_name: &str,
) -> Option<SubtreeShape> {
    execute_on_of(machines.for_task(&task.id), state_name)?;
    Some(subtree_shape(task, machines))
}

/// One supervisor and the subtree it holds, as the release test reads them.
///
/// Two callers ask the same question of it: the completion handlers, about the
/// visit that just ended, and the halt classifier, about a supervisor a *past*
/// release already stranded.
// §FS-rhei-supervision.3.6
struct SupervisedSubtree<'a> {
    /// The plan the question is asked of — for a finished visit, the plan as
    /// re-read after the subprocess exited (§FS-rhei-supervision.4.1).
    rhei: &'a rhei_core::ast::Rhei,
    machines: &'a rhei_validator::MachineSet,
    /// The roots a ready-set scan would resolve settings and artifacts against.
    roots: ReadySetRoots<'a>,
    supervisor: &'a rhei_core::ast::Task,
}

/// One finished supervising visit, as the release test reads it.
// §FS-rhei-supervision.3.6
struct FinishedVisit<'a> {
    /// The run's own root: where merged settings are read from, as the
    /// ready-set scan reads them.
    workspace_root: &'a Path,
    /// The plan as re-read after the subprocess exited (§FS-rhei-supervision.4.1).
    plan: &'a LoadedPlan,
    machines: &'a rhei_validator::MachineSet,
    /// The supervisor whose visit just ended, and the state it ran in.
    task_id: &'a TaskId,
    state: &'a str,
    /// Its subtree as the spawn found it. `None` leaves rule 2 unasked.
    before: Option<&'a SubtreeShape>,
    /// Where this spawn recorded itself, so a withheld edge can give back the
    /// attempt it was charged. `None` where nothing was spawned and so nothing
    /// was charged. §FS-rhei-agents.3.2.3
    spawn_record: Option<&'a Path>,
}

/// Withhold this visit's self-loop when it released nothing, and say so — or
/// `None` when the visit released something and the engine advances as it
/// always has.
///
/// The three release rules of §FS-rhei-supervision.3.6, in order: there is no
/// subtree to release, the visit moved it, or it can still move without the
/// supervisor. Anything that is not a supervising state, not a self-loop, or
/// not answerable returns `None` — this rule only ever *withholds* an edge, so
/// every uncertainty resolves to today's behaviour.
///
/// Taking the edge and giving the attempt back are one act, so they are one
/// call: the engine, not the worker, is why the state did not move, and a
/// budget spent on that would bar the supervisor's whole subtree for good.
// §FS-rhei-supervision.3.6 §FS-rhei-agents.3.2.3
fn withhold_empty_supervising_visit(visit: FinishedVisit<'_>) -> Option<String> {
    let machine = visit.machines.for_task(visit.task_id);
    execute_on_of(machine, visit.state)?;
    let task = find_task_by_id(&visit.plan.rhei.tasks, visit.task_id)?;
    // Only the release edge is withheld. A conditioned exit — `visitCount >=
    // visits`, `openDescendants < 1` — is the machine's own decision.
    let selected = find_next_transition(task, &visit.plan.rhei, machine).ok().flatten()?;
    if normalized_state_name(&selected, machine) != visit.state {
        return None;
    }
    // Rule 1: nothing is being held, so nothing is released.
    let open = open_descendant_tasks(task, visit.machines);
    if open.is_empty() {
        return None;
    }
    // Rule 2: the visit steered the subtree; the engine does not ask how.
    if visit.before.is_some_and(|before| *before != subtree_shape(task, visit.machines)) {
        return None;
    }
    // Rule 3: something under the barrier can move once it lifts.
    let held = SupervisedSubtree {
        rhei: &visit.plan.rhei,
        machines: visit.machines,
        roots: ReadySetRoots {
            workspace_root: visit.workspace_root,
            task_roots: &visit.plan.task_roots,
        },
        supervisor: task,
    };
    if supervised_subtree_can_move(&held, &open) {
        return None;
    }
    if let Some(record) = visit.spawn_record {
        uncharge_withheld_visit(record);
    }
    Some(empty_visit_warning(visit.task_id, visit.state, &held, &open))
}

/// The same test at the advance that spawns nothing: `rhei run` reaches the
/// release edge in agent mode whenever the state's declared `outputs:` are
/// already on disk and no invocation is left to run (§FS-rhei-agents.3.2), and
/// a held visit is exactly what leaves those outputs there.
///
/// Rule 2 goes unasked — no subprocess ran, so nothing could have moved while
/// it did — and there is no spawn record, because nothing was spawned and so
/// nothing was charged. `judge` is false for the advances that intended no
/// visit at any state (`--no-agent`, `--no-program`), which §3.6 exempts.
// §FS-rhei-supervision.3.6
fn withhold_empty_supervising_advance(
    judge: bool,
    workspace_root: &Path,
    plan: &LoadedPlan,
    machines: &rhei_validator::MachineSet,
    task_id: &TaskId,
    state: &str,
) -> Option<String> {
    judge.then_some(())?;
    withhold_empty_supervising_visit(FinishedVisit {
        workspace_root,
        plan,
        machines,
        task_id,
        state,
        before: None,
        spawn_record: None,
    })
}

/// Whether any of `open` — the supervisor's non-terminal descendants — can move
/// once the barrier lifts. §FS-rhei-supervision.3.6 rule 3
fn supervised_subtree_can_move(
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> bool {
    let (states, subtree) = subtree_membership(held);
    open.iter().any(|task| descendant_can_still_move(held, &states, &subtree, task))
}

/// Every task's state and who is under the barrier — what rule 3 and the
/// warning that reports it both read. §FS-rhei-supervision.3.6
#[allow(clippy::type_complexity)]
fn subtree_membership<'a>(
    held: &SupervisedSubtree<'a>,
) -> (std::collections::HashMap<&'a TaskId, String>, HashSet<TaskId>) {
    let mut all = Vec::new();
    collect_plan_tasks(&held.rhei.tasks, &mut all);
    let states = plan_state_map(&all, held.machines);
    let subtree = subtree_shape(held.supervisor, held.machines)
        .iter()
        .map(|(id, _)| parse_task_id(id))
        .collect();
    (states, subtree)
}

/// Whether this non-terminal descendant can move once the barrier lifts —
/// either it is in the ready set with its supervisor read as released, or what
/// it waits on is not its supervisor's to give.
///
/// Only a gate answers on its own. Every other way of waiting — a poll's next
/// attempt, a prior held by work elsewhere — decides *when* the descendant is
/// scheduled and puts no file on disk, so each is conjoined with the state's
/// declared `inputs:`: a brief only the supervisor writes is missing whichever
/// clock the descendant is on.
// §FS-rhei-supervision.3.6 rule 3
fn descendant_can_still_move(
    held: &SupervisedSubtree<'_>,
    states: &std::collections::HashMap<&TaskId, String>,
    subtree: &HashSet<TaskId>,
    task: &rhei_core::ast::Task,
) -> bool {
    let machine = held.machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    let Some(state_def) = machine.states.get(&state) else { return false };
    // A human owns the next move of a gate, and moves it with `rhei
    // transition`, which reads no inputs. §FS-rhei-transition-cmd.3
    if state_def.gating {
        return true;
    }
    // A `poll:` state needs nothing here either: time schedules its next
    // attempt, and the inputs check below says whether that attempt could run.
    // A descendant held by a *nested* supervisor needs no special case: that
    // supervisor is itself a descendant, and it answers here for itself.
    prior_blocking_descendant(held, states, subtree, task).is_none()
        && state_inputs_exist_for_ready_set(
            held.roots.workspace_root,
            held.roots.artifact_root(&task.id.to_string()),
            held.rhei,
            machine,
            task,
            &state,
            None,
        )
}

/// The first `**Prior:**` that keeps this descendant from moving once the
/// barrier lifts, as `Task <id> (<state>)` — the shape every other blocked-on
/// row uses — or `None` when none of them does.
///
/// Other work owns a prior outside this subtree that is still open: it is not
/// something the supervisor could have unblocked on this visit. That makes it
/// no reason to call the subtree stranded — and no reason to skip the inputs
/// the descendant will still need when it lands.
///
/// One function for the rule and for the warning, so the run never blames a
/// descendant on a prior the rule did not read as blocking.
// §FS-rhei-supervision.3.6 rule 3
fn prior_blocking_descendant(
    held: &SupervisedSubtree<'_>,
    states: &std::collections::HashMap<&TaskId, String>,
    subtree: &HashSet<TaskId>,
    task: &rhei_core::ast::Task,
) -> Option<String> {
    task.prior.iter().find_map(|prior| {
        let Some(prior_state) = states.get(prior) else {
            return Some(format!("Task {prior} (missing)"));
        };
        let machine = held.machines.for_task(prior);
        if dependency_is_satisfied(prior_state, machine) {
            return None;
        }
        (subtree.contains(prior) || is_terminal_state(prior_state, machine))
            .then(|| format!("Task {prior} ({prior_state})"))
    })
}

/// What a dry run prints for an advance that fires the release edge — or, when
/// §3.6 withholds it, for the edge it will not fire, because a run that will
/// not take an edge must not report that it would.
// §FS-rhei-run.4 §FS-rhei-supervision.3.6
fn format_dry_run_advance(
    withheld: bool,
    task_id: &str,
    from_raw: &str,
    state: &str,
    to: &str,
    machine: &rhei_validator::StateMachine,
) -> String {
    match withheld {
        true => format!(
            "withheld: Task {task_id}  {state} -> {state} \
             (the release edge fires only for a visit that released something)"
        ),
        false => format_dry_run_transition(task_id, from_raw, to, machine),
    }
}

/// The line an empty visit prints: what the visit did not do, what its
/// descendants are waiting for, and the one action that answers it.
///
/// The blocked descendants are named with what each waits on, because that is
/// the sentence an operator can act on — the run's own diagnosis of the same
/// tickets says exactly this, and a warning that only said "released nothing"
/// would send them to the log the visit did not write. The rerun is promised
/// unconditionally because the hold really is unconditional: the withheld edge
/// gives its attempt back, so no budget can run out under this line
/// (§FS-rhei-agents.3.2.3), and the next run's advance is judged by this same
/// test whether it spawns a visit or finds the outputs already there.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
fn empty_visit_warning(
    task_id: &TaskId,
    state: &str,
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> String {
    format!(
        "  holding Task {task_id} in state '{state}': the visit released nothing — it moved no \
         descendant and left none able to move: {}. No transition fires, the visit is not spent \
         and no attempt is charged for it; the ticket stays in '{state}' and every later `rhei \
         run` visits it again. Unblock what the ticket(s) above are waiting for, then rerun.",
        format_blocked_descendants(held, open)
    )
}

/// The open descendants with what each is waiting for, capped at three with a
/// `(+N more)` tail — the shape every other ticket list in the run uses, plus
/// the reason, because that is what an operator can act on.
///
/// Every listed ticket carries one: the files it is missing, or, when it is
/// missing none, the `**Prior:**` rule 3 blamed instead. A bare name would be
/// read against a closing sentence that says to write what it waits for, which
/// for a prior-blocked ticket is nothing.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
fn format_blocked_descendants(
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> String {
    let (states, subtree) = subtree_membership(held);
    let blocked: Vec<String> = open
        .iter()
        .take(3)
        .map(|task| {
            let machine = held.machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            let missing =
                missing_state_inputs_for_ready_set(&held.roots, held.rhei, machine, task, &state);
            if !missing.is_empty() {
                return format!("Task {} ({state}) waits on {}", task.id, missing.join(", "));
            }
            match prior_blocking_descendant(held, &states, &subtree, task) {
                Some(prior) => format!("Task {} ({state}) waits on {prior}", task.id),
                None => format!("Task {} ({state})", task.id),
            }
        })
        .collect();
    let more = open.len().saturating_sub(blocked.len());
    match more {
        0 => blocked.join("; "),
        more => format!("{} (+{more} more)", blocked.join("; ")),
    }
}

/// The reason a *released* supervisor is not moving, when nothing beneath it
/// can move either — a workspace stranded by an empty visit before
/// §FS-rhei-supervision.3.6 withheld that self-loop.
///
/// Nothing can wake it: a released supervisor is scheduled only by a descendant
/// checkpoint, and a subtree where nothing can move produces none. The run used
/// to read this as "not scheduled … rerun to pick it up", which is the one
/// remedy that provably does nothing.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
fn stranded_released_supervisor(
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> Option<String> {
    if open.is_empty()
        || recorded_supervision_phase(held.rhei.metadata.as_ref(), &held.supervisor.id)
            != Some(SupervisionPhase::Released)
    {
        return None;
    }
    (!supervised_subtree_can_move(held, open)).then(|| format_blocked_descendants(held, open))
}
