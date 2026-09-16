impl StateMachine {
    fn validate_program_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            if let Some(program) = &state.program {
                validate_program_value(state_name, program)?;
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare a 'program' (terminal states have no work to execute)"
                    )));
                }
                if state.gating {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is gating and cannot declare a 'program' (gating states require human action)"
                    )));
                }
            }
        }

        for transition in &self.transitions {
            if transition.exit_code.is_none() {
                continue;
            }

            let Some(from_state) = self.states.get(&transition.from.0) else {
                continue;
            };
            if from_state.program.is_none() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "transition from '{}' to '{}' declares 'exit_code' but source state '{}' does not declare a program",
                    transition.from.0, transition.to.0, transition.from.0
                )));
            }
        }

        Ok(())
    }

    /// Validate the per-state `poll:` block: well-formed `interval` and
    /// `max_attempts`, a non-blank `waiting_on` when one is declared
    /// (§FS-rhei-states.2.5), mutually exclusive with `visits`, forbidden on
    /// final/gating states, and at least one self-loop transition is
    /// declared so the retry branch is reachable.
    fn validate_poll_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            let Some(poll) = state.poll.as_ref() else { continue };
            if parse_duration_secs(&poll.interval).is_none() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has poll.interval '{}' that is not a valid duration (expected e.g. '30s', '5m', '1h')",
                    poll.interval
                )));
            }
            if poll.max_attempts < 1 {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has poll.max_attempts {} (must be >= 1)",
                    poll.max_attempts
                )));
            }
            // A blank label declares the person wait and names nobody, the one
            // shape no surface can print. §FS-rhei-states.1.3 §FS-rhei-states.2.5
            if poll.waiting_on.as_ref().is_some_and(|label| label.trim().is_empty()) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has an empty poll.waiting_on; name the person or role the poll waits on (e.g. 'author'), or omit the field"
                )));
            }
            if state.terminal {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' is final and cannot declare 'poll' (terminal states have no work to execute)"
                )));
            }
            if state.gating {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' is gating and cannot declare 'poll' (gating states require human action; polling executes autonomously)"
                )));
            }
            if state.visits.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares both 'poll' and 'visits'; poll.max_attempts replaces the visits cap"
                )));
            }
            if state.snapshot.as_ref().and_then(|snapshot| snapshot.inherit.as_ref()).is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares both 'poll' and 'snapshot.inherit'; polling states cannot inherit snapshots in v1"
                )));
            }
            let has_self_loop =
                self.transitions.iter().any(|t| t.from.0 == *state_name && t.to.0 == *state_name);
            if !has_self_loop {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'poll' but has no self-loop transition; add a transition with from: {state_name} and to: {state_name} so the retry branch is reachable"
                )));
            }
        }
        Ok(())
    }

    /// Trim every declared `poll.waiting_on` once, so the stored label is the
    /// one every surface prints and no reader has to normalize again — the
    /// serialized machine and the text surfaces cannot then disagree about the
    /// same person. Runs after [`Self::validate_poll_configuration`], which has
    /// already refused a label that is blank once trimmed.
    // §FS-rhei-states.2.5
    fn normalize_poll_waiting_on(&mut self) {
        for state in self.states.values_mut() {
            let Some(poll) = state.poll.as_mut() else { continue };
            let Some(label) = poll.waiting_on.as_deref() else { continue };
            let trimmed = label.trim();
            poll.waiting_on = (!trimmed.is_empty()).then(|| trimmed.to_string());
        }
    }

    /// Validate the per-state `execute_on:` field: a legal value, an
    /// agent-bearing state that is neither final, gating, program-driven nor
    /// polling, no fanout, and a self-loop transition to release the subtree
    /// with.
    ///
    /// Every rule here is an error rather than a warning because each one
    /// names a supervisor the engine could never schedule: without an executor
    /// nothing runs the visit, and without the self-loop the subtree is never
    /// released.
    // §FS-rhei-supervision.1.2
    fn validate_execute_on_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            let Some(raw) = state.execute_on.as_deref() else { continue };
            if ExecuteOn::parse(raw).is_none() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has execute_on '{raw}' (expected one of {})",
                    ExecuteOn::VALUES.join(", ")
                )));
            }
            if state.terminal {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' is final and cannot declare 'execute_on' (terminal states have no work to execute)"
                )));
            }
            if state.gating {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' is gating and cannot declare 'execute_on' (gating states require human action; a supervisor executes autonomously)"
                )));
            }
            if state.program.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares both 'program' and 'execute_on'; a supervisor is a continued agent session, not a program"
                )));
            }
            if state.poll.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares both 'poll' and 'execute_on'; a state has one trigger: `poll:` (time) or `execute_on:` (its subtree)"
                )));
            }
            if !state.all_targets.is_empty() || !state.all_models.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' combines 'execute_on' with 'all_targets' or 'all_models'; a supervisor is one continued session, not a fanout"
                )));
            }
            if !state.is_agent_bearing() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'execute_on' but is not agent-bearing; give it an 'agent', 'target', or 'model'"
                )));
            }
            let has_self_loop =
                self.transitions.iter().any(|t| t.from.0 == *state_name && t.to.0 == *state_name);
            if !has_self_loop {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'execute_on' but has no self-loop transition; add a transition with from: {state_name} and to: {state_name} so the release branch is reachable"
                )));
            }
        }
        Ok(())
    }

    /// Validate the per-state `mcp_servers` and `skills` lists and the
    /// matching `mcp_unavailable` / `skill_unavailable` transition triggers.
    ///
    /// This pass is purely structural — it rejects malformed entries,
    /// duplicate ids, and the gating/program/final exclusions. Cross-file
    /// reference resolution against settings registries happens elsewhere
    /// (the CLI merges settings and checks id resolution at load time).
    fn validate_tooling_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            validate_state_mcp_entries(state_name, state)?;
            validate_state_skill_entries(state_name, state)?;
        }

        for transition in &self.transitions {
            validate_transition_tooling_trigger(
                transition,
                transition.mcp_unavailable.as_ref(),
                "mcp_unavailable",
            )?;
            validate_transition_tooling_trigger(
                transition,
                transition.skill_unavailable.as_ref(),
                "skill_unavailable",
            )?;

            if transition.mcp_unavailable.is_some() || transition.skill_unavailable.is_some() {
                if let Some(from_state) = self.states.get(&transition.from.0) {
                    if from_state.program.is_some() {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "transition from '{}' to '{}' declares a tooling-unavailable trigger \
                             but source state '{}' is a program state (tooling triggers are agent-only)",
                            transition.from.0, transition.to.0, transition.from.0
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}
