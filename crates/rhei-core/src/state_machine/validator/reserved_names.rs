// Shared cancellation classification. §FS-rhei-states.1.4

/// Backward-compatible inference for bare reserved state names.
/// Machine-aware consumers use `StateMachine::is_cancellation` instead.
pub fn is_cancelled_state_name(state: &str) -> bool {
    matches!(state, "cancelled" | "canceled")
}

impl StateMachine {
    /// Classify an exact or counted state without interpreting generated names.
    /// §FS-rhei-states.1.4
    pub fn is_cancellation(&self, state: &str) -> bool {
        let state = parse_task_state(state, self).state;
        is_cancelled_state_name(&state)
            || self.states.get(&state).is_some_and(|def| def.role.as_deref() == Some("cancellation"))
    }
}
