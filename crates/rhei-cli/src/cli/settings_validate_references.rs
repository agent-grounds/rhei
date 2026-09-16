// Registry and plan reference validation share the merged settings but do not
// participate in reading or merging their documents. §AR-source-file-size.3

/// The agents the merged registry knows, so an error does not leave an author
/// guessing at names nothing else lists — and where to declare the one they
/// wanted, which the listing alone never says. §FS-rhei-agents.1.1
/// §FS-rhei-errors.1.4
fn known_agents_hint(settings: &RheiSettings) -> String {
    let location =
        settings_entry_location("agents.<id>", settings.project_settings_file.relative_path());
    if settings.agents.is_empty() {
        return format!("no agents are configured; declare one under {location}");
    }
    let names: Vec<&str> = settings.agents.keys().map(String::as_str).collect();
    format!("known agents: {}; declare another under {location}", names.join(", "))
}

/// The modes one agent declares, listed the way an invalid state lists its
/// allowed states, and where the missing one is declared. §FS-rhei-agents.1.1
/// §FS-rhei-errors.1.4
fn known_modes_hint(settings: &RheiSettings, id: &str, profile: &CustomAgentProfile) -> String {
    let location = settings_entry_location(
        &format!("agents.{id}.modes"),
        settings.project_settings_file.relative_path(),
    );
    if profile.modes.is_empty() {
        // Both ways out, as spawn time offers them: the brackets are usually
        // the mistake, and declaring the mode is the other. §FS-rhei-errors.1.2
        return format!(
            "it declares no modes; drop the brackets from the selector, \
             or declare one under {location}"
        );
    }
    let modes: Vec<&str> = profile.modes.keys().map(String::as_str).collect();
    format!("known modes: {}; declare another under {location}", modes.join(", "))
}

/// Validate legacy executions with runtime's precedence; selector-owned modes
/// remain under the existing selector checks. §FS-rhei-agents.1.4.1
fn validate_effective_static_agent_modes(
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    errors: &mut Vec<String>,
    shadowed_by_task_target: impl Fn(&rhei_validator::StateDef) -> bool,
) {
    let opts = default_run_options();
    let mut refused = BTreeSet::new();

    for (state_name, state) in &machine.states {
        let uses_selector = state.target.is_some() || !state.all_targets.is_empty();
        let inactive = state.terminal || state.gating || state.program.is_some();
        if uses_selector
            || (inactive && state.agent_mode.is_none())
            || shadowed_by_task_target(state)
        {
            continue;
        }

        let model_overrides: Vec<Option<String>> = if state.all_models.is_empty() {
            vec![None]
        } else {
            state.all_models.iter().cloned().map(Some).collect()
        };

        for model_override in model_overrides {
            let model = select_legacy_model(Some(state), settings, &opts, model_override);
            let model_profile = model.as_deref().and_then(|id| settings.models.get(id));
            let Some(agent) = select_legacy_agent(Some(state), settings, &opts, model_profile)
            else {
                continue;
            };
            let Some(profile) = settings.agents.get(agent.id()) else {
                continue;
            };
            let Some(mode) = select_legacy_agent_mode(Some(state), settings, &opts, profile) else {
                continue;
            };
            if profile.modes.is_empty() || profile.modes.contains_key(&mode) {
                continue;
            }
            if refused.insert((agent.id().to_string(), mode.clone())) {
                errors.push(format!(
                    "agent '{}' has no mode '{}' in state '{}' ({})",
                    agent.id(),
                    mode,
                    state_name,
                    known_modes_hint(settings, agent.id(), profile)
                ));
            }
        }
    }
}

#[cfg(test)]
fn validate_machine_settings_references(
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> Vec<String> {
    validate_machine_settings_references_inner(machine, settings, true)
}

/// Validate registry entries without requiring or parsing a plan. This is the
/// common settings-only boundary used before roster rendering and as the first
/// phase of plan-aware validation. §FS-rhei-agents.1.1.7
fn validate_intrinsic_settings(settings: &RheiSettings) -> Vec<String> {
    let mut errors = Vec::new();

    // Agent registry self-validation: `command` is required, and
    // `mcp_flag` and `mcp_config_flag` are mutually exclusive per
    // §FS-rhei-agents.1.1.2: Validate agent transport profile settings.
    for (id, profile) in &settings.agents {
        if profile.command.is_empty() {
            errors.push(format!(
                "agent '{}' has an empty 'command'; the `command` field is required",
                id
            ));
        }
        if profile.mcp_flag.is_some() && profile.mcp_config_flag.is_some() {
            errors.push(format!(
                "agent '{}' declares both 'mcp_flag' and 'mcp_config_flag'; \
                 they are mutually exclusive",
                id
            ));
        }
    }

    // MCP server registry self-validation: exactly one of `command`/`url`;
    // §FS-rhei-agents.1.1.4: Validate MCP server registry entries.
    for (id, profile) in &settings.mcp_servers {
        match (profile.command.is_some(), profile.url.is_some()) {
            (false, false) => errors.push(format!(
                "mcp_servers.'{}' must declare exactly one of 'command' or 'url'",
                id
            )),
            (true, true) => errors.push(format!(
                "mcp_servers.'{}' declares both 'command' and 'url'; they are \
                 mutually exclusive",
                id
            )),
            (false, true) => {
                if profile.transport.as_deref().map_or(true, str::is_empty) {
                    errors.push(format!(
                        "mcp_servers.'{}' uses 'url' but does not declare 'transport'; \
                         set transport to 'sse' or 'websocket'",
                        id
                    ));
                }
            }
            (true, false) => {}
        }
    }

    // Model registry self-validation: `provider` and `model` are required
    // §FS-rhei-agents.1.1.3: Validate model profile registry entries.
    for (id, profile) in &settings.models {
        if profile.provider.as_deref().map_or(true, str::is_empty) {
            errors.push(format!("models.'{}' is missing required field 'provider'", id));
        }
        if profile.model.as_deref().map_or(true, str::is_empty) {
            errors.push(format!("models.'{}' is missing required field 'model'", id));
        }
    }

    errors
}

fn validate_machine_settings_references_inner(
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    validate_static_modes: bool,
) -> Vec<String> {
    let mut errors = validate_intrinsic_settings(settings);

    validate_mcp_entries_known(
        "defaults.mcp_servers",
        settings.defaults.mcp_servers.as_deref(),
        &settings.mcp_servers,
        &mut errors,
    );
    validate_skill_entries_known(
        "defaults.skills",
        settings.defaults.skills.as_deref(),
        &settings.skills,
        &mut errors,
    );

    if validate_static_modes {
        validate_effective_static_agent_modes(machine, settings, &mut errors, |_| false);
    }

    for (state_name, state) in &machine.states {
        validate_mcp_entries_known(
            &format!("state '{state_name}' mcp_servers"),
            state.mcp_servers.as_deref(),
            &settings.mcp_servers,
            &mut errors,
        );
        validate_skill_entries_known(
            &format!("state '{state_name}' skills"),
            state.skills.as_deref(),
            &settings.skills,
            &mut errors,
        );

        if let Some(agent) = state.agent.as_ref() {
            if !settings.agents.contains_key(agent.id()) {
                errors.push(format!(
                    "state '{}' references unknown agent '{}' ({})",
                    state_name,
                    agent.id(),
                    known_agents_hint(settings)
                ));
                continue;
            }
        }

        let selectors = state
            .target
            .iter()
            .cloned()
            .chain(state.all_targets.iter().cloned())
            .collect::<Vec<_>>();
        for selector in selectors {
            match parse_execution_target(&selector) {
                Ok(target) => {
                    let Some(profile) = settings.agents.get(target.agent.as_str()) else {
                        errors.push(format!(
                            "state '{}' references unknown target agent '{}' in '{}' ({})",
                            state_name,
                            target.agent,
                            selector,
                            known_agents_hint(settings)
                        ));
                        continue;
                    };
                    if let Some(mode) = target.mode.as_deref() {
                        if !profile.modes.contains_key(mode) {
                            errors.push(format!(
                                "state '{}' references unknown target mode '{}' for agent '{}' in '{}' ({})",
                                state_name,
                                mode,
                                target.agent,
                                selector,
                                known_modes_hint(settings, &target.agent, profile)
                            ));
                        }
                    }
                }
                Err(err) => errors.push(format!(
                    "state '{}' has invalid target selector '{}': {}",
                    state_name, selector, err
                )),
            }
        }

        // Static selections are resolved as a set so every fanout member is
        // representable before validation succeeds; resolution performs no
        // scheduling or spawn. §FS-rhei-states.1.3
        if validate_static_modes && state.effort.is_some() {
            if let Err(error) =
                resolve_agent_invocations(machine, state_name, settings, &default_run_options())
            {
                errors.push(format!("state '{state_name}' has invalid effort selection: {error}"));
            }
        }

        if state.snapshot.as_ref().and_then(|snapshot| snapshot.emit.as_ref()).is_some()
            || state.snapshot.as_ref().and_then(|snapshot| snapshot.inherit.as_ref()).is_some()
        {
            // Settings-aware snapshot checks need the merged agent/model
            // registry, so they live in the CLI validation layer rather than
            // §FS-rhei-snapshots.9.2 §FS-rhei-snapshots.11: Registry-aware checks.
            match resolve_agent_invocations(machine, state_name, settings, &default_run_options()) {
                Ok(invocations) if invocations.is_empty() => {
                    errors.push(format!(
                        "state '{}' declares snapshot operations but no effective target tuple resolves (snapshot-requires-target)",
                        state_name
                    ));
                }
                Ok(invocations) => {
                    let mut seen_slugs: HashMap<String, String> = HashMap::new();
                    for invocation in &invocations {
                        let Some(slug) = resolved_agent_target_slug(invocation) else {
                            errors.push(format!(
                                "state '{}' declares snapshot operations but agent '{}' does not resolve provider and model (snapshot-requires-target)",
                                state_name,
                                invocation.agent.id()
                            ));
                            continue;
                        };
                        if let Some(previous) =
                            seen_slugs.insert(slug.clone(), invocation.agent.id().to_string())
                        {
                            errors.push(format!(
                                "state '{}' has multiple resolved invocations for agents '{}' and '{}' that normalize to snapshot target slug '{}'",
                                state_name,
                                previous,
                                invocation.agent.id(),
                                slug
                            ));
                        }
                        if state
                            .snapshot
                            .as_ref()
                            .and_then(|snapshot| snapshot.emit.as_ref())
                            .is_some()
                            && !profile_has_snapshot_layout(&invocation.profile.session)
                        {
                            errors.push(format!(
                                "state '{}' declares snapshot.emit but agent '{}' has no supported snapshot session layout (unsupported-snapshot-session){}",
                                state_name,
                                invocation.agent.id(),
                                snapshot_removal_hint(state)
                            ));
                        }
                        if state
                            .snapshot
                            .as_ref()
                            .and_then(|snapshot| snapshot.inherit.as_ref())
                            .is_some_and(|inherit| inherit.required == Some(true))
                            && !profile_has_snapshot_preload(&invocation.profile.session)
                        {
                            errors.push(format!(
                                "state '{}' declares required snapshot.inherit but agent '{}' has no supported snapshot preload strategy (unsupported-snapshot-session){}",
                                state_name,
                                invocation.agent.id(),
                                snapshot_removal_hint(state)
                            ));
                        }
                    }
                }
                Err(err) => errors.push(format!(
                    "state '{}' declares snapshot operations but no effective target tuple resolves: {} (snapshot-requires-target)",
                    state_name, err
                )),
            }
        }
    }

    errors
}

fn validate_mcp_entries_known(
    label: &str,
    entries: Option<&[StateMcpEntry]>,
    registry: &BTreeMap<String, McpServerProfile>,
    errors: &mut Vec<String>,
) {
    for entry in entries.unwrap_or(&[]) {
        if !entry.is_inline() && !registry.contains_key(entry.id()) {
            errors.push(format!("{label} references unknown mcp server '{}'", entry.id()));
        }
    }
}

fn validate_skill_entries_known(
    label: &str,
    entries: Option<&[StateSkillEntry]>,
    registry: &BTreeMap<String, SkillProfile>,
    errors: &mut Vec<String>,
) {
    for entry in entries.unwrap_or(&[]) {
        if !entry.is_inline() && !registry.contains_key(entry.id()) {
            errors.push(format!("{label} references unknown skill '{}'", entry.id()));
        }
    }
}

fn tasks_for_machine<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    machine: &rhei_validator::StateMachine,
) -> Vec<&'a rhei_core::ast::Task> {
    fn visit<'a>(
        tasks: &'a [rhei_core::ast::Task],
        machines: &rhei_validator::MachineSet,
        fingerprint: &str,
        matching: &mut Vec<&'a rhei_core::ast::Task>,
    ) {
        for task in tasks {
            if machines.for_task(&task.id).fingerprint() == fingerprint {
                matching.push(task);
            }
            visit(&task.children, machines, fingerprint, matching);
        }
    }

    let mut matching = Vec::new();
    visit(&rhei.tasks, machines, &machine.fingerprint(), &mut matching);
    matching
}

/// Validate merged-settings references in the execution contexts the plan can
/// actually use. A task `**Target:**` owns its full identity and cannot make a
/// shadowed settings mode applicable. §FS-rhei-validate.4 §FS-rhei-agents.1.4.1
fn validate_plan_settings_references(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
) -> Vec<String> {
    let mut errors = Vec::new();
    for machine in machines.distinct() {
        errors.extend(validate_machine_settings_references_inner(
            machine,
            settings,
            false,
        ));
        let tasks = tasks_for_machine(rhei, machines, machine);
        let opts = default_run_options();
        validate_effective_static_agent_modes(machine, settings, &mut errors, |state| {
            !tasks.is_empty()
                && tasks.iter().all(|task| {
                    task.target.is_some()
                        && task_execution_override_applies_to_state(state, settings, &opts)
                })
        });
        validate_effective_state_efforts(machine, settings, &tasks, &mut errors);
    }
    errors.extend(validate_task_execution_override_settings_references(rhei, settings));
    errors
}

/// Resolve each state effort against every task identity that can reach it.
/// Full target overrides therefore validate the replacement profile instead
/// of a shadowed state profile, and fanout resolves completely before success.
/// §FS-rhei-states.1.3 §FS-rhei-plan-language.3.11
fn validate_effective_state_efforts(
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    tasks: &[&rhei_core::ast::Task],
    errors: &mut Vec<String>,
) {
    let opts = default_run_options();
    let mut refused = BTreeSet::new();
    for (state_name, state) in &machine.states {
        if state.effort.is_none() {
            continue;
        }
        if tasks.is_empty() {
            if let Err(error) = resolve_agent_invocations(machine, state_name, settings, &opts) {
                refused.insert(format!(
                    "state '{state_name}' has invalid effort selection: {error}"
                ));
            }
            continue;
        }
        for task in tasks {
            if let Err(error) =
                resolve_agent_invocations_for_task(machine, state_name, settings, &opts, Some(task))
            {
                refused.insert(format!(
                    "state '{state_name}' has invalid effort selection for Task {}: {error}",
                    task.id
                ));
            }
        }
    }
    errors.extend(refused);
}

fn validate_task_execution_override_settings_references(
    rhei: &rhei_core::ast::Rhei,
    settings: &RheiSettings,
) -> Vec<String> {
    fn visit(task: &rhei_core::ast::Task, settings: &RheiSettings, errors: &mut Vec<String>) {
        if let Some(selector) = task.target.as_deref() {
            match parse_execution_target(selector) {
                Ok(target) => {
                    let Some(profile) = settings.agents.get(target.agent.as_str()) else {
                        errors.push(format!(
                            "Task {} references unknown target agent '{}' in **Target:** '{}' ({})",
                            task.id,
                            target.agent,
                            selector,
                            known_agents_hint(settings)
                        ));
                        return;
                    };
                    if let Some(mode) = target.mode.as_deref() {
                        if !profile.modes.contains_key(mode) {
                            errors.push(format!(
                                "Task {} references unknown target mode '{}' for agent '{}' in **Target:** '{}' ({})",
                                task.id,
                                mode,
                                target.agent,
                                selector,
                                known_modes_hint(settings, &target.agent, profile)
                            ));
                        }
                    }
                }
                Err(_) => {
                    // Shape errors are reported by the semantic validator.
                }
            }
        }

        for child in &task.children {
            visit(child, settings, errors);
        }
    }

    // §FS-rhei-plan-language.3.11: Task `**Target:**` uses state target registry checks.
    let mut errors = Vec::new();
    for task in &rhei.tasks {
        visit(task, settings, &mut errors);
    }
    errors
}
