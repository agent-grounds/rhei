// Whether a supervising state has a way to finish.
//
// Its own part because this one question is asked by two surfaces that must
// agree — `rhei validate`'s warning and `rhei run`'s halt — and neither of the
// files that asks it owns the contract. It was written twice before, in the
// validator and in the CLI, which is exactly how the two could have drifted.

// §AR-source-file-size.3 §FS-rhei-supervision.1.2

/// Whether a supervising state has a way to finish.
///
/// True when some `openDescendants` transition out of `state`, other than its
/// own self-loop, has a target from which a final state the supervisor can
/// finish in is reachable. The walk counts the edges that take a task out of
/// the state they leave (§FS-rhei-transitions.4.6) and every `final: true`
/// state but the reserved cancellation terminal (§FS-rhei-states.1.4):
/// abandonment is somewhere for the engine to take a task, and it is not the
/// supervised work being declared done.
///
/// The walk starts at the edge's target rather than at `state` itself. A
/// machine that declares no `openDescendants` edge at all can still reach
/// `completed` from its supervising state by ordinary edges, so starting there
/// would silence the warning's principal true positive.
///
/// `pub` because §FS-rhei-supervision.1.2 requires the warning and the run-time
/// halt to say the same thing, and the halt is classified in the CLI. What that
/// commits to is the answer, not the walk: the walk stays private.
// §FS-rhei-supervision.1.2
pub fn supervising_state_can_finish(machine: &StateMachine, state: &str) -> bool {
    machine.transitions.iter().any(|rule| {
        rule.from.0 == state
            && rule.to.0 != state
            && rule.condition.as_deref().is_some_and(|cond| cond.contains("openDescendants"))
            && state_can_reach_final(machine, &rule.to.0, None, FinalReached::NotCancellation)
    })
}
