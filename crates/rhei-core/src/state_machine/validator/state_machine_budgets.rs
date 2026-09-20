// Authored numbers must be finite and positive even before autonomous
// admission is requested. §FS-rhei-budgets.2

impl StateMachine {
    fn validate_budget_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (name, state) in &self.states {
            if let Some(threshold) = &state.budget_threshold {
                threshold.validate().map_err(|error| {
                    StateMachineLoadError::Invalid(format!(
                        "state '{name}' budget_threshold: {error}"
                    ))
                })?;
            }
        }
        if let Some(profiles) = &self.profiles {
            for (name, profile) in profiles {
                if profile.transition_limit == Some(0) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{name}' transition_limit must be a positive integer"
                    )));
                }
            }
        }
        Ok(())
    }
}
