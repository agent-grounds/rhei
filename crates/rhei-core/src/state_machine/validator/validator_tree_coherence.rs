// Coherence of the task tree itself: a claim that names nobody, siblings that
// share an id, and a terminal parent over a subtree that is not.
//
// Its own part because each of these reads the shape of the tree — the sibling
// set, the subtree under a node — rather than one node against its machine.

// §AR-source-file-size.3 §FS-rhei-plan-language.3

/// Warn when an authored `**Assignee:**` value is empty after trim.
///
/// The spec treats the field itself as optional; its *value* is only
/// required to be a non-empty title when present.
fn validate_assignee_nonempty(rhei: &Rhei, report: &mut ValidationReport) {
    for_each_node(rhei, |task| {
        if let Some(assignee) = &task.assignee {
            if assignee.trim().is_empty() {
                report.warnings.push(format!("Task {} has an empty **Assignee:** value", task.id));
            }
        }
    });
}

/// Verify that sibling ids are unique under the same parent and that every
/// child id extends its parent id by exactly one segment.
///
/// Together with the segment-extension check, this also implies global id
/// uniqueness across the whole plan, so no separate global pass is needed.
fn validate_sibling_uniqueness(rhei: &Rhei, report: &mut ValidationReport) {
    fn recurse(parent: Option<&Task>, siblings: &[Task], report: &mut ValidationReport) {
        let mut seen: HashSet<TaskId> = HashSet::new();
        for task in siblings {
            if let Some(p) = parent {
                if !task.id.extends(&p.id) {
                    report.errors.push(format!(
                        "Task {} must extend parent Task {} by exactly one segment",
                        task.id, p.id
                    ));
                }
            }
            if !seen.insert(task.id.clone()) {
                report.errors.push(format!(
                    "Duplicate sibling task id: Task {}{}",
                    task.id,
                    parent.map(|p| format!(" under Task {}", p.id)).unwrap_or_default()
                ));
            }
            recurse(Some(task), &task.children, report);
        }
    }
    recurse(None, &rhei.tasks, report);
}

/// Enforce that a terminal node has no non-terminal descendants anywhere in
/// its subtree.
fn validate_terminal_tree_coherence(
    rhei: &Rhei,
    machines: &MachineSet,
    report: &mut ValidationReport,
) {
    fn is_terminal(state_raw: &str, machine: &StateMachine) -> bool {
        let parsed = parse_task_state(state_raw, machine);
        machine.states.get(&parsed.state).map(|d| d.terminal).unwrap_or(false)
    }

    fn check_descendants(
        ancestor: &Task,
        node: &Task,
        machine: &StateMachine,
        report: &mut ValidationReport,
    ) {
        for child in &node.children {
            if !is_terminal(&child.state, machine) {
                report.errors.push(format!(
                    "Task {} is in terminal state '{}' but descendant Task {} ('{}') is in non-terminal state '{}'",
                    ancestor.id, ancestor.state, child.id, child.title, child.state
                ));
            }
            check_descendants(ancestor, child, machine, report);
        }
    }

    for_each_node(rhei, |task| {
        // A subtree lives inside one rhei, so the top task's machine governs
        // every descendant. §DA-per-rhei-state-machines
        let machine = machines.for_task(&task.id);
        if is_terminal(&task.state, machine) {
            check_descendants(task, task, machine, report);
        }
    });
}
