impl StateMachine {
    /// Validate the `profiles` and `node_policy` blocks introduced by the
    /// current schema revision.
    ///
    /// When both are absent, the machine is treated as legacy and no
    /// additional checks run. When either is present, both must be present
    /// and consistent.
    fn validate_profiles_and_node_policy(&self) -> Result<(), StateMachineLoadError> {
        let requires_profile_schema = self.schema_major_version().is_some_and(|major| major >= 3);
        match (self.profiles.as_ref(), self.node_policy.as_ref()) {
            (None, None) if !requires_profile_schema => return Ok(()),
            (None, None) => {
                return Err(StateMachineLoadError::Invalid(
                    "state machine schema version 3 requires 'profiles' and 'node_policy' blocks"
                        .to_string(),
                ));
            }
            (Some(_), None) => {
                return Err(StateMachineLoadError::Invalid(
                    "state machine declares 'profiles' but no 'node_policy' block".to_string(),
                ));
            }
            (None, Some(_)) => {
                return Err(StateMachineLoadError::Invalid(
                    "state machine declares 'node_policy' but no 'profiles' block".to_string(),
                ));
            }
            (Some(_), Some(_)) => {}
        }

        let profiles = self.profiles.as_ref().expect("profiles present");
        let policy = self.node_policy.as_ref().expect("node_policy present");

        if profiles.is_empty() {
            return Err(StateMachineLoadError::Invalid(
                "'profiles' must declare at least one profile".to_string(),
            ));
        }

        for (profile_name, profile) in profiles {
            if profile_name.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "'profiles' contains an entry with an empty name".to_string(),
                ));
            }

            if profile.initial.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' declares an empty 'initial'"
                )));
            }

            if !self.states.contains_key(&profile.initial) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' has 'initial: {}' but no such state is defined",
                    profile.initial
                )));
            }

            if profile.allowed.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' declares an empty 'allowed' list"
                )));
            }

            let mut seen = HashSet::new();
            for allowed in &profile.allowed {
                let trimmed = allowed.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{profile_name}' contains an empty entry in 'allowed'"
                    )));
                }
                if !seen.insert(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{profile_name}' contains duplicate 'allowed' entry '{trimmed}'"
                    )));
                }
                if !self.states.contains_key(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{profile_name}' lists unknown state '{trimmed}' in 'allowed'"
                    )));
                }
            }

            if !profile.allowed.iter().any(|s| s == &profile.initial) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' 'initial: {}' is not in its own 'allowed' list",
                    profile.initial
                )));
            }

            if !profile
                .allowed
                .iter()
                .any(|state| self.states.get(state).is_some_and(|def| def.terminal))
            {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' must allow at least one final state"
                )));
            }

            // The machine-wide question of §FS-rhei-states.1.3 asked again of
            // this profile's narrowed set, counting edges the same way.
            // §FS-rhei-states.8.2
            let allowed_states: HashSet<&str> =
                profile.allowed.iter().map(String::as_str).collect();
            for allowed in &profile.allowed {
                if self.states.get(allowed).is_some_and(|def| def.terminal) {
                    continue;
                }
                if !state_can_reach_final(self, allowed, Some(&allowed_states), FinalReached::Any) {
                    return Err(StateMachineLoadError::Invalid(profile_dead_end_message(
                        self,
                        profile_name,
                        allowed,
                        &allowed_states,
                    )));
                }
            }
        }

        if !profiles.contains_key(&policy.root) {
            return Err(StateMachineLoadError::Invalid(format!(
                "'node_policy.root' references undefined profile '{}'",
                policy.root
            )));
        }
        if !profiles.contains_key(&policy.default) {
            return Err(StateMachineLoadError::Invalid(format!(
                "'node_policy.default' references undefined profile '{}'",
                policy.default
            )));
        }

        let mut seen_kinds = HashSet::new();
        for (kind, profile_name) in &policy.by_type {
            let trimmed_kind = kind.trim();
            if trimmed_kind.is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "'node_policy.by_type' contains an empty node-kind key".to_string(),
                ));
            }
            if trimmed_kind.eq_ignore_ascii_case("panta") || trimmed_kind.eq_ignore_ascii_case("rhei") {
                return Err(StateMachineLoadError::Invalid(
                    format!(
                        "'node_policy.by_type' must not declare the reserved kind '{trimmed_kind}'"
                    ),
                ));
            }
            if !seen_kinds.insert(trimmed_kind.to_ascii_lowercase()) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.by_type' contains duplicate kind '{trimmed_kind}' \
                     (kind matching is case-insensitive)"
                )));
            }
            if !profiles.contains_key(profile_name) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.by_type.{trimmed_kind}' references undefined profile \
                     '{profile_name}'"
                )));
            }
        }

        for (idx, ov) in policy.overrides.iter().enumerate() {
            if let Some(node_type) = ov.match_.node_type.as_deref() {
                if node_type.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "'node_policy.overrides[{idx}].match.type' must be a non-empty node kind"
                    )));
                }
                if node_type.eq_ignore_ascii_case("panta") || node_type.eq_ignore_ascii_case("rhei") {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "'node_policy.overrides[{idx}].match.type' must not be '{}'; it is a reserved kind",
                        node_type
                    )));
                }
            }
            if ov.match_.level == Some(0) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.overrides[{idx}].match.level' must be at least 1; the root node is bound via 'node_policy.root'"
                )));
            }
            if !profiles.contains_key(&ov.profile) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.overrides[{idx}]' references undefined profile '{}'",
                    ov.profile
                )));
            }
        }

        for (state_name, state) in &self.states {
            if state.initial {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'initial: true', but the machine uses \
                     'profiles' — the initial state is a property of each profile"
                )));
            }
        }

        Ok(())
    }

    fn schema_major_version(&self) -> Option<u64> {
        match &self.version {
            serde_yaml::Value::Number(number) => number
                .as_u64()
                .or_else(|| number.as_i64().and_then(|value| u64::try_from(value).ok()))
                .or_else(|| {
                    let value = number.as_f64()?;
                    if value.fract() == 0.0 && value >= 0.0 {
                        Some(value as u64)
                    } else {
                        None
                    }
                }),
            serde_yaml::Value::String(value) => value.split('.').next()?.parse().ok(),
            _ => None,
        }
    }

    /// Validate that every `{if <condition>}` tag in `instructions` and
    /// `personality` fields references a condition the engine can evaluate.
    ///
    /// Supported forms:
    /// - `input.<name>.exists` — `<name>` must be a declared input artifact
    ///   on the same state.
    /// - `mcp.<name>.available` — `<name>` must appear in the state's
    ///   `mcp_servers` list (including `defaults.mcp_servers` ids when
    ///   inherited is not cleared; this layer only checks the state-level
    ///   declaration since defaults live in settings, not the machine).
    /// - `skill.<id>.available` — same rule for the `skills` list.
    fn validate_template_conditions(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            let mut fields: Vec<(&str, &str)> = Vec::new();
            if let Some(reference) = state.prompt_template.as_ref() {
                if let Some(template) = self.prompt_templates.get(reference.name().trim()) {
                    fields.push(("prompt_template", template.instructions.as_str()));
                }
            }
            if let Some(text) = state.personality.as_deref() {
                fields.push(("personality", text));
            }
            if let Some(text) = state.instructions.as_deref() {
                fields.push(("instructions", text));
            }

            for (field_name, text) in fields {
                validate_no_nested_conditionals(state_name, field_name, text)?;
                for condition in extract_if_conditions(text) {
                    if let Some(input_name) =
                        condition.strip_prefix("input.").and_then(|s| s.strip_suffix(".exists"))
                    {
                        if !state.inputs.iter().any(|a| a.name == input_name) {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' {field_name} contains \
                                 '{{if {condition}}}' but '{input_name}' is not a declared input \
                                 on this state"
                            )));
                        }
                    } else if let Some(mcp_id) =
                        condition.strip_prefix("mcp.").and_then(|s| s.strip_suffix(".available"))
                    {
                        let declared = state
                            .mcp_servers
                            .as_deref()
                            .map(|entries| entries.iter().any(|e| e.id() == mcp_id))
                            .unwrap_or(false);
                        if !declared {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' {field_name} contains \
                                 '{{if {condition}}}' but '{mcp_id}' is not declared in this state's \
                                 'mcp_servers' list"
                            )));
                        }
                    } else if let Some(skill_id) =
                        condition.strip_prefix("skill.").and_then(|s| s.strip_suffix(".available"))
                    {
                        let declared = state
                            .skills
                            .as_deref()
                            .map(|entries| entries.iter().any(|e| e.id() == skill_id))
                            .unwrap_or(false);
                        if !declared {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' {field_name} contains \
                                 '{{if {condition}}}' but '{skill_id}' is not declared in this state's \
                                 'skills' list"
                            )));
                        }
                    } else {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' {field_name} contains \
                             '{{if {condition}}}' which is not a recognised condition; \
                             supported forms: 'input.<name>.exists', 'mcp.<name>.available', 'skill.<id>.available'"
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}
