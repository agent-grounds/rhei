
/// Load settings from a JSON file, returning defaults if the file doesn't exist.
#[cfg(test)]
fn load_settings(path: &Path) -> MietteResult<RheiSettings> {
    Ok(load_settings_document(path)?.typed)
}

fn json_field_present(raw: &serde_json::Value, key: &str) -> bool {
    raw.as_object().map(|obj| obj.contains_key(key)).unwrap_or(false)
}

fn json_child<'a>(raw: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    raw.as_object().and_then(|obj| obj.get(key)).unwrap_or(&serde_json::Value::Null)
}

fn json_nested_field_present(raw: &serde_json::Value, section: &str, key: &str) -> bool {
    json_child(raw, section).as_object().map(|obj| obj.contains_key(key)).unwrap_or(false)
}

fn merge_model_agent_binding(
    existing: &mut ModelAgentBinding,
    project: ModelAgentBinding,
    project_raw: &serde_json::Value,
) {
    if json_field_present(project_raw, "args") {
        existing.args = project.args;
    }
    if json_field_present(project_raw, "autonomous_args") {
        existing.autonomous_args = project.autonomous_args;
    }
    if json_field_present(project_raw, "timeout") {
        existing.timeout = project.timeout;
    }
}

fn load_merged_settings_for_completion(plan_root: &Path) -> RheiSettings {
    // Shell completion must not fail because a project settings file is half-written.
    load_merged_settings(plan_root)
        .unwrap_or_else(|_| RheiSettings { agents: built_in_agents(), ..Default::default() })
}

/// The settings file's name under either home. §FS-rhei-agents.1.1
const PROJECT_SETTINGS_FILE: &str = "settings.json";

/// Where rhei writes a project settings file, and what a message names when
/// this is the file the project resolves. §FS-rhei-templates.1.1
const PROJECT_SETTINGS_RELATIVE_PATH: &str = ".agent-grounds/rhei/settings.json";

/// The deprecated file, named by the same messages when it is the one being
/// read: telling that project to write to the path above would have it create
/// a file that shadows its own registry. §FS-rhei-agents.1.1
const DEPRECATED_PROJECT_SETTINGS_RELATIVE_PATH: &str = ".agents/rhei/settings.json";

/// The project settings file: `.agent-grounds/rhei/settings.json`, and failing
/// that the deprecated `.agents/rhei/settings.json`. First match wins and the
/// two are never merged — merging would invent a precedence tier between global
/// and project that nothing else has. §FS-rhei-agents.1.1
fn project_settings_home(plan_root: &Path) -> RheiHomePath {
    resolve_rhei_home_file(plan_root, PROJECT_SETTINGS_FILE).unwrap_or_else(|| {
        RheiHomePath::plain(rhei_home_write_path(plan_root, PROJECT_SETTINGS_FILE))
    })
}

fn project_settings_path(plan_root: &Path) -> PathBuf {
    project_settings_home(plan_root).into_path()
}

/// Load merged settings: built-ins, then global, then project-level overrides.
fn load_merged_settings(plan_root: &Path) -> MietteResult<RheiSettings> {
    let global = match home_dir() {
        Ok(home) => load_settings_document(&home.join(".config/rhei/settings.json"))?,
        Err(_) => empty_settings_document(),
    };

    // §FS-rhei-agents.1.1: project settings live in rhei's project-local home.
    let project_settings = project_settings_home(plan_root);
    // Which of the two was read is what every later message about project
    // settings names. §FS-rhei-agents.1.1
    let project_settings_file = match project_settings.deprecated_path() {
        Some(_) => ProjectSettingsFile::Deprecated,
        None => ProjectSettingsFile::Current,
    };
    project_settings.warn_if_deprecated();
    let project = load_settings_document(project_settings.path())?;
    let project_raw = &project.raw;
    let global = global.typed;
    let project = project.typed;

    // Agent registry: built-ins seed the map; global then project entries
    // replace an id wholesale when present.
    let mut agents = built_in_agents();
    for (id, profile) in global.agents {
        agents.insert(id, profile);
    }
    for (id, profile) in project.agents {
        agents.insert(id, profile);
    }

    // Registries merge by id: start with global, override by project.
    let mut mcp_servers = global.mcp_servers.clone();
    for (id, profile) in project.mcp_servers {
        mcp_servers.insert(id, profile);
    }
    let mut skills = global.skills.clone();
    for (id, profile) in project.skills {
        skills.insert(id, profile);
    }
    // `models` merge by model id; within a matching id, `models.<id>.agents`
    // is deep-merged by agent id.
    // §FS-rhei-agents.1.3: Merge models by id and model-agent bindings by agent id.
    let mut models = global.models.clone();
    for (id, project_profile) in project.models {
        let project_model_raw = json_child(json_child(project_raw, "models"), &id);
        match models.get_mut(&id) {
            Some(existing) => {
                if json_field_present(project_model_raw, "provider") {
                    existing.provider = project_profile.provider;
                }
                if json_field_present(project_model_raw, "model") {
                    existing.model = project_profile.model;
                }
                if json_field_present(project_model_raw, "default_agent") {
                    existing.default_agent = project_profile.default_agent;
                }
                for (agent_id, binding) in project_profile.agents {
                    let project_binding_raw =
                        json_child(json_child(project_model_raw, "agents"), &agent_id);
                    match existing.agents.get_mut(&agent_id) {
                        Some(existing_binding) => merge_model_agent_binding(
                            existing_binding,
                            binding,
                            project_binding_raw,
                        ),
                        None => {
                            existing.agents.insert(agent_id, binding);
                        }
                    }
                }
            }
            None => {
                models.insert(id, project_profile);
            }
        }
    }

    // `defaults.mcp_servers` / `defaults.skills`: project replaces global
    // wholesale when present (including an explicit empty list).
    let defaults = SettingsDefaults {
        model: if json_nested_field_present(project_raw, "defaults", "model") {
            project.defaults.model
        } else {
            global.defaults.model
        },
        agent: if json_nested_field_present(project_raw, "defaults", "agent") {
            project.defaults.agent
        } else {
            global.defaults.agent
        },
        agent_mode: if json_nested_field_present(project_raw, "defaults", "agent_mode") {
            project.defaults.agent_mode
        } else {
            global.defaults.agent_mode
        },
        agent_timeout: if json_nested_field_present(project_raw, "defaults", "agent_timeout") {
            project.defaults.agent_timeout
        } else {
            global.defaults.agent_timeout
        },
        program_timeout: if json_nested_field_present(project_raw, "defaults", "program_timeout") {
            project.defaults.program_timeout
        } else {
            global.defaults.program_timeout
        },
        attempts: if json_nested_field_present(project_raw, "defaults", "attempts") {
            project.defaults.attempts
        } else {
            global.defaults.attempts
        },
        mcp_servers: if json_nested_field_present(project_raw, "defaults", "mcp_servers") {
            project.defaults.mcp_servers
        } else {
            global.defaults.mcp_servers
        },
        skills: if json_nested_field_present(project_raw, "defaults", "skills") {
            project.defaults.skills
        } else {
            global.defaults.skills
        },
    };

    Ok(RheiSettings {
        project_settings_file,
        agent: if json_field_present(project_raw, "agent") { project.agent } else { global.agent },
        agent_mode: if json_field_present(project_raw, "agent_mode") {
            project.agent_mode
        } else {
            global.agent_mode
        },
        model: if json_field_present(project_raw, "model") { project.model } else { global.model },
        agent_timeout: if json_field_present(project_raw, "agent_timeout") {
            project.agent_timeout
        } else {
            global.agent_timeout
        },
        program_timeout: if json_field_present(project_raw, "program_timeout") {
            project.program_timeout
        } else {
            global.program_timeout
        },
        defaults,
        agents,
        models,
        mcp_servers,
        skills,
        snapshots: if json_field_present(project_raw, "snapshots") && project.snapshots.is_none() {
            None
        } else {
            merge_snapshot_settings(global.snapshots, project.snapshots)
        },
    })
}

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
) {
    let opts = default_run_options();
    let mut refused = BTreeSet::new();

    for (state_name, state) in &machine.states {
        let uses_selector = state.target.is_some() || !state.all_targets.is_empty();
        if state.terminal || state.gating || state.program.is_some() || uses_selector {
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

fn validate_machine_settings_references_inner(
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    validate_static_modes: bool,
) -> Vec<String> {
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
        validate_effective_static_agent_modes(machine, settings, &mut errors);
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

fn machine_has_unshadowed_task_selection(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    machine: &rhei_validator::StateMachine,
) -> bool {
    fn visit(
        tasks: &[rhei_core::ast::Task],
        machines: &rhei_validator::MachineSet,
        fingerprint: &str,
    ) -> bool {
        tasks.iter().any(|task| {
            (machines.for_task(&task.id).fingerprint() == fingerprint && task.target.is_none())
                || visit(&task.children, machines, fingerprint)
        })
    }

    visit(&rhei.tasks, machines, &machine.fingerprint())
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
        let validate_static_modes = machine_has_unshadowed_task_selection(rhei, machines, machine);
        errors.extend(validate_machine_settings_references_inner(
            machine,
            settings,
            validate_static_modes,
        ));
    }
    errors.extend(validate_task_execution_override_settings_references(rhei, settings));
    errors
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

fn validate_snapshot_plan_context(
    loaded: &LoadedPlan,
    machines: &ResolvedMachineSet,
) -> Vec<String> {
    let mut errors = Vec::new();
    for task in &loaded.rhei.tasks {
        let machine = machines.machine_for_task_str(&task.id.to_string());
        let state_name = normalized_state_name(task.state.as_str(), machine);
        if machine
            .states
            .get(&state_name)
            .and_then(|state| state.snapshot.as_ref())
            .and_then(|snapshot| snapshot.inherit.as_ref())
            .and_then(|inherit| inherit.from_axis.as_deref())
            == Some("ancestor")
        {
            errors.push(format!(
                "Task {} is a root task in state '{}' but that state declares snapshot.inherit.from: ancestor (snapshot root tasks have no ancestor)",
                task.id, state_name
            ));
        }
    }
    errors
}

fn snapshot_orphan_validation_warnings(
    workspace_root: &Path,
    loaded: &LoadedPlan,
    machines: &ResolvedMachineSet,
    settings: &RheiSettings,
) -> MietteResult<Vec<String>> {
    let cache_root = snapshot_cache_dir(settings, workspace_root);
    if !cache_root.exists() {
        return Ok(Vec::new());
    }
    let records = read_snapshot_records(&cache_root)?;
    let mut warnings = Vec::new();
    for record in records {
        // A snapshot belongs to one ticket; judge it under that ticket's
        // machine. §DA-per-rhei-state-machines
        let machine = machines.machine_for_task_str(&record.task_id);
        if snapshot_record_is_orphaned_for_loaded(&record, loaded, machine, settings) {
            warnings.push(format!(
                "snapshot {} is orphaned relative to the current plan/state machine",
                record.display_ref()
            ));
        }
    }
    Ok(warnings)
}

fn snapshot_record_is_orphaned_for_loaded(
    record: &SnapshotRecord,
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> bool {
    let task_exists =
        flatten_tasks(&loaded.rhei).into_iter().any(|task| task.id.to_string() == record.task_id);
    if !task_exists {
        return true;
    }
    if !machine.states.contains_key(&record.emitting_state) {
        return true;
    }
    let Ok(slugs) = effective_target_slugs_for_state(machine, &record.emitting_state, settings)
    else {
        return true;
    };
    slugs.is_empty() || !slugs.contains(&record.target_slug)
}

/// What to do about a `snapshot:` block the resolved agent cannot honour.
///
/// On a supervising state the block is *recommended*, so an author who followed
/// the recommendation onto an agent without session support hits a hard error
/// with nothing to do about it. Supervision works without it — worse, but it
/// works — and that is the sentence the error owes them.
// §FS-rhei-supervision.1.1 §FS-rhei-supervision.6
fn snapshot_removal_hint(state: &rhei_validator::StateDef) -> &'static str {
    if state.execute_on().is_some() {
        "; remove the `snapshot:` block \u{2014} the supervisor still receives \
         `## Checkpoints` and the briefs (\u{a7}FS-rhei-supervision.6), it just starts each \
         visit cold"
    } else {
        ""
    }
}

fn profile_has_snapshot_layout(session: &Option<serde_json::Value>) -> bool {
    session.as_ref().is_some_and(snapshot_emit_session_supported)
}

fn profile_has_snapshot_preload(session: &Option<serde_json::Value>) -> bool {
    session.as_ref().is_some_and(snapshot_preload_session_supported)
}
