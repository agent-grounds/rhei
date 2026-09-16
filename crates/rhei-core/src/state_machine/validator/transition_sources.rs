// General flat-machine escape ownership. §FS-rhei-transitions.4.6
impl StateMachine {
    /// Match a source without changing exact/wildcard precedence at the caller.
    pub fn transition_matches_source(&self, rule: &TransitionRule, state: &str) -> bool {
        let state = parse_task_state(state, self).state;
        if rule.from.0 != "*" {
            return rule.from.0 == state;
        }
        self.states.get(&state).is_some_and(|def| !def.terminal)
            && rule.sources.as_ref().is_none_or(|sources| sources.contains(&state))
    }

    /// Applicable source rules in exact-before-wildcard declaration order.
    /// §FS-rhei-transitions.4.6
    pub fn transitions_from<'a>(&'a self, state: &'a str) -> impl Iterator<Item = &'a TransitionRule> {
        self.transitions.iter().filter(move |rule| rule.from.0 != "*" && self.transition_matches_source(rule, state))
            .chain(self.transitions.iter().filter(move |rule| rule.from.0 == "*" && self.transition_matches_source(rule, state)))
    }

    /// Shared by final validation and the compiler before qualification.
    /// §FS-rhei-states.1.4 §FS-rhei-transitions.4.6
    pub fn validate_cancellation_and_sources(&self) -> Result<(), StateMachineLoadError> {
        for (name, state) in &self.states {
            if let Some(role) = &state.role {
                if role != "cancellation" || !state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{name}' role '{role}' is invalid: use role: cancellation with final: true, or omit role"
                    )));
                }
            }
        }
        for rule in &self.transitions {
            if let Some(sources) = &rule.sources {
                if rule.from.0 != "*" {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition '{} -> {}' declares sources on an exact edge; use from: \"*\" or remove sources",
                        rule.from.0, rule.to.0
                    )));
                }
                let mut seen = HashSet::new();
                for source in sources {
                    if !self.states.contains_key(source) || !seen.insert(source) {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "transition '* -> {}' has unknown or duplicate source '{source}'; sources must contain unique declared state names",
                            rule.to.0
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}
