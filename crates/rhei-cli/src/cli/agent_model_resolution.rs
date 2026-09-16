fn resolve_model_profile<'a>(
    settings: &'a RheiSettings,
    model: Option<&str>,
) -> MietteResult<Option<&'a ModelProfile>> {
    let Some(id) = model else {
        return Ok(None);
    };
    settings.models.get(id).map(Some).ok_or_else(|| {
        let known = settings.models.keys().cloned().collect::<Vec<_>>();
        // The file the project resolves, not the write path: a project on
        // the deprecated home sent to the new one shadows its own settings.
        // §FS-rhei-agents.1.1
        miette!(
            help = format!(
                "{}Add a `models.{id}` entry to {} or \
                 ~/.config/rhei/settings.json, or drop the model selection.",
                did_you_mean(id, &known).map(|hint| format!("{hint} ")).unwrap_or_default(),
                settings.project_settings_file.relative_path()
            ),
            "model '{}' is not defined in settings.models",
            id
        )
    })
}

fn resolve_legacy_agent_timeout(
    state_def: Option<&rhei_validator::StateDef>,
    settings: &RheiSettings,
    profile: &CustomAgentProfile,
    binding: Option<&ModelAgentBinding>,
) -> Option<u64> {
    state_def
        .and_then(|d| d.agent_timeout.as_deref())
        .and_then(rhei_validator::parse_duration_secs)
        .or_else(|| {
            binding.and_then(|b| b.timeout.as_deref()).and_then(rhei_validator::parse_duration_secs)
        })
        .or_else(|| profile.timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| settings.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| {
            settings.defaults.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        })
}

/// Resolve the state's legacy identity first, then replace only its model
/// dimension while preserving agent, mode, and provider. §FS-rhei-plan-language.3.11
fn resolve_legacy_agent_with_task_model(
    state_def: Option<&rhei_validator::StateDef>,
    settings: &RheiSettings,
    opts: &RunOptions,
    model_override: &str,
) -> MietteResult<Option<ResolvedAgent>> {
    let Some(mut resolved) = resolve_legacy_agent_with_model(state_def, settings, opts, None)? else {
        return Ok(None);
    };
    let model_profile = resolve_model_profile(settings, Some(model_override))?
        .expect("a named task model has a model profile");
    let binding = model_profile.agents.get(resolved.agent.id());

    resolved.model = Some(model_override.to_string());
    resolved.model_name = model_profile.model.clone().or_else(|| resolved.model.clone());
    resolved.timeout_secs =
        resolve_legacy_agent_timeout(state_def, settings, &resolved.profile, binding);
    resolved.autonomous_args = binding.map(|b| b.autonomous_args.clone()).unwrap_or_default();
    // A task model changes the binding, not the state's effort policy; remap
    // the new binding arguments through the already selected agent profile.
    // §FS-rhei-plan-language.3.11 §FS-rhei-agents.1.4.1
    let agent_id = resolved.agent.id().to_string();
    apply_state_effort(
        state_def,
        &mut resolved.profile,
        resolved.mode.as_deref(),
        &mut resolved.autonomous_args,
        &agent_id,
    )?;
    Ok(Some(resolved))
}
