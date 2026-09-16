/// Match one configured conflict token, including an embedded placeholder.
/// §FS-rhei-agents.2.2
fn effort_token_matches(pattern: &str, actual: &str) -> bool {
    let Some((prefix, suffix)) = pattern.split_once("{value}") else {
        return pattern == actual;
    };
    actual.starts_with(prefix)
        && actual.ends_with(suffix)
        && actual.len() >= prefix.len() + suffix.len()
}

/// Remove complete conflict spans without disturbing unrelated argument order.
/// §FS-rhei-agents.2.2
fn remove_effort_conflicts(tokens: &[String], patterns: &[Vec<String>]) -> Vec<String> {
    let mut retained = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let matched = patterns.iter().find(|pattern| {
            index + pattern.len() <= tokens.len()
                && pattern
                    .iter()
                    .zip(&tokens[index..index + pattern.len()])
                    .all(|(expected, actual)| effort_token_matches(expected, actual))
        });
        if let Some(pattern) = matched {
            index += pattern.len();
        } else {
            retained.push(tokens[index].clone());
            index += 1;
        }
    }
    retained
}

/// Translate state effort after identity selection. Filtering each existing
/// argument source here preserves mode selection while giving explicit state
/// policy one dedicated span after model-binding arguments.
/// §FS-rhei-agents.2.2 §FS-rhei-states.5
fn apply_state_effort(
    state_def: Option<&rhei_validator::StateDef>,
    profile: &mut CustomAgentProfile,
    mode: Option<&str>,
    autonomous_args: &mut Vec<String>,
    agent_id: &str,
) -> MietteResult<()> {
    let Some(value) = state_def.and_then(|state| state.effort) else { return Ok(()) };
    let Some(mapping) = profile.effort.clone() else { return Ok(()) };
    let native = mapping.values.get(value.as_str()).ok_or_else(|| {
        miette!(
            help = "Configure a native mapping for this canonical effort value, or use a supported value.",
            "agent '{}' cannot represent state effort '{}'",
            agent_id,
            value.as_str()
        )
    })?;

    let mut patterns = mapping.conflicts;
    patterns.push(mapping.args.clone());
    if profile.command.len() > 1 {
        let mut command = vec![profile.command[0].clone()];
        command.extend(remove_effort_conflicts(&profile.command[1..], &patterns));
        profile.command = command;
    }
    if let Some(flags) = mode.and_then(|name| profile.modes.get_mut(name)) {
        *flags = remove_effort_conflicts(flags, &patterns);
    }
    *autonomous_args = remove_effort_conflicts(autonomous_args, &patterns);
    autonomous_args.extend(
        mapping.args.iter().map(|token| token.replace("{value}", native)),
    );
    Ok(())
}
