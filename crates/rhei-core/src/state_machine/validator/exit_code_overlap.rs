// Two exit_code rules from one state that can match the same code, with nothing
// to tell them apart, are refused when the machine loads, so declaration order
// never decides where a program's exit goes.

// §FS-rhei-programs.8.2 §FS-rhei-programs.3.2

/// What one rule's `exit_code` matches: specific codes, or every nonzero code.
enum ExitCodeSet {
    Codes(Vec<i64>),
    Nonzero,
}

impl ExitCodeSet {
    /// The set `rule` matches, or `None` when it declares no (recognized) `exit_code`.
    /// Mirrors the router's `transition_matches_exit_code`; change both together.
    fn of(rule: &TransitionRule) -> Option<Self> {
        match rule.exit_code.as_ref()? {
            serde_yaml::Value::Number(number) => number.as_i64().map(|code| Self::Codes(vec![code])),
            serde_yaml::Value::Sequence(values) => {
                Some(Self::Codes(values.iter().filter_map(serde_yaml::Value::as_i64).collect()))
            }
            serde_yaml::Value::String(value) if value == "nonzero" => Some(Self::Nonzero),
            _ => None,
        }
    }

    /// How the code both sets match is named in the refusal, or `None` when
    /// they share none. A specific set never overlaps `nonzero`: step 2 of the
    /// evaluation order is tried before step 3. §FS-rhei-programs.3.2
    fn shared(&self, other: &Self) -> Option<String> {
        match (self, other) {
            (Self::Nonzero, Self::Nonzero) => Some("nonzero exit codes".to_string()),
            (Self::Codes(left), Self::Codes(right)) => left
                .iter()
                .filter(|code| right.contains(code))
                .min()
                .map(|code| format!("exit code {code}")),
            _ => None,
        }
    }
}

impl StateMachine {
    /// Refuse a state whose `exit_code` rules overlap with nothing to tell
    /// them apart, as §FS-rhei-programs.8.2 defines it: an overlapping pair
    /// stands only when both rules carry a `condition` and the trimmed texts
    /// differ, or, on a `poll:` state, when exactly one of the two is the
    /// self-loop retry (§FS-rhei-run.5.1). The first pair by declaration
    /// order is reported, naming the state, the shared code and both rules.
    fn validate_exit_code_overlaps(&self) -> Result<(), StateMachineLoadError> {
        let rules: Vec<(&TransitionRule, ExitCodeSet)> = self
            .transitions
            .iter()
            .filter(|rule| self.states.contains_key(&rule.from.0))
            .filter_map(|rule| ExitCodeSet::of(rule).map(|set| (rule, set)))
            .collect();

        for (index, (first, first_set)) in rules.iter().enumerate() {
            for (second, second_set) in &rules[index + 1..] {
                if first.from.0 != second.from.0 {
                    continue;
                }
                let Some(shared) = first_set.shared(second_set) else { continue };
                let state_name = &first.from.0;
                let polls = self.states.get(state_name).is_some_and(|state| state.poll.is_some());
                let first_loops = first.to.0 == *state_name;
                if polls && first_loops != (second.to.0 == *state_name) {
                    continue;
                }
                let first_condition = first.condition.as_deref().map(str::trim);
                let second_condition = second.condition.as_deref().map(str::trim);
                let why = match (first_condition, second_condition) {
                    (Some(left), Some(right)) if left != right => continue,
                    (Some(same), Some(_)) => {
                        format!("with the same condition '{same}', which cannot tell them apart")
                    }
                    _ => "with no condition to tell them apart".to_string(),
                };
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has two exit_code rules matching {shared} {why}: '{}' -> '{}' and '{}' -> '{}'",
                    first.from.0, first.to.0, second.from.0, second.to.0
                )));
            }
        }
        Ok(())
    }
}
