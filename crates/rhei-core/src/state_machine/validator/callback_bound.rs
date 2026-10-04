// The machine's `callback_timeout`: where it may be written, how a rule
// resolves it, and what validation says about it. §FS-rhei-transitions.4.10

impl StateMachine {
    /// The bound a rule's callbacks run under: the rule's own `callback_timeout`,
    /// then the machine's, then none, which is the unbounded behaviour of a
    /// machine that declares neither. A redirect resolves from the rule it
    /// redirects to, so callers pass whichever rule is firing.
    // §FS-rhei-transitions.4.10
    pub fn callback_bound(&self, rule: &TransitionRule) -> Option<crate::callback::CallbackBound> {
        let authored = rule.callback_timeout.as_deref().or(self.callback_timeout.as_deref())?;
        // Validation refused anything else at load, so a value that does not
        // parse to a non-zero duration cannot reach here.
        let secs = parse_duration_secs(authored).filter(|secs| *secs > 0)?;
        Some(crate::callback::CallbackBound {
            limit: std::time::Duration::from_secs(secs),
            authored: authored.to_string(),
        })
    }

    /// Refuse a `callback_timeout` that is not a duration, or is zero, at the
    /// machine's root and on every rule, naming where it was written.
    // §FS-rhei-validate.4 §FS-rhei-transitions.4.10
    fn validate_callback_bounds(&self) -> Result<(), StateMachineLoadError> {
        if let Some(bound) = self.callback_timeout.as_deref() {
            check_callback_bound("the machine", bound)?;
        }
        for rule in &self.transitions {
            if let Some(bound) = rule.callback_timeout.as_deref() {
                check_callback_bound(
                    &format!("the edge '{} -> {}'", rule.from.0, rule.to.0),
                    bound,
                )?;
            }
        }
        Ok(())
    }
}

/// One `callback_timeout` value, refused when malformed or zero.
// §FS-rhei-validate.4
fn check_callback_bound(place: &str, bound: &str) -> Result<(), StateMachineLoadError> {
    match parse_duration_secs(bound) {
        Some(0) => Err(StateMachineLoadError::Invalid(format!(
            "{place} has 'callback_timeout' value '{bound}', but a callback bound must be greater \
             than zero (omit it to leave callbacks unbounded)"
        ))),
        Some(_) => Ok(()),
        None => Err(StateMachineLoadError::Invalid(format!(
            "{place} has invalid 'callback_timeout' value '{bound}' \
             (expected format like '30s', '5m', '1h', '2h30m')"
        ))),
    }
}

/// Warn about a rule that declares a `callback_timeout` but no callback for it
/// to bound: the value changes nothing at runtime, so it is reported rather
/// than refused.
// §FS-rhei-validate.4
fn warn_on_idle_callback_bounds(machine: &StateMachine, report: &mut ValidationReport) {
    for rule in &machine.transitions {
        if rule.callback_timeout.is_some() && rule.on_leave.is_none() && rule.on_enter.is_none() {
            report.warnings.push(format!(
                "transition {} -> {} declares 'callback_timeout' but neither 'on_leave' nor \
                 'on_enter'; there is no callback for it to bound",
                rule.from.0, rule.to.0
            ));
        }
    }
}
