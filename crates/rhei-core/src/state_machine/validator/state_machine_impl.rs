impl StateMachine {
    /// Return the built-in default state machine shipped with rhei.
    pub fn builtin_default() -> Self {
        Self::from_yaml_str(DEFAULT_STATES_YAML).expect("built-in states YAML is always valid")
    }

    /// Load a StateMachine from YAML string contents.
    ///
    /// Prompt templates are read from a directory beside `states.yaml`, so a
    /// machine parsed from a string always has an empty template map; use
    /// [`Self::from_yaml_file`] when states select a `prompt_template`.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, StateMachineLoadError> {
        Self::parse_fragment(yaml)?.validate()
    }

    /// Parse and run the pre-deserialization rejections shared by every entry
    /// point, without validating. Both loaders funnel through here so a new
    /// check cannot be added to one and silently skipped by the other.
    pub fn parse_fragment(yaml: &str) -> Result<Self, StateMachineLoadError> {
        // One raw parse feeds every check that has to see the YAML as written
        // rather than as serde collapsed it.
        let raw: serde_yaml::Value = serde_yaml::from_str(yaml)?;
        reject_explicit_empty_all_targets(&raw)?;
        reject_inline_prompt_templates(&raw)?;
        Ok(serde_yaml::from_str(yaml)?)
    }

    fn validate(mut self) -> Result<Self, StateMachineLoadError> {
        self.validate_model_configuration()?;
        self.validate_prompt_templates()?;
        self.validate_program_configuration()?;
        self.validate_snapshot_configuration()?;
        self.validate_tooling_configuration()?;
        self.validate_template_conditions()?;
        self.validate_poll_configuration()?;
        self.validate_metrics_configuration()?;
        // Every surface reads the stored label, so normalize it once here
        // rather than on each read. §FS-rhei-states.2.5
        self.normalize_poll_waiting_on();
        self.validate_execute_on_configuration()?;
        self.validate_profiles_and_node_policy()?;
        self.validate_terminal_state_present()?;
        // Last, so a machine that is invalid for a narrower reason is still
        // reported by that reason rather than by its dead ends. §FS-rhei-states.1.3
        self.validate_every_state_can_be_left()?;
        Ok(self)
    }

    /// Reject state machines that declare zero terminal states. Without one,
    /// `rhei complete`, terminal-state filters, and prerequisite resolution
    /// cannot work correctly, and a forgotten or mistyped `final: true` is
    /// otherwise silently accepted.
    fn validate_terminal_state_present(&self) -> Result<(), StateMachineLoadError> {
        if self.states.values().any(|state| state.terminal) {
            return Ok(());
        }
        Err(StateMachineLoadError::Invalid(format!(
            "state machine '{}' declares no terminal states. Mark at least one \
             state with `final: true` (note: the field is `final`, not `terminal`).",
            self.name
        )))
    }

    // §FS-rhei-states.9.2: Resolve non-root node profiles by policy order.

    /// Resolve the profile for a non-root node, following node-policy order:
    /// `overrides`, `by_type[<kind>]`, then `default`.
    /// Returns `None` when `profiles` / `node_policy` is absent.
    pub fn profile_for_node(&self, kind: &str, level: u8) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        let resolved_name = policy
            .overrides
            .iter()
            .find(|ov| ov.match_.matches(kind, level))
            .map(|ov| ov.profile.as_str())
            .or_else(|| {
                policy
                    .by_type
                    .iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(kind))
                    .map(|(_, profile)| profile.as_str())
            })
            .unwrap_or(policy.default.as_str());
        profiles.get(resolved_name)
    }

    /// §FS-rhei-states.9.2: Resolve the profile bound to the plan-root node.
    pub fn root_profile(&self) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        profiles.get(policy.root.as_str())
    }

    /// Load a StateMachine from a file path, including the reusable prompt
    /// templates in its sibling `prompt_templates/` directory.
    // §FS-rhei-states.4.4: prompt files load with the state machine.
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self, StateMachineLoadError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let mut sm = Self::parse_fragment(&text)?;
        reject_legacy_prompt_templates_file(path)?;
        sm.prompt_templates = load_prompt_templates_dir(&prompt_templates_dir(path))?;
        sm.validate()
    }

    /// Returns true if `state` is among the allowed states.
    pub fn is_valid_state<S: AsRef<str>>(&self, state: S) -> bool {
        self.states.contains_key(state.as_ref())
    }

    /// Return the set of allowed state names.
    pub fn allowed_states(&self) -> impl Iterator<Item = &str> {
        self.states.keys().map(|s| s.as_str())
    }

    /// Return the declared transitions between states.
    pub fn transitions(&self) -> &[TransitionRule] {
        &self.transitions
    }

    /// Stable content fingerprint. Two machines share one only when they are
    /// the same definition, field for field.
    ///
    /// Name and version do **not** identify a machine. Instantiating one
    /// template twice bakes each instantiation's inputs into its own
    /// `states.yaml` while both keep the template's `name` and `version`, so
    /// grouping by name silently merged distinct machines: every surface that
    /// walks the distinct set then reported one rhei's states — and one rhei's
    /// baked-in prompts — as though they governed all of them.
    // §FS-rhei-states-cmd.3: machine identity is content, not name.
    pub fn fingerprint(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| self.name.clone())
    }

    fn validate_model_configuration(&self) -> Result<(), StateMachineLoadError> {
        let mut seen = HashSet::new();
        for model in &self.models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "top-level 'models' entries must be non-empty strings".to_string(),
                ));
            }
            if !seen.insert(trimmed) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "top-level 'models' contains duplicate entry '{trimmed}'"
                )));
            }
        }

        for (state_name, state) in &self.states {
            if state.target.is_some() && !state.all_targets.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot set both 'target' and 'all_targets'"
                )));
            }
            if (state.target.is_some() || !state.all_targets.is_empty())
                && (state.model.is_some()
                    || !state.all_models.is_empty()
                    || state.agent.is_some()
                    || state.agent_mode.is_some())
            {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot combine 'target' or 'all_targets' with \
                     'model', 'all_models', 'agent', or 'agent_mode'"
                )));
            }
            if let Some(selector) = state.target.as_deref() {
                parse_execution_target(selector).map_err(|message| {
                    StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'target': {message}"
                    ))
                })?;
            }
            if !state.all_targets.is_empty() {
                let mut seen_targets = HashSet::new();
                let mut seen_target_slugs: HashMap<String, String> = HashMap::new();
                for selector in &state.all_targets {
                    let parsed = parse_execution_target(selector).map_err(|message| {
                        StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' has invalid 'all_targets' entry: {message}"
                        ))
                    })?;
                    let normalized = parsed.selector();
                    if !seen_targets.insert(normalized.clone()) {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' contains duplicate 'all_targets' entry '{normalized}'"
                        )));
                    }
                    let slug = parsed.slug();
                    if let Some(previous) = seen_target_slugs.insert(slug.clone(), selector.clone())
                    {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' has all_targets entries '{previous}' and '{selector}' that normalize to the same snapshot target slug '{slug}'"
                        )));
                    }
                }
            }
            if !state.all_models.is_empty() && state.model.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot set both 'all_models' and 'model'"
                )));
            }

            if state.visits == Some(0) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'visits: 0' but visits must be at least 1"
                )));
            }

            validate_artifact_definitions(state_name, "inputs", &state.inputs)?;
            validate_artifact_definitions(state_name, "outputs", &state.outputs)?;
            validate_handoff_definitions(state_name, state)?;

            // Agent validation.
            if let Some(agent) = &state.agent {
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare an 'agent' (terminal states have no work to execute)"
                    )));
                }
                if agent.id().trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'agent' value"
                    )));
                }
            }
            if let Some(mode) = &state.agent_mode {
                if state.agent.is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares 'agent_mode' without declaring an 'agent'"
                    )));
                }
                if mode.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'agent_mode' value"
                    )));
                }
            }
            // A state selected through settings need not declare an agent here,
            // but deterministic and human-only states can never consume effort.
            // §FS-rhei-states.1.3
            if state.effort.is_some()
                && (state.program.is_some() || state.gating || state.terminal)
            {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'effort' but is not eligible for autonomous agent work"
                )));
            }
            if let Some(timeout) = &state.agent_timeout {
                if parse_duration_secs(timeout).is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'agent_timeout' value '{timeout}' \
                         (expected format like '30s', '5m', '1h', '2h30m')"
                    )));
                }
            }
            if let Some(timeout) = &state.program_timeout {
                if parse_duration_secs(timeout).is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'program_timeout' value '{timeout}' \
                         (expected format like '30s', '5m', '1h', '2h30m')"
                    )));
                }
            }
            if state.agent.is_some() && state.program.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot declare both 'agent' and 'program' (they are mutually exclusive)"
                )));
            }

            if !state.all_models.is_empty() && self.models.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' sets 'all_models' but the machine does not declare any top-level 'models'"
                )));
            }

            let mut state_seen = HashSet::new();
            for model in &state.all_models {
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' contains an empty 'all_models' entry"
                    )));
                }
                if !state_seen.insert(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' contains duplicate 'all_models' entry '{trimmed}'"
                    )));
                }
                if !seen.contains(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' references unknown model '{trimmed}' in 'all_models'"
                    )));
                }
            }

            if let Some(model) = state.model.as_deref() {
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'model' value"
                    )));
                }
                if self.models.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' sets 'model: {trimmed}' but the machine does not declare any top-level 'models'"
                    )));
                }
                if !seen.contains(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' references unknown model '{trimmed}'"
                    )));
                }
            }
        }

        Ok(())
    }


}

fn reject_explicit_empty_all_targets(
    raw: &serde_yaml::Value,
) -> Result<(), StateMachineLoadError> {
    // `all_targets` carries `#[serde(default)]`, so serde collapses missing
    // and explicit-empty into the same empty Vec. Inspect the raw YAML to
    // distinguish them and reject `all_targets: []` as authoring sugar that
    // most likely means "I intended to list targets here and forgot."
    let Some(states) = raw.get("states").and_then(serde_yaml::Value::as_mapping) else {
        return Ok(());
    };

    for (state_name, state_value) in states {
        let Some(state) = state_value.as_mapping() else { continue };
        let Some(all_targets) = state.get("all_targets") else { continue };
        if all_targets.as_sequence().is_some_and(Vec::is_empty) {
            let label = state_name.as_str().unwrap_or("<unknown>");
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{label}' declares 'all_targets: []' but all_targets must be a non-empty list when present"
            )));
        }
    }

    Ok(())
}
