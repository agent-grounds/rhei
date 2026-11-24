// A gating state's `gate:` block, checked when its machine loads. Every
// problem across every state is reported together: an author fixing a brief
// fixes it from one list. The block's own rules live with the gate module.

// §FS-rhei-gate-brief.1.2 §FS-rhei-states.1.3

impl StateMachine {
    /// Refuse a `gate:` block on a non-gating state, and every problem a block
    /// on a gating state has against the edges that leave it.
    fn validate_gate_configuration(&self) -> Result<(), StateMachineLoadError> {
        let mut problems = Vec::new();
        for (name, state) in &self.states {
            let Some(gate) = &state.gate else {
                continue;
            };
            if !state.gating {
                problems.push(format!(
                    "state '{name}' declares `gate:` but is not `gating: true`; a brief is what a \
                     gate puts to a person (§FS-rhei-gate-brief.1.2)"
                ));
                continue;
            }
            let targets: Vec<&str> = self.transitions_from(name).map(|rule| rule.to.0.as_str()).collect();
            for problem in crate::gate::gate_def_problems(gate, &targets) {
                problems.push(format!("state '{name}' gate: {problem}"));
            }
        }
        if problems.is_empty() {
            return Ok(());
        }
        Err(StateMachineLoadError::Invalid(format!(
            "invalid `gate:` block (§FS-rhei-gate-brief.1.2):\n  - {}",
            problems.join("\n  - ")
        )))
    }
}
