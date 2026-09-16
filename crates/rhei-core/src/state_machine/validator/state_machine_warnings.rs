// Machine-level warnings: what a state machine is allowed to declare but is
// unlikely to have meant. Every finding here is a warning rather than an error,
// so they sit apart from the load-time rules that reject a machine outright.

// §AR-source-file-size.3 §FS-rhei-validate.4

fn validate_state_machine_warnings(machine: &StateMachine, report: &mut ValidationReport) {
    for (state_name, state) in &machine.states {
        if state.gating && state.agent.is_some() {
            report.warnings.push(format!(
                "state '{state_name}' declares 'agent' on a gating state; gating states are human-only, so rhei run will not invoke this agent"
            ));
        }
        warn_on_supervising_state(machine, state_name, state, report);
        warn_on_unbounded_self_loop(machine, state_name, state, report);
    }
}

/// Warn about a self-loop nothing terminates.
///
/// A non-poll self-loop is a loop-back re-entry, and its visits are counted so
/// a `visitCount` exit can end it. A state that declares neither a `visits`
/// budget nor such an exit has no way out of its own loop: the engine keeps
/// selecting the self-loop and the task never leaves the state. A poll state is
/// exempt — `max_attempts` bounds its attempts.
// §FS-rhei-supervision.4.2 §FS-rhei-states.1.3
fn warn_on_unbounded_self_loop(
    machine: &StateMachine,
    state_name: &str,
    state: &StateDef,
    report: &mut ValidationReport,
) {
    if state.poll.is_some() || state.visits.is_some() {
        return;
    }
    let outgoing = || machine.transitions.iter().filter(|rule| rule.from.0 == *state_name);
    if !outgoing().any(|rule| rule.to.0 == *state_name) {
        return;
    }
    let has_counted_exit = outgoing().any(|rule| {
        rule.to.0 != *state_name
            && rule.condition.as_deref().is_some_and(|cond| cond.contains("visitCount"))
    });
    if !has_counted_exit {
        report.warnings.push(format!(
            "state '{state_name}' has a self-loop but declares neither 'visits' nor a transition \
             bounded by `visitCount`; nothing ends the loop, so a run may re-enter it forever"
        ));
    }
}

/// Warn about a supervising state that can never finish or never give up.
///
/// Both are accepted machines — the engine runs them — so they are warnings,
/// not errors. Each names a supervisor the run would keep waking forever: one
/// with no terminal edge it can select, one with no budget and no escalation.
// §FS-rhei-supervision.1.2
fn warn_on_supervising_state(
    machine: &StateMachine,
    state_name: &str,
    state: &StateDef,
    report: &mut ValidationReport,
) {
    if state.execute_on().is_none() {
        return;
    }
    let outgoing = || machine.transitions.iter().filter(|rule| rule.from.0 == *state_name);

    // §FS-rhei-supervision.4.1: `openDescendants` is how a machine selects the
    // edge that finishes a parent once its subtree is closed — and finishing is
    // a path from that edge's target, not the edge alone.
    if !supervising_state_can_finish(machine, state_name) {
        report.warnings.push(format!(
            "state '{state_name}' declares 'execute_on' but no `openDescendants` transition from \
             it reaches a final state by any path, cancellation aside; the supervisor has no way \
             to finish. Point one at a final state, or at a state that reaches one."
        ));
    }

    // §FS-rhei-supervision.1.2: a supervisor that neither budgets its visits
    // nor declares an exhaustion edge waits on a subtree that may never converge.
    let has_exhaustion_edge = outgoing().any(|rule| {
        rule.to.0 != *state_name
            && rule.condition.as_deref().is_some_and(|cond| cond.contains("visitCount"))
    });
    if state.visits.is_none() && !has_exhaustion_edge {
        report.warnings.push(format!(
            "state '{state_name}' declares 'execute_on' but neither 'visits' nor an exhaustion \
             transition on `visitCount`; a subtree that never converges has no safety valve"
        ));
    }
}
