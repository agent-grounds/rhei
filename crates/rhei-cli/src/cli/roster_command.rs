// `rhei roster` payload construction and presentation. The settings loader
// owns selection and merging; this module only renders the result it receives.
// §FS-rhei-agents.1.1.7

/// A stable source key used by every provenance leaf. §FS-rhei-agents.1.1.7
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RosterOrigin {
    BuiltIn,
    Global,
    Project,
}

impl RosterOrigin {
    /// Render the public JSON spelling. §FS-rhei-agents.1.1.7
    fn label(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

/// Field origins for one effective model and its bindings. §FS-rhei-agents.1.1.7
#[derive(Clone, Debug, Default)]
struct ModelRosterProvenance {
    fields: BTreeMap<String, RosterOrigin>,
    agents: BTreeMap<String, BTreeMap<String, RosterOrigin>>,
}

/// Provenance at each unit that settings merge independently. §FS-rhei-agents.1.1.7
#[derive(Clone, Debug, Default)]
struct RosterProvenance {
    agents: BTreeMap<String, RosterOrigin>,
    models: BTreeMap<String, ModelRosterProvenance>,
    defaults: BTreeMap<String, RosterOrigin>,
}

/// Files actually selected from the optional filesystem layers. §FS-rhei-agents.1.1.7
#[derive(Clone, Debug, Default)]
struct RosterSources {
    global: Option<PathBuf>,
    project: Option<(PathBuf, ProjectSettingsFile)>,
}

/// Effective execution settings plus their roster-only inspection record. §FS-rhei-agents.1.1.7
#[derive(Clone, Debug)]
struct MergedRoster {
    settings: RheiSettings,
    sources: RosterSources,
    provenance: RosterProvenance,
    /// Fields supplied by the winning wholesale agent entry. The transport
    /// type alone cannot distinguish an omitted default from an explicit
    /// `false`, empty map, or `null`. §FS-rhei-agents.1.1.7
    agent_fields: BTreeMap<String, BTreeSet<String>>,
}

fn canonical_roster_path(path: &Path, purpose: &str) -> MietteResult<String> {
    // Public source and root paths are resolved paths. §FS-rhei-agents.1.1.7
    fs::canonicalize(path)
        .map(|path| path.display().to_string())
        .map_err(|err| file_io_report(path, purpose, err))
}

fn origin_value(origin: RosterOrigin) -> serde_json::Value {
    // Every provenance leaf uses one of the source keys. §FS-rhei-agents.1.1.7
    serde_json::Value::String(origin.label().to_string())
}

fn origins_value(origins: &BTreeMap<String, RosterOrigin>) -> serde_json::Value {
    // BTree iteration makes field output deterministic. §FS-rhei-agents.1.1.7
    serde_json::Value::Object(
        origins
            .iter()
            .map(|(field, origin)| (field.clone(), origin_value(*origin)))
            .collect(),
    )
}

fn supplied_agent_value(
    profile: &CustomAgentProfile,
    supplied: &BTreeSet<String>,
) -> MietteResult<serde_json::Value> {
    // Agents replace wholesale, including which optional fields exist. §FS-rhei-agents.1.1.7
    let mut value = serde_json::to_value(profile).map_err(|err| {
        miette!(help = internal_error_help(), "failed to serialize an agent profile: {err}")
    })?;
    let object = value.as_object_mut().expect("agent profiles serialize as objects");
    object.retain(|field, _| supplied.contains(field));
    // These two fields have serde omission rules on the execution type. For
    // roster output, explicit empty/null values remain observable.
    if supplied.contains("modes") && !object.contains_key("modes") {
        object.insert("modes".to_string(), serde_json::json!({}));
    }
    if supplied.contains("session") && !object.contains_key("session") {
        object.insert("session".to_string(), serde_json::Value::Null);
    }
    Ok(value)
}

fn optional_value<T: serde::Serialize>(value: &Option<T>) -> serde_json::Value {
    // A recorded origin makes `None` an explicit JSON null. §FS-rhei-agents.1.1.7
    serde_json::to_value(value).expect("settings fields serialize")
}

fn binding_value(
    binding: &ModelAgentBinding,
    origins: &BTreeMap<String, RosterOrigin>,
) -> serde_json::Value {
    // Binding origins are field-specific. §FS-rhei-agents.1.1.7
    let mut object = serde_json::Map::new();
    for field in origins.keys() {
        let value = match field.as_str() {
            "args" => serde_json::json!(binding.args),
            "autonomous_args" => serde_json::json!(binding.autonomous_args),
            "timeout" => optional_value(&binding.timeout),
            _ => continue,
        };
        object.insert(field.clone(), value);
    }
    serde_json::Value::Object(object)
}

fn model_values_and_provenance(
    roster: &MergedRoster,
) -> (serde_json::Value, serde_json::Value) {
    // Values and matching origins are built in one traversal. §FS-rhei-agents.1.1.7
    let mut models = serde_json::Map::new();
    let mut model_provenance = serde_json::Map::new();
    for (id, profile) in &roster.settings.models {
        let origins = roster.provenance.models.get(id).cloned().unwrap_or_default();
        let mut value = serde_json::Map::new();
        for field in origins.fields.keys() {
            let supplied = match field.as_str() {
                "provider" => optional_value(&profile.provider),
                "model" => optional_value(&profile.model),
                "default_agent" => optional_value(&profile.default_agent),
                _ => continue,
            };
            value.insert(field.clone(), supplied);
        }
        let mut bindings = serde_json::Map::new();
        let mut binding_provenance = serde_json::Map::new();
        for (agent_id, binding) in &profile.agents {
            let field_origins = origins.agents.get(agent_id).cloned().unwrap_or_default();
            bindings.insert(agent_id.clone(), binding_value(binding, &field_origins));
            binding_provenance.insert(agent_id.clone(), origins_value(&field_origins));
        }
        value.insert("agents".to_string(), serde_json::Value::Object(bindings));

        let mut provenance = origins_value(&origins.fields)
            .as_object()
            .expect("origin map is an object")
            .clone();
        provenance.insert(
            "agents".to_string(),
            serde_json::Value::Object(binding_provenance),
        );
        models.insert(id.clone(), serde_json::Value::Object(value));
        model_provenance.insert(id.clone(), serde_json::Value::Object(provenance));
    }
    (
        serde_json::Value::Object(models),
        serde_json::Value::Object(model_provenance),
    )
}

fn default_field_value(defaults: &SettingsDefaults, field: &str) -> serde_json::Value {
    // Canonical defaults include only fields with a recorded source. §FS-rhei-agents.1.1.7
    match field {
        "model" => optional_value(&defaults.model),
        "agent" => optional_value(&defaults.agent),
        "agent_mode" => optional_value(&defaults.agent_mode),
        "agent_timeout" => optional_value(&defaults.agent_timeout),
        "program_timeout" => optional_value(&defaults.program_timeout),
        "attempts" => optional_value(&defaults.attempts),
        "mcp_servers" => optional_value(&defaults.mcp_servers),
        "skills" => optional_value(&defaults.skills),
        _ => serde_json::Value::Null,
    }
}

/// Build JSON v1 from the already-merged values and origins. BTree-backed
/// registries keep repeated output byte-stable. §FS-rhei-agents.1.1.7
fn roster_json_value(root: &Path, roster: &MergedRoster) -> MietteResult<serde_json::Value> {
    let global = roster
        .sources
        .global
        .as_deref()
        .map(|path| {
            canonical_roster_path(path, "failed to resolve global settings path")
                .map(|path| serde_json::json!({ "path": path }))
        })
        .transpose()?;
    let project = roster
        .sources
        .project
        .as_ref()
        .map(|(path, home)| {
            canonical_roster_path(path, "failed to resolve project settings path").map(|path| {
                serde_json::json!({
                    "path": path,
                    "home": match home {
                        ProjectSettingsFile::Current => "current",
                        ProjectSettingsFile::Deprecated => "deprecated",
                    }
                })
            })
        })
        .transpose()?;

    let mut agents = serde_json::Map::new();
    for (id, profile) in &roster.settings.agents {
        let supplied = roster.agent_fields.get(id).cloned().unwrap_or_default();
        agents.insert(id.clone(), supplied_agent_value(profile, &supplied)?);
    }
    let mut defaults = serde_json::Map::new();
    for field in roster.provenance.defaults.keys() {
        defaults.insert(field.clone(), default_field_value(&roster.settings.defaults, field));
    }
    let (models, models_provenance) = model_values_and_provenance(roster);

    Ok(serde_json::json!({
        "schema_version": 1,
        "project_root": canonical_roster_path(root, "failed to resolve roster project root")?,
        "sources": {
            "built_in": { "version": env!("CARGO_PKG_VERSION") },
            "global": global,
            "project": project,
        },
        "agents": agents,
        "models": models,
        "defaults": defaults,
        "provenance": {
            "agents": origins_value(&roster.provenance.agents),
            "models": models_provenance,
            "defaults": origins_value(&roster.provenance.defaults),
        }
    }))
}

fn render_roster_text(payload: &serde_json::Value) -> String {
    // Human output follows sources/defaults, agents, then models. §FS-rhei-agents.1.1.7
    let mut out = String::new();
    out.push_str("Sources\n");
    out.push_str(&format!(
        "  built_in: {}\n",
        payload["sources"]["built_in"]["version"].as_str().unwrap_or("")
    ));
    for source in ["global", "project"] {
        let value = &payload["sources"][source];
        if value.is_null() {
            out.push_str(&format!("  {source}: none\n"));
        } else if source == "project" {
            out.push_str(&format!(
                "  project: {} ({})\n",
                value["path"].as_str().unwrap_or(""),
                value["home"].as_str().unwrap_or("")
            ));
        } else {
            out.push_str(&format!("  global: {}\n", value["path"].as_str().unwrap_or("")));
        }
    }

    out.push_str("Defaults\n");
    let defaults = payload["defaults"].as_object().expect("defaults object");
    if defaults.is_empty() {
        out.push_str("  (none)\n");
    }
    for (field, value) in defaults {
        let origin = payload["provenance"]["defaults"][field].as_str().unwrap_or("");
        out.push_str(&format!("  {field}: {value} [{origin}]\n"));
    }

    out.push_str("Agents\n");
    for (id, profile) in payload["agents"].as_object().expect("agents object") {
        let origin = payload["provenance"]["agents"][id].as_str().unwrap_or("");
        let modes = profile
            .get("modes")
            .and_then(serde_json::Value::as_object)
            .map(|modes| modes.keys().cloned().collect::<Vec<_>>().join(", "))
            .filter(|modes| !modes.is_empty())
            .unwrap_or_else(|| "none".to_string());
        out.push_str(&format!("  {id} [{origin}] modes: {modes}\n"));
    }

    out.push_str("Models\n");
    let models = payload["models"].as_object().expect("models object");
    if models.is_empty() {
        out.push_str("  (none)\n");
    }
    for (id, model) in models {
        out.push_str(&format!("  {id}"));
        for field in ["provider", "model", "default_agent"] {
            if let Some(value) = model.get(field) {
                let origin = payload["provenance"]["models"][id][field]
                    .as_str()
                    .unwrap_or("");
                out.push_str(&format!(" {field}={value} [{origin}]"));
            }
        }
        out.push('\n');
        for (agent_id, binding) in model["agents"].as_object().expect("bindings object") {
            out.push_str(&format!("    {agent_id}"));
            for field in ["args", "autonomous_args", "timeout"] {
                if let Some(value) = binding.get(field) {
                    let origin = payload["provenance"]["models"][id]["agents"][agent_id]
                        [field]
                        .as_str()
                        .unwrap_or("");
                    out.push_str(&format!(" {field}={value} [{origin}]"));
                }
            }
            out.push('\n');
        }
    }
    out
}

fn write_roster_stdout(rendered: &str) -> MietteResult<()> {
    // A closed consumer is successful pipeline termination. §FS-rhei-agents.1.1.7
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(rendered.as_bytes()).and_then(|()| stdout.write_all(b"\n")) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(miette!("failed to write roster output: {err}")),
    }
}

/// Accept only the plan shapes shared discovery recognizes, without loading
/// their task bodies. §FS-rhei-agents.1.1.7 §FS-rhei-panta.6
fn roster_project_root(input: Option<PathBuf>) -> MietteResult<PathBuf> {
    let target = resolve_plan_target(input).map_err(|err| {
        miette!(
            help = "pass a plan or project path: rhei roster <RHEI_PLAN>",
            "{err}"
        )
    })?;
    let normalized = normalize_workspace_input(target.path());
    let is_single_file_plan = normalized.is_file()
        && normalized
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".rhei.md"));
    if workspace::panta_project_dir(&normalized).is_some()
        || workspace::workspace_dir(&normalized).is_some()
        || is_single_file_plan
    {
        return Ok(execution_workspace_root(&normalized));
    }
    if normalized.is_dir() {
        return Err(unrecognized_plan_directory_report(&normalized, None)?);
    }
    Err(miette!(
        help = "pass a `*.rhei.md` plan, a Directory Workspace, or a Panta Project: \
                rhei roster <RHEI_PLAN>",
        "'{}' is not a recognized Rhei plan or project",
        normalized.display()
    ))
}

fn warn_deprecated_roster_source(root: &Path, sources: &RosterSources) {
    let Some((read, ProjectSettingsFile::Deprecated)) = sources.project.as_ref() else {
        return;
    };
    // Successful inspection retains the warning, after every fallible
    // settings and output step has completed. §FS-rhei-agents.1.1.7
    warn_deprecated_rhei_home(read, &rhei_home_write_path(root, PROJECT_SETTINGS_FILE));
}

/// Resolve only the project/settings boundary, validate the complete selected
/// documents, then emit one atomic payload. Plan task bodies are never loaded.
/// §FS-rhei-agents.1.1.7 §FS-rhei-panta.6
fn roster_command(input: Option<PathBuf>, json: bool) -> MietteResult<()> {
    let root = roster_project_root(input)?;
    let roster = load_merged_roster(&root, false)?;
    let errors = validate_intrinsic_settings(&roster.settings);
    if !errors.is_empty() {
        return Err(miette!(
            help = settings_help(),
            "invalid merged settings:\n- {}",
            errors.join("\n- ")
        ));
    }
    let payload = roster_json_value(&root, &roster)?;
    let rendered = if json {
        serde_json::to_string_pretty(&payload).map_err(|err| {
            miette!(help = internal_error_help(), "failed to serialize roster JSON: {err}")
        })?
    } else {
        render_roster_text(&payload)
    };
    write_roster_stdout(&rendered)?;
    warn_deprecated_roster_source(&root, &roster.sources);
    Ok(())
}
