// The task tally of a summary's lead line: how many tasks reached each
// terminal state, and how many are still in progress, counted from the plan,
// from a completed run's records, or from the states a checkpoint recorded.

// §FS-rhei-summary.2.1

/// A completed-run tally comes from the last recorded state of each represented
/// task, never from the plan's current state.
/// §FS-rhei-summary.2.1
fn selected_run_task_tally(
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
) -> String {
    let mut tasks: BTreeMap<&str, &str> = BTreeMap::new();
    for held in &inspection.invocations {
        tasks.insert(held.record.task_id.as_str(), held.record.state.as_str());
    }
    tally_states(machine, tasks.values().copied())
}

/// Tasks per terminal state in machine declaration order, with the
/// in-progress remainder appended so a mid-run summary says it is one.
///
/// The tally counts the tasks of the invocation's own scope: one sentence must
/// not describe two, and a member's invocations beside the whole project's task
/// counts reads as a summary of neither. §FS-rhei-summary.2.1
fn summary_task_tally(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    scope: &RheiScope,
) -> String {
    let tasks: Vec<&rhei_core::ast::Task> = narrow_to_rhei_scope(flatten_tasks(rhei), scope);
    tally_states(machine, tasks.iter().map(|task| task.state.as_str()))
}

/// Count recorded or current task states in machine declaration order.
/// §FS-rhei-summary.2.1
fn tally_states<'a>(
    machine: &rhei_validator::StateMachine,
    states: impl IntoIterator<Item = &'a str>,
) -> String {
    let states: Vec<&str> = states.into_iter().collect();
    let mut parts: Vec<(usize, &str)> = Vec::new();
    for (state, def) in &machine.states {
        if !def.terminal {
            continue;
        }
        let count = states.iter().filter(|task_state| **task_state == state.as_str()).count();
        if count > 0 {
            parts.push((count, state.as_str()));
        }
    }
    let terminal: HashSet<&str> = machine
        .states
        .iter()
        .filter(|(_, def)| def.terminal)
        .map(|(state, _)| state.as_str())
        .collect();
    let in_progress = states.iter().filter(|state| !terminal.contains(**state)).count();
    if in_progress > 0 {
        parts.push((in_progress, "in progress"));
    }
    if parts.is_empty() {
        return "no tasks".to_string();
    }
    let mut tally = String::new();
    for (index, (count, label)) in parts.iter().enumerate() {
        if index == 0 {
            tally.push_str(&format!("{count} task{} {label}", plural_s(*count)));
        } else {
            tally.push_str(&format!(", {count} {label}"));
        }
    }
    tally
}
