// What a `**Prior:**` in a terminal state says about the ticket behind it: a
// prior order the plan contradicts, and a prior that can never satisfy it.

/// Warn about tickets that went terminal while a `**Prior:**` is unsatisfied.
/// A terminal ticket leaves readiness and `--blocked`, so nothing else reveals
/// it; legitimate authoring reaches it, so warn. §FS-rhei-validate.4
fn validate_prior_order_coherence(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    machines: &MachineSet,
    report: &mut ValidationReport,
) {
    // A prior is judged under the machine of the rhei that owns it: the
    // target's states mean what its own process says. §FS-rhei-panta.6.1
    let satisfied = |id: &TaskId| -> bool {
        index
            .get(id)
            .map(|dep| {
                let machine = machines.for_task(id);
                let state = parse_task_state(dep.state.as_str(), machine).state;
                // §FS-rhei-states.1.4: the reserved cancel name, either spelling.
                !machine.is_cancellation(&state)
                    && machine.states.get(&state).map(|def| def.terminal).unwrap_or(false)
            })
            .unwrap_or(false)
    };

    for_each_node(rhei, |task| {
        let machine = machines.for_task(&task.id);
        let state = parse_task_state(task.state.as_str(), machine).state;
        // Only a *successful* terminal state is a contradiction: a cancelled
        // ticket never claimed its prerequisites ran.
        if machine.is_cancellation(&state)
            || !machine.states.get(&state).map(|def| def.terminal).unwrap_or(false)
        {
            return;
        }
        // A missing prior is already a hard error in dependency integrity;
        // do not double-report it here.
        let unmet: Vec<String> = task
            .prior
            .iter()
            .filter(|dep| index.contains_key(*dep) && !satisfied(dep))
            .map(|dep| {
                format!(
                    "Task {} ({})",
                    dep,
                    parse_task_state(index[dep].state.as_str(), machines.for_task(dep)).state
                )
            })
            .collect();
        if !unmet.is_empty() {
            report.warnings.push(format!(
                "{} {} is '{}' but its prerequisites are unsatisfied: {}. The plan contradicts its own **Prior:** dependencies.",
                title_case_kind(&task.kind),
                task.id,
                state,
                unmet.join(", ")
            ));
        }
    });
}

/// Warn about a ticket that is not yet terminal while a `**Prior:**` sits in a
/// cancellation state: that prior is final and never satisfies it, so the
/// ticket can never become ready. A warning, never an error. A cancelled
/// ticket waits on nothing, and a successfully terminal one is
/// [`validate_prior_order_coherence`]'s. §FS-rhei-validate.4 §FS-rhei-states.1.4
fn validate_cancelled_priors(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    machines: &MachineSet,
    report: &mut ValidationReport,
) {
    for_each_node(rhei, |task| {
        let machine = machines.for_task(&task.id);
        let state = parse_task_state(task.state.as_str(), machine).state;
        if machine.states.get(&state).map(|def| def.terminal).unwrap_or(false) {
            return;
        }
        // Judged under the prior's own machine, so a custom
        // `role: cancellation` state counts. §FS-rhei-panta.6.1
        let cancelled: Vec<String> = task
            .prior
            .iter()
            .filter_map(|dep| {
                let prior_machine = machines.for_task(dep);
                let prior_state = parse_task_state(index.get(dep)?.state.as_str(), prior_machine).state;
                prior_machine
                    .is_cancellation(&prior_state)
                    .then(|| format!("Task {} ({})", dep, prior_state))
            })
            .collect();
        if cancelled.is_empty() {
            return;
        }
        let (what, them) = if cancelled.len() == 1 {
            ("a cancelled prior", "it")
        } else {
            ("cancelled priors", "them")
        };
        let kind = title_case_kind(&task.kind);
        report.warnings.push(format!(
            "{kind} {id} waits on {priors}, {what} that can never satisfy it: re-point its \
             **Prior:** (and any **Consumes:** naming {them}) at a completed step, or cancel \
             {kind} {id}.",
            id = task.id,
            priors = cancelled.join(", "),
        ));
    });
}
