
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
    provenance: &mut BTreeMap<String, RosterOrigin>,
) {
    // Origin changes beside the value, so the two cannot drift. §FS-rhei-agents.1.1.7
    if json_field_present(project_raw, "args") {
        existing.args = project.args;
        provenance.insert("args".to_string(), RosterOrigin::Project);
    }
    if json_field_present(project_raw, "autonomous_args") {
        existing.autonomous_args = project.autonomous_args;
        provenance.insert("autonomous_args".to_string(), RosterOrigin::Project);
    }
    if json_field_present(project_raw, "timeout") {
        existing.timeout = project.timeout;
        provenance.insert("timeout".to_string(), RosterOrigin::Project);
    }
}

fn load_merged_settings_for_completion(plan_root: &Path) -> RheiSettings {
    // Shell completion must not fail because a project settings file is half-written.
    load_merged_settings(plan_root)
        .unwrap_or_else(|_| RheiSettings { agents: built_in_agents(), ..Default::default() })
}

fn raw_object_keys(raw: &serde_json::Value) -> BTreeSet<String> {
    // Wholesale agents still preserve omission versus explicit defaults. §FS-rhei-agents.1.1.7
    raw.as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn built_in_agent_fields(profile: &CustomAgentProfile) -> BTreeSet<String> {
    // Programmatic built-ins supply only their non-default optional fields. §FS-rhei-agents.1.1.7
    let mut fields = BTreeSet::from(["command".to_string()]);
    for (name, supplied) in [
        ("prompt_flag", profile.prompt_flag.is_some()),
        ("model_flag", profile.model_flag.is_some()),
        ("stdin_prompt", profile.stdin_prompt),
        ("intervene_stdin", profile.intervene_stdin),
        ("timeout", profile.timeout.is_some()),
        ("mcp_flag", profile.mcp_flag.is_some()),
        ("mcp_config_flag", profile.mcp_config_flag.is_some()),
        ("skill_flag", profile.skill_flag.is_some()),
        ("modes", !profile.modes.is_empty()),
        ("session", profile.session.is_some()),
    ] {
        if supplied {
            fields.insert(name.to_string());
        }
    }
    fields
}

fn field_origins(raw: &serde_json::Value, fields: &[&str], origin: RosterOrigin) -> BTreeMap<String, RosterOrigin> {
    // Raw presence distinguishes omission from a supplied clear. §FS-rhei-agents.1.1.7
    fields
        .iter()
        .filter(|field| json_field_present(raw, field))
        .map(|field| ((*field).to_string(), origin))
        .collect()
}

fn model_provenance(raw: &serde_json::Value, origin: RosterOrigin) -> ModelRosterProvenance {
    // Models and bindings merge one field at a time. §FS-rhei-agents.1.1.7
    let fields = field_origins(raw, &["provider", "model", "default_agent"], origin);
    let agents = json_child(raw, "agents")
        .as_object()
        .into_iter()
        .flat_map(|bindings| bindings.iter())
        .map(|(id, binding)| {
            (
                id.clone(),
                field_origins(binding, &["args", "autonomous_args", "timeout"], origin),
            )
        })
        .collect();
    ModelRosterProvenance { fields, agents }
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

/// Reject effort capabilities that cannot translate one canonical value into
/// one deterministic argument span. §FS-rhei-agents.1.1.2
fn validate_agent_effort_profiles(
    agents: &BTreeMap<String, CustomAgentProfile>,
) -> MietteResult<()> {
    for (id, profile) in agents {
        let Some(effort) = profile.effort.as_ref() else { continue };
        if effort.values.is_empty() {
            return Err(miette!(
                help = "Provide at least one canonical effort value and native mapping.",
                "agent '{id}' has an empty 'effort.values' mapping"
            ));
        }
        for (canonical, native) in &effort.values {
            if !rhei_validator::StateEffort::is_canonical(canonical) {
                return Err(miette!(
                    help = "Use one of the documented canonical effort values.",
                    "agent '{id}' maps unknown canonical effort value '{canonical}'"
                ));
            }
            if native.trim().is_empty() {
                return Err(miette!(
                    help = "Provide a non-empty native value for this effort mapping.",
                    "agent '{id}' maps effort '{canonical}' to an empty native value"
                ));
            }
        }

        let validate_pattern = |field: &str, pattern: &[String]| -> MietteResult<()> {
            if pattern.is_empty() {
                return Err(miette!(
                    help = "Provide a token pattern containing the {value} placeholder.",
                    "agent '{id}' has an empty '{field}' effort pattern"
                ));
            }
            let placeholders =
                pattern.iter().map(|token| token.matches("{value}").count()).sum::<usize>();
            if placeholders != 1 {
                return Err(miette!(
                    help = "Include exactly one {value} placeholder in the pattern.",
                    "agent '{id}' effort '{field}' pattern must contain exactly one '{{value}}' placeholder"
                ));
            }
            Ok(())
        };
        validate_pattern("args", &effort.args)?;
        for pattern in &effort.conflicts {
            validate_pattern("conflicts", pattern)?;
        }
    }
    Ok(())
}

/// Load merged settings plus the source decisions that produced every roster
/// value. Execution discards the additional record; inspection renders it
/// without re-reading or re-merging settings. §FS-rhei-agents.1.1.7
fn load_merged_roster(
    plan_root: &Path,
    warn_before_project_load: bool,
) -> MietteResult<MergedRoster> {
    let global_document = match home_dir() {
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
    if warn_before_project_load {
        // Execution retains the existing warning timing. Roster defers it
        // until inspection succeeds so JSON errors stay singular.
        // §FS-rhei-agents.1.1.7
        project_settings.warn_if_deprecated();
    }
    let project_document = load_settings_document(project_settings.path())?;
    let sources = RosterSources {
        global: global_document.source_path.clone(),
        project: project_document
            .source_path
            .clone()
            .map(|path| (path, project_settings_file)),
    };
    let global_raw = &global_document.raw;
    let project_raw = &project_document.raw;
    let global = global_document.typed;
    let project = project_document.typed;

    // Agent registry: built-ins seed the map; global then project entries
    // replace an id wholesale when present.
    let mut agents = built_in_agents();
    let mut agent_fields: BTreeMap<String, BTreeSet<String>> = agents
        .iter()
        .map(|(id, profile)| (id.clone(), built_in_agent_fields(profile)))
        .collect();
    let mut provenance = RosterProvenance {
        agents: agents.keys().map(|id| (id.clone(), RosterOrigin::BuiltIn)).collect(),
        ..Default::default()
    };
    for (id, profile) in global.agents {
        agent_fields.insert(
            id.clone(),
            raw_object_keys(json_child(json_child(global_raw, "agents"), &id)),
        );
        provenance.agents.insert(id.clone(), RosterOrigin::Global);
        agents.insert(id, profile);
    }
    for (id, profile) in project.agents {
        agent_fields.insert(
            id.clone(),
            raw_object_keys(json_child(json_child(project_raw, "agents"), &id)),
        );
        provenance.agents.insert(id.clone(), RosterOrigin::Project);
        agents.insert(id, profile);
    }
    validate_agent_effort_profiles(&agents)?;

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
    provenance.models = models
        .keys()
        .map(|id| {
            let raw = json_child(json_child(global_raw, "models"), id);
            (id.clone(), model_provenance(raw, RosterOrigin::Global))
        })
        .collect();
    for (id, project_profile) in project.models {
        let project_model_raw = json_child(json_child(project_raw, "models"), &id);
        match models.get_mut(&id) {
            Some(existing) => {
                let model_origins = provenance.models.entry(id.clone()).or_default();
                if json_field_present(project_model_raw, "provider") {
                    existing.provider = project_profile.provider;
                    model_origins
                        .fields
                        .insert("provider".to_string(), RosterOrigin::Project);
                }
                if json_field_present(project_model_raw, "model") {
                    existing.model = project_profile.model;
                    model_origins.fields.insert("model".to_string(), RosterOrigin::Project);
                }
                if json_field_present(project_model_raw, "default_agent") {
                    existing.default_agent = project_profile.default_agent;
                    model_origins
                        .fields
                        .insert("default_agent".to_string(), RosterOrigin::Project);
                }
                // Rates are one authored object: absence inherits, while an
                // object or `null` replaces/clears it wholesale.
                // §FS-rhei-agents.1.3
                if json_field_present(project_model_raw, "prices") {
                    existing.prices = project_profile.prices;
                }
                for (agent_id, binding) in project_profile.agents {
                    let project_binding_raw =
                        json_child(json_child(project_model_raw, "agents"), &agent_id);
                    match existing.agents.get_mut(&agent_id) {
                        Some(existing_binding) => merge_model_agent_binding(
                            existing_binding,
                            binding,
                            project_binding_raw,
                            model_origins.agents.entry(agent_id).or_default(),
                        ),
                        None => {
                            model_origins.agents.insert(
                                agent_id.clone(),
                                field_origins(
                                    project_binding_raw,
                                    &["args", "autonomous_args", "timeout"],
                                    RosterOrigin::Project,
                                ),
                            );
                            existing.agents.insert(agent_id, binding);
                        }
                    }
                }
            }
            None => {
                provenance.models.insert(
                    id.clone(),
                    model_provenance(project_model_raw, RosterOrigin::Project),
                );
                models.insert(id, project_profile);
            }
        }
    }

    // `defaults.mcp_servers` / `defaults.skills`: project replaces global
    // wholesale when present (including an explicit empty list).
    provenance.defaults = field_origins(
        json_child(global_raw, "defaults"),
        &[
            "model",
            "agent",
            "agent_mode",
            "agent_timeout",
            "program_timeout",
            "attempts",
            "mcp_servers",
            "skills",
        ],
        RosterOrigin::Global,
    );
    for field in [
        "model",
        "agent",
        "agent_mode",
        "agent_timeout",
        "program_timeout",
        "attempts",
        "mcp_servers",
        "skills",
    ] {
        if json_nested_field_present(project_raw, "defaults", field) {
            provenance.defaults.insert(field.to_string(), RosterOrigin::Project);
        }
    }
    let defaults = SettingsDefaults {
        // §FS-rhei-budgets.2.1: state, project defaults, global defaults.
        budget_threshold: if json_nested_field_present(project_raw, "defaults", "budget_threshold") {
            project.defaults.budget_threshold
        } else {
            global.defaults.budget_threshold
        },
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

    let settings = RheiSettings {
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
    };

    Ok(MergedRoster { settings, sources, provenance, agent_fields })
}

/// Preserve execution's settings-only interface while sharing exactly the
/// merge used by roster inspection. §FS-rhei-agents.1.1.7
fn load_merged_settings(plan_root: &Path) -> MietteResult<RheiSettings> {
    Ok(load_merged_roster(plan_root, true)?.settings)
}

fn validate_snapshot_plan_context(
    loaded: &LoadedPlan,
    machines: &ResolvedMachineSet,
) -> Vec<String> {
    let mut errors = Vec::new();
    fn visit(
        task: &rhei_core::ast::Task,
        machines: &ResolvedMachineSet,
        errors: &mut Vec<String>,
    ) {
        let machine = machines.machine_for_task_str(&task.id.to_string());
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let effective = effective_snapshot_inherit(machine, task, &state_name);
        if effective.as_ref().and_then(|inherit| inherit.from_axis.as_deref())
            == Some("ancestor")
            && task.profile_level() == 1
        {
            errors.push(format!(
                "Task {} is a root task in state '{}' but its effective snapshot inheritance uses from: ancestor (snapshot root tasks have no ancestor)",
                task.id, state_name
            ));
        }
        if effective.is_some()
            && machine.states.get(&state_name).is_some_and(|state| state.poll.is_some())
        {
            errors.push(format!(
                "Task {} has effective snapshot inheritance in polling state '{}'; polling states cannot inherit snapshots in v1",
                task.id, state_name
            ));
        }
        if let Some(inherit) = effective.as_ref() {
            if let Some(error) = effective_snapshot_emitter_error(task, inherit, machines) {
                errors.push(error);
            }
        }
        for child in &task.children {
            visit(child, machines, errors);
        }
    }
    for task in &loaded.rhei.tasks {
        visit(task, machines, &mut errors);
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
