fn state_inputs_exist_for_ready_set(
    workspace_root: &Path,
    artifact_root: &Path,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    state_name: &str,
) -> bool {
    let Some(state_def) = machine.states.get(state_name) else {
        return false;
    };
    if state_def.inputs.is_empty() {
        return true;
    }
    let settings = match load_merged_settings(workspace_root) {
        Ok(settings) => settings,
        Err(_) => return false,
    };
    let visit_count = Some(render_visit_count(
        rhei.metadata.as_ref(),
        &task.id,
        state_name,
        task.state.as_str(),
        machine,
    ));
    ensure_state_inputs_exist_for_transition(
        artifact_root,
        Some(task),
        &task.id.to_string(),
        state_name,
        state_def,
        visit_count,
        machine,
        &settings,
        "",
    )
    .is_ok()
}

/// The roots the ready-set scan resolves a ticket's artifacts against: the
/// plan's own root, and the per-ticket execution roots a Panta project adds.
///
/// One value because the two are only ever right together. As separate
/// arguments the mistake type-checks, and it was made: `find_runnable_tasks`
/// and `find_held_tasks` handed a plan's root beside an empty map for as long
/// as Panta members have had roots of their own, so a member's inputs were
/// looked for under the project. Everything that reads the ready set — the
/// scan, its wrappers, and the halt classifier that explains what it refused —
/// takes the pair.
// §AR-rhei-panta.5
struct ReadySetRoots<'a> {
    workspace_root: &'a Path,
    task_roots: &'a std::collections::HashMap<String, std::path::PathBuf>,
}

impl<'a> ReadySetRoots<'a> {
    /// Where `task_id`'s artifacts live: its owning rhei's execution root in a
    /// Panta project, the plan's own root otherwise. §AR-rhei-panta.5
    fn artifact_root(&self, task_id: &str) -> &'a Path {
        self.task_roots.get(task_id).map_or(self.workspace_root, |root| root.as_path())
    }

    /// A plan with no per-ticket roots at all: a single-file plan, the shape
    /// the unit tests build. A loaded Panta project always carries roots, so
    /// nothing on the run path constructs this. §AR-rhei-panta.5
    #[cfg(test)]
    fn plan_only(workspace_root: &'a Path) -> Self {
        type TaskRoots = std::collections::HashMap<String, std::path::PathBuf>;
        static EMPTY: std::sync::OnceLock<TaskRoots> = std::sync::OnceLock::new();
        Self { workspace_root, task_roots: EMPTY.get_or_init(TaskRoots::new) }
    }
}

/// The required inputs the ready-set scan could not find for `task` in
/// `state_name`; empty when the state may start, and empty when nothing can be
/// determined — a halt line must name files, never guess at them.
///
/// Resolved exactly as [`state_inputs_exist_for_ready_set`] resolves them, so
/// the halt report names the files that scan looked for.
// §AR-rhei-panta.5 §FS-rhei-run-report.3.1
fn missing_state_inputs_for_ready_set(
    roots: &ReadySetRoots<'_>,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    state_name: &str,
) -> Vec<String> {
    let Some(state_def) = machine.states.get(state_name) else {
        return Vec::new();
    };
    if state_def.inputs.is_empty() {
        return Vec::new();
    }
    let Ok(settings) = load_merged_settings(roots.workspace_root) else {
        return Vec::new();
    };
    let task_id = task.id.to_string();
    let visit_count = Some(render_visit_count(
        rhei.metadata.as_ref(),
        &task.id,
        state_name,
        task.state.as_str(),
        machine,
    ));
    missing_state_inputs_for_transition(
        roots.artifact_root(&task_id),
        Some(task),
        &task_id,
        state_name,
        state_def,
        visit_count,
        machine,
        &settings,
    )
}

/// Whether the ready-set scan refused this ticket *before* it ever looked at
/// the state's `inputs:`.
///
/// Two skips in [`find_ready_tasks`] come first and have no halt cause of their
/// own: a `poll:` state whose next attempt is still in the future, and a
/// supervisor that has released its subtree or still has work draining beneath
/// it. Either can be true at the same time as a missing input, and naming the
/// input then is the less useful of two true readings — the operator writes the
/// file, reruns, and the ticket still does not schedule.
// §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.2
fn ready_scan_stops_before_inputs(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    machine: &rhei_validator::StateMachine,
    state_name: &str,
) -> bool {
    if machine.states.get(state_name).and_then(|def| def.poll.as_ref()).is_some()
        && poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, state_name)
            .is_some_and(|deadline| deadline > current_unix_secs())
    {
        return true;
    }
    if !task_is_supervising(task, machine) {
        return false;
    }
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let verdict = supervision_verdict_for(
        task,
        &task_index(&all),
        machines,
        rhei.metadata.as_ref(),
        &HashSet::new(),
    );
    matches!(verdict, SupervisionVerdict::SupervisorWaiting)
}

/// Every descendant of `task` that is not yet terminal, in preorder.
// §DA-per-rhei-state-machines: each node is judged under the machine its own
// id resolves to, so a mixed-machine project cannot misread a child's state.
fn open_descendant_tasks<'a>(
    task: &'a rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> Vec<&'a rhei_core::ast::Task> {
    fn recurse<'a>(
        task: &'a rhei_core::ast::Task,
        machines: &rhei_validator::MachineSet,
        out: &mut Vec<&'a rhei_core::ast::Task>,
    ) {
        for child in &task.children {
            if !is_terminal_state(child.state.as_str(), machines.for_task(&child.id)) {
                out.push(child);
            }
            recurse(child, machines, out);
        }
    }
    let mut out = Vec::new();
    recurse(task, machines, &mut out);
    out
}

/// Whether `task` has any non-terminal descendant.
///
/// The eligibility rule below only asks whether the subtree is closed, and it
/// is asked for every node on every scheduling pass, so this stops at the first
/// open node instead of materializing the whole list.
// §DA-per-rhei-state-machines: each node is judged under the machine its own
// id resolves to, so a mixed-machine project cannot misread a child's state.
fn any_open_descendant(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> bool {
    task.children.iter().any(|child| {
        !is_terminal_state(child.state.as_str(), machines.for_task(&child.id))
            || any_open_descendant(child, machines)
    })
}

/// The one eligibility rule for non-leaf tasks, shared by `rhei next` and
/// `rhei run` instead of splitting on whether the node has children.
// §FS-rhei-plan-language.3 §FS-rhei-next.3: workable once the subtree is
// terminal; a leaf satisfies it trivially.
fn descendants_are_terminal(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> bool {
    !any_open_descendant(task, machines)
}

/// `Task <id> (<state>)` for each open descendant, capped at three with a
/// `(+N more)` tail — the shape every other ticket list in this module uses.
fn format_open_descendants(
    open: &[&rhei_core::ast::Task],
    machines: &rhei_validator::MachineSet,
) -> String {
    let items: Vec<String> = open
        .iter()
        .take(3)
        .map(|task| {
            format!(
                "Task {} ({})",
                task.id,
                normalized_state_name(task.state.as_str(), machines.for_task(&task.id))
            )
        })
        .collect();
    let suffix =
        if open.len() > 3 { format!(" (+{} more)", open.len() - 3) } else { String::new() };
    format!("{}{}", items.join(", "), suffix)
}

/// Find tasks that are ready to advance — the one definition of readiness, that
/// `rhei run`, `rhei next` and `rhei list --ready` all answer from.
///
/// A task is ready when every descendant is terminal, its own state is one the
/// machine declares and is neither terminal nor gating, no `poll:` deadline
/// recorded for that state is still ahead, every task in its `**Prior:**` field
/// is in a successful terminal state, and every required `inputs:` artifact
/// that state declares exists under the ticket's own artifact root. An
/// `optional: true` input is not part of it.
///
/// A state the machine does not declare fails the input check outright: nothing
/// can be said about readiness in a state that does not exist, and every caller
/// that schedules validates the plan first, so only a lenient reader gets here.
///
/// Assignees and the initial state are deliberately absent: `rhei next` narrows
/// this set by both, and `rhei list --ready` reports the set itself.
///
/// `spawned` names the tickets a live run has an invocation out for. It only
/// matters under supervision, where a supervisor is ready once its subtree is
/// quiescent rather than once its subtree is terminal, so a caller with no run
/// behind it passes an empty set.
///
/// Returns task references in source order.
// §FS-rhei-supervision.3.2: the ready set's one supervision refinement.
fn find_ready_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
    spawned: &HashSet<String>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks_in_view(rhei, machines, roots, spawned, ReadySetView::RunnableNow)
}

/// Apply the one readiness implementation through either its ordinary
/// runnable-now view or the program mode probe that alone overlooks a future
/// poll deadline. Every other eligibility rule remains shared. §FS-rhei-run.3
fn find_ready_tasks_in_view<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
    spawned: &HashSet<String>,
    view: ReadySetView,
) -> Vec<&'a rhei_core::ast::Task> {
    use std::collections::HashMap;

    let mut all_tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all_tasks);
    let index = task_index(&all_tasks);

    // Build a map of every task node's state for dependency lookups, each
    // normalized under its owning rhei's machine. §FS-rhei-run.3
    let state_map: HashMap<&TaskId, String> = all_tasks
        .iter()
        .map(|t| (&t.id, normalized_state_name(t.state.as_str(), machines.for_task(&t.id))))
        .collect();

    let mut ready = Vec::new();

    for task in &all_tasks {
        let task = *task;
        // §FS-rhei-supervision.3.2: a supervisor is scheduled *between* its
        // descendants, and everything under a held one waits — the one
        // declared refinement of the eligibility rule below.
        match supervision_verdict_for(task, &index, machines, rhei.metadata.as_ref(), spawned) {
            SupervisionVerdict::Held { .. } | SupervisionVerdict::SupervisorWaiting => continue,
            SupervisionVerdict::SupervisorReady => {}
            // §FS-rhei-plan-language.3: a non-leaf task is workable only once
            // its subtree is terminal — the same rule for `rhei next` and
            // `rhei run`, so a parent is never worked beside its own child.
            SupervisionVerdict::Unsupervised => {
                if !descendants_are_terminal(task, machines) {
                    continue;
                }
            }
        }
        let machine = machines.for_task(&task.id);
        let current_state = task.state.as_str();

        // Skip tasks already in a terminal or gating state.
        let normalized_state = normalized_state_name(current_state, machine);
        if is_terminal_state(current_state, machine)
            || machine.states.get(&normalized_state).map(|def| def.gating).unwrap_or(false)
        {
            continue;
        }

        if view == ReadySetView::RunnableNow
            && machine.states.get(&normalized_state).and_then(|def| def.poll.as_ref()).is_some()
            && poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &normalized_state)
                .is_some_and(|deadline| deadline > current_unix_secs())
        {
            continue;
        }

        // Check that all prior dependencies are satisfied — each judged under
        // the machine of the rhei that owns the prior. §FS-rhei-panta.6.1
        let all_priors_done = task.prior.iter().all(|dep_id| {
            state_map
                .get(dep_id)
                .map(|s| dependency_is_satisfied(s, machines.for_task(dep_id)))
                .unwrap_or(false)
        });

        // §AR-rhei-panta.5: input artifacts resolve from the owning rhei execution root.
        let artifact_root = roots.artifact_root(&task.id.to_string());
        if all_priors_done
            && state_inputs_exist_for_ready_set(
                roots.workspace_root,
                artifact_root,
                rhei,
                machine,
                task,
                &normalized_state,
            )
        {
            ready.push(task);
        }
    }

    ready
}

/// Ready tickets `rhei run` will not touch because someone already holds them.
/// The loop counts only what it can schedule, so a held ticket vanished from
/// the pass report and read as "not ready". §FS-rhei-run.3
// §AR-rhei-panta.5: `roots` keeps a member's inputs resolving at its own root.
fn find_held_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machines, roots, &HashSet::new())
        .into_iter()
        .filter(|task| task.assignee.is_some())
        .collect()
}

/// One line per supervisor whose barrier is stopping tickets this pass.
///
/// A dry run is the surface an author reads to understand what a machine will
/// do, and the barrier is invisible in it: four of five tickets simply never
/// appear as ready, with nothing saying why. Grouped by supervisor because the
/// supervisor, not the count, is the thing to look at.
// §FS-rhei-supervision.3.4
fn format_supervisor_holds(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Vec<String> {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let mut counts: Vec<(String, usize)> = Vec::new();
    for task in all
        .iter()
        .copied()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
    {
        let Some(hold) = held_by_supervisor(task, rhei, machines) else { continue };
        let key = hold.supervisor.to_string();
        match counts.iter_mut().find(|(id, _)| *id == key) {
            Some((_, count)) => *count += 1,
            None => counts.push((key, 1)),
        }
    }
    counts
        .into_iter()
        .map(|(supervisor, count)| {
            format!("{count} ticket(s) held by supervisor Task {supervisor}")
        })
        .collect()
}

/// One line naming every held ticket and who holds it.
fn format_held_tasks(held: &[&rhei_core::ast::Task]) -> String {
    held.iter()
        .map(|task| {
            format!("Task {} (assignee {})", task.id, task.assignee.as_deref().unwrap_or("?"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Find tasks that are ready to be claimed by `rhei next` in automatic mode.
///
/// A task is claimable when every descendant of it is terminal, it is in the
/// state machine's initial state, its prerequisites are satisfied, and it has
/// no `**Assignee:**` field (already claimed by another agent).
// §FS-rhei-next.3
fn find_claimable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    roots: &ReadySetRoots<'_>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machines, roots, &HashSet::new())
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .filter(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            task_is_in_initial_state(task, &state, machine)
        })
        .collect()
}

fn task_is_in_initial_state(
    task: &rhei_core::ast::Task,
    normalized_state: &str,
    machine: &rhei_validator::StateMachine,
) -> bool {
    machine
        .profile_for_node(task.kind.as_str(), task.profile_level())
        .map(|profile| profile.initial == normalized_state)
        .unwrap_or_else(|| machine.states.get(normalized_state).map(|def| def.initial).unwrap_or(false))
}

fn collect_plan_tasks<'a>(
    tasks: &'a [rhei_core::ast::Task],
    out: &mut Vec<&'a rhei_core::ast::Task>,
) {
    for task in tasks {
        out.push(task);
        collect_plan_tasks(&task.children, out);
    }
}

fn plan_state_map<'a>(
    tasks: &[&'a rhei_core::ast::Task],
    machines: &rhei_validator::MachineSet,
) -> std::collections::HashMap<&'a TaskId, String> {
    tasks
        .iter()
        .map(|task| {
            (&task.id, normalized_state_name(task.state.as_str(), machines.for_task(&task.id)))
        })
        .collect()
}

/// Every unsatisfied `**Prior:**` of `task` as `Task <id> (<state>)`. Judged
/// exactly as readiness judges it, so mutation commands and the scheduler
/// agree on what "blocked" means. §FS-rhei-panta.6.1
fn blocking_priors(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
) -> Vec<String> {
    task.prior
        .iter()
        .filter_map(|dep_id| match state_map.get(dep_id) {
            Some(state) if !dependency_is_satisfied(state, machines.for_task(dep_id)) => {
                Some(format!("Task {} ({})", dep_id, state))
            }
            None => Some(format!("Task {} (missing)", dep_id)),
            _ => None,
        })
        .collect()
}

fn first_blocking_prior(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Option<String> {
    task.prior.iter().find_map(|dep_id| match state_map.get(dep_id) {
        Some(state) if !dependency_is_satisfied(state, machines.for_task(dep_id)) => {
            // §FS-rhei-panta.6.1: `--rhei` narrows candidates, never prior
            // resolution, so name the prior that sits outside the scope
            // rather than leaving the operator to guess why nothing ran.
            let outside = if task_in_rhei_scope(scope, &dep_id.to_string()) {
                ""
            } else {
                ", outside the --rhei scope"
            };
            Some(format!("Task {} ({}{})", dep_id, state, outside))
        }
        None => Some(format!("Task {} (missing)", dep_id)),
        _ => None,
    })
}

/// Check whether a state is terminal (final) in the state machine.
fn is_terminal_state(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    let normalized = normalized_state_name(state, machine);
    machine.states.get(&normalized).map(|def| def.terminal).unwrap_or(false)
}
