
/// Which of rhei's two homes the project settings file came from.
///
/// Carried on the merged settings because the messages that tell an author
/// where to define an agent, a model, or a default hold the settings and not
/// always a plan root, and naming the write path to a project still on the
/// deprecated home sends them to a file that shadows the one rhei just read.
/// §FS-rhei-agents.1.1
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ProjectSettingsFile {
    /// `.agent-grounds/rhei/settings.json` — where rhei writes, and the answer
    /// when no project settings file was read at all.
    #[default]
    Current,
    /// The deprecated `.agents/rhei/settings.json`, read because the current
    /// home had none. §FS-rhei-templates.1.1
    Deprecated,
}

impl ProjectSettingsFile {
    /// The path, relative to the plan root, that a message names.
    /// §FS-rhei-agents.1.1
    fn relative_path(self) -> &'static str {
        match self {
            ProjectSettingsFile::Current => PROJECT_SETTINGS_RELATIVE_PATH,
            ProjectSettingsFile::Deprecated => DEPRECATED_PROJECT_SETTINGS_RELATIVE_PATH,
        }
    }
}

/// Rhei settings loaded from `~/.config/rhei/settings.json` or `.agent-grounds/rhei/settings.json`.
#[derive(Debug, Default, Deserialize, Clone)]
struct RheiSettings {
    /// The project settings file this registry was merged from, so a message
    /// naming one names the file rhei read. §FS-rhei-agents.1.1
    #[serde(skip)]
    project_settings_file: ProjectSettingsFile,
    #[serde(default)]
    agent: Option<AgentConfig>,
    #[serde(default)]
    agent_mode: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    agent_timeout: Option<String>,
    #[serde(default)]
    program_timeout: Option<String>,
    /// Spec-aligned nested defaults. The `defaults.{model, agent,
    /// agent_mode, agent_timeout, program_timeout, mcp_servers, skills}` keys
    /// are the canonical settings shape. The top-level `agent` / `model` /
    /// `agent_timeout` / `program_timeout` / `agent_mode` fields above remain
    /// readable for backward compatibility.
    // §FS-rhei-agents.1.1.1: Nested settings defaults.
    #[serde(default)]
    defaults: SettingsDefaults,
    /// Registry of agent transport profiles keyed by agent id.
    #[serde(default)]
    agents: BTreeMap<String, CustomAgentProfile>,
    /// §FS-rhei-agents.1.1.3: Registry of model profiles keyed by model id.
    #[serde(default)]
    models: BTreeMap<String, ModelProfile>,
    /// Registry of MCP server profiles keyed by server id.
    #[serde(default)]
    mcp_servers: BTreeMap<String, McpServerProfile>,
    /// Registry of skill profiles keyed by skill id.
    #[serde(default)]
    skills: BTreeMap<String, SkillProfile>,
    /// Top-level snapshots block. The field is retained verbatim from
    /// settings so the snapshot subsystem (impl-rhei-snapshots) can read its
    /// configured `cache_dir`, `redactor`, and adapter gates without
    /// reparsing the file.
    // §FS-rhei-agents.1.1.6 §FS-rhei-snapshot-operations.4: Snapshot settings.
    #[serde(default)]
    snapshots: Option<SnapshotSettings>,
}

/// Top-level `snapshots` settings block.
///
/// `cache_dir` defaults to `.rhei/cache/snapshots` under the plan workspace;
/// fields omitted here inherit from global settings before defaults are
/// applied.
// §FS-rhei-snapshot-operations.4.1 §FS-rhei-snapshot-operations.4.2: Settings block.
#[derive(Debug, Default, Deserialize, Clone)]
struct SnapshotSettings {
    #[serde(default)]
    cache_dir: Option<PathBuf>,
    #[serde(default)]
    experimental: Option<serde_json::Value>,
    #[serde(default)]
    provider_cache_ttl: BTreeMap<String, String>,
    #[serde(default)]
    redactor: Option<PathBuf>,
    /// Optional allow-list for redactor environment forwarding. The v1 hook
    /// keeps the parent environment closed by default per §4.2.
    #[serde(default)]
    redactor_env: Vec<String>,
}

fn merge_snapshot_settings(
    global: Option<SnapshotSettings>,
    project: Option<SnapshotSettings>,
) -> Option<SnapshotSettings> {
    match (global, project) {
        (None, None) => None,
        (Some(settings), None) | (None, Some(settings)) => Some(settings),
        (Some(mut global), Some(project)) => {
            if project.cache_dir.is_some() {
                global.cache_dir = project.cache_dir;
            }
            if project.experimental.is_some() {
                global.experimental = project.experimental;
            }
            for (provider, ttl) in project.provider_cache_ttl {
                global.provider_cache_ttl.insert(provider, ttl);
            }
            if project.redactor.is_some() {
                global.redactor = project.redactor;
            }
            if !project.redactor_env.is_empty() {
                global.redactor_env = project.redactor_env;
            }
            Some(global)
        }
    }
}

fn snapshot_cache_dir(settings: &RheiSettings, workspace_root: &Path) -> PathBuf {
    let configured = settings
        .snapshots
        .as_ref()
        .and_then(|snapshots| snapshots.cache_dir.clone())
        .unwrap_or_else(|| PathBuf::from(".rhei/cache/snapshots"));
    if configured.is_absolute() {
        configured
    } else {
        workspace_root.join(configured)
    }
}

/// §FS-rhei-agents.1.1.3: One entry in the merged `models` registry.
#[derive(Debug, Default, Deserialize, Clone)]
struct ModelProfile {
    /// Provider identifier such as `anthropic` or `openai`.
    #[serde(default)]
    provider: Option<String>,
    /// Concrete provider model name (`claude-sonnet-4-6`, `o3`, ...). Passed
    /// to the agent's `model_flag` when present.
    #[serde(default)]
    model: Option<String>,
    /// Preferred agent id when `rhei run` needs to spawn this model
    /// autonomously and no other level configured one.
    #[serde(default)]
    default_agent: Option<String>,
    /// Per-agent launch overrides for this model, keyed by agent id.
    #[serde(default)]
    agents: BTreeMap<String, ModelAgentBinding>,
    /// Optional static accounting rates selected with this named profile.
    /// §FS-rhei-agents.1.1.3
    #[serde(default)]
    prices: Option<ModelProfilePrices>,
}

/// One whole `models.<id>.prices` object. The known fields feed the standard
/// price-book shape; every other permitted value remains entry metadata.
/// §FS-rhei-agents.1.1.3
#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
struct ModelProfilePrices {
    currency: String,
    effective_at: String,
    input_total_micro: u64,
    input_cached_read_micro: u64,
    input_cache_write_micro: u64,
    output_total_micro: u64,
    #[serde(default, flatten)]
    extensions: BTreeMap<String, serde_json::Value>,
}

/// One `models.<id>.agents.<agent>` binding. Only `timeout` is consumed by
/// `rhei run` today; `args` and `autonomous_args` are accepted by the parser
/// for forward compatibility.
#[derive(Debug, Default, Deserialize, Clone)]
struct ModelAgentBinding {
    #[serde(default)]
    #[allow(dead_code)]
    args: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    autonomous_args: Vec<String>,
    #[serde(default)]
    timeout: Option<String>,
}

/// Nested `defaults` section in settings.
///
/// `mcp_servers` and `skills` use `Option<Vec<_>>` so the merge layer can
/// distinguish "unset" (inherit) from "empty" (explicitly clear inherited).
#[derive(Debug, Default, Deserialize, Clone)]
struct SettingsDefaults {
    /// Finite live threshold; qualification supplies the additional residual.
    /// §FS-rhei-budgets.2.1
    #[serde(default)]
    budget_threshold: Option<rhei_core::budget::Money>,
    /// §FS-rhei-agents.1.1.1: Default model profile id.
    #[serde(default)]
    model: Option<String>,
    /// Default agent id resolved against the `agents` registry. The spec
    /// requires a bare string id — inline agent objects are rejected by
    /// `AgentConfig`'s transparent deserialisation, which surfaces a JSON
    /// type error.
    #[serde(default)]
    agent: Option<AgentConfig>,
    /// Default agent mode applied when a state does not set `agent_mode`.
    /// `null` explicitly clears an inherited default.
    #[serde(default)]
    agent_mode: Option<String>,
    #[serde(default)]
    agent_timeout: Option<String>,
    /// §FS-rhei-agents.1.1.1: Default program timeout.
    #[serde(default)]
    program_timeout: Option<String>,
    /// Default per-visit attempt budget for states that do not set `attempts:`.
    // §FS-rhei-agents.3.2.3: the resolution chain this is the second level of.
    #[serde(default)]
    attempts: Option<u32>,
    #[serde(default)]
    mcp_servers: Option<Vec<StateMcpEntry>>,
    #[serde(default)]
    skills: Option<Vec<StateSkillEntry>>,
}

/// Built-in agent registry.
///
/// Each entry is a ready-to-use `CustomAgentProfile` for one of the agents
/// that Rhei supports out of the box. The per-agent "autonomous" flag set
/// that was hard-coded as `default_args` is now exposed as a named `yolo`
/// mode so states and defaults can select it explicitly via `agent_mode`.
///
/// A user-written entry with the same id in global or project settings
/// replaces the built-in entry wholesale (see `load_merged_settings`).
fn built_in_agents() -> BTreeMap<String, CustomAgentProfile> {
    fn flags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    let modes_yolo_only = |yolo: Vec<String>| {
        let mut modes = IndexMap::new();
        modes.insert("yolo".to_string(), yolo);
        modes
    };

    // §FS-rhei-agents.1.1.2: Built-in profiles expose the approved native mappings.
    let effort = |values: &[&str], args: &[&str]| rhei_validator::AgentEffortProfile {
        values: values.iter().map(|value| ((*value).to_string(), (*value).to_string())).collect(),
        args: flags(args),
        conflicts: Vec::new(),
    };

    let mut agents = BTreeMap::new();

    // claude-code: the prompt travels on stdin, so a brief is not bounded by one
    // argument. `-p` stays declared and bare — it is `--print`, not the text.
    // §FS-rhei-agents.2 §FS-rhei-agents.1.1.2: Built-in claude-code profile.
    agents.insert(
        "claude-code".to_string(),
        CustomAgentProfile {
            command: flags(&["claude"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            mcp_config_flag: Some("--mcp-config".to_string()),
            skill_flag: Some("--skill".to_string()),
            modes: modes_yolo_only(flags(&["--permission-mode", "bypassPermissions"])),
            effort: Some(effort(
                &["low", "medium", "high", "xhigh", "max"],
                &["--effort", "{value}"],
            )),
            ..Default::default()
        },
    );

    // codex: `codex exec` is non-interactive. The `yolo` mode mirrors the
    // known-agent profile table: `--sandbox danger-full-access --skip-git-repo-check
    // -c approval_policy="never"`. `-c approval_policy="never"` replaced the
    // older `-a never` short flag, which codex-cli no longer accepts.

    // Its session block is the §FS-rhei-snapshots.9.2 row: `resume` as a
    // `codex exec` subcommand and no `fork` (§FS-rhei-snapshots.9.3.4),
    // `--ephemeral` off, no `session_dir_flag`, so §9.1.1 finds the rollout.

    // Interactive continuation is the top-level `codex`, not `codex exec`.
    // §FS-rhei-agents.2: Built-in codex profile.
    agents.insert(
        "codex".to_string(),
        CustomAgentProfile {
            command: flags(&["codex", "exec"]),
            prompt_flag: None,
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            mcp_flag: Some("--mcp".to_string()),
            modes: modes_yolo_only(flags(&[
                "--sandbox",
                "danger-full-access",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"never\"",
            ])),
            effort: Some(effort(
                &["minimal", "low", "medium", "high", "xhigh"],
                &["-c", "model_reasoning_effort=\"{value}\""],
            )),
            session: Some(serde_json::json!({
                "resume": {"flag": "resume"},
                "interactive": {"command": ["codex"]},
                "no_session_flag": "--ephemeral",
                "layout": {
                    "kind": "FlatById",
                    "dir_template": "~/.codex/sessions",
                    "ext": "jsonl",
                    "nested": true,
                    "id_from_stem": "trailing_uuid",
                    "confirm_cwd_path": ["payload", "cwd"]
                }
            })),
            ..Default::default()
        },
    );

    // gemini: `--approval-mode yolo` is the autonomous posture
    // (`auto_edit` still prompts on shell tool calls).
    agents.insert(
        "gemini".to_string(),
        CustomAgentProfile {
            command: flags(&["gemini"]),
            prompt_flag: Some("--prompt".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--approval-mode", "yolo"])),
            ..Default::default()
        },
    );

    // kilocode: `kilo --auto "<prompt>"` is the documented CI invocation;
    // `--yolo` auto-approves tool permissions. `--auto` takes the prompt
    // as its argument, so it maps onto `prompt_flag`.
    agents.insert(
        "kilocode".to_string(),
        CustomAgentProfile {
            command: flags(&["kilo"]),
            prompt_flag: Some("--auto".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--yolo"])),
            effort: Some(effort(
                &["minimal", "low", "high", "max"],
                &["--variant", "{value}"],
            )),
            ..Default::default()
        },
    );

    // cursor: the headless binary is `cursor-agent` (distinct from the
    // `cursor` IDE launcher). `-p`/`--print` is the non-interactive flag;
    // `--force` is the auto-approve posture.
    agents.insert(
        "cursor".to_string(),
        CustomAgentProfile {
            command: flags(&["cursor-agent"]),
            prompt_flag: Some("--print".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--force"])),
            ..Default::default()
        },
    );

    // pi (openclaw / badlogic pi-coding-agent). Headless mode exits
    // deterministically after one turn. pi has no permission layer — modes
    // are intentionally empty; isolation is the caller's responsibility
    // (e.g. sandbox/container).

    // §FS-rhei-agents.2: Built-in pi profile.
    agents.insert(
        "pi".to_string(),
        CustomAgentProfile {
            command: flags(&["pi"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            skill_flag: Some("--skill".to_string()),
            effort: Some(effort(
                &["off", "minimal", "low", "medium", "high", "xhigh"],
                &["--thinking", "{value}"],
            )),
            session: Some(serde_json::json!({
                "resume": {"flag": "--continue"},
                "fork": {"flag": "--fork"},
                "interactive": {},
                "session_dir_flag": "--session-dir",
                "no_session_flag": "--no-session",
                "layout": {"kind": "FlatById", "ext": "jsonl"}
            })),
            ..Default::default()
        },
    );

    agents
}

#[derive(Debug, Clone)]
struct SettingsDocument {
    raw: serde_json::Value,
    typed: RheiSettings,
    /// The selected file when one was actually read. Roster inspection uses
    /// this fact instead of guessing from an empty parsed object.
    /// §FS-rhei-agents.1.1.7
    source_path: Option<PathBuf>,
}

fn empty_settings_document() -> SettingsDocument {
    SettingsDocument {
        raw: serde_json::Value::Object(serde_json::Map::new()),
        typed: RheiSettings::default(),
        source_path: None,
    }
}

fn load_settings_document(path: &Path) -> MietteResult<SettingsDocument> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_settings_document());
        }
        Err(err) => {
            return Err(miette!(
                help = settings_help(),
                "failed to read settings '{}': {err}", path.display()
            ));
        }
    };
    let raw: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|err| miette!(
            help = settings_help(),
            "failed to parse settings '{}': {err}", path.display()
        ))?;
    validate_profile_prices_document(path, &raw)?;
    let typed: RheiSettings = serde_json::from_value(raw.clone())
        .map_err(|err| miette!(
            help = settings_help(),
            "failed to decode settings '{}': {err}", path.display()
        ))?;
    Ok(SettingsDocument { raw, typed, source_path: Some(path.to_path_buf()) })
}

/// Validate profile rates before typed decoding so every failure can name the
/// complete author-facing settings path. §FS-rhei-agents.1.1.3
fn validate_profile_prices_document(path: &Path, raw: &serde_json::Value) -> MietteResult<()> {
    const REQUIRED_STRINGS: [&str; 2] = ["currency", "effective_at"];
    const REQUIRED_RATES: [&str; 4] = [
        "input_total_micro",
        "input_cached_read_micro",
        "input_cache_write_micro",
        "output_total_micro",
    ];
    const RESERVED: [&str; 5] = ["provider", "model", "unit", "source", "model_profiles"];

    let Some(models) = raw.get("models").and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    for (id, profile) in models {
        let Some(prices) = profile.get("prices") else { continue };
        if prices.is_null() {
            continue;
        }
        let Some(object) = prices.as_object() else {
            return Err(invalid_profile_price(path, &format!("models.{id}.prices"), "must be an object or null"));
        };
        for field in REQUIRED_STRINGS {
            let field_path = format!("models.{id}.prices.{field}");
            if object
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
            {
                return Err(invalid_profile_price(path, &field_path, "must be a non-empty string"));
            }
        }
        for field in REQUIRED_RATES {
            let field_path = format!("models.{id}.prices.{field}");
            if object.get(field).and_then(serde_json::Value::as_u64).is_none() {
                return Err(invalid_profile_price(
                    path,
                    &field_path,
                    "must be a non-negative integer no greater than u64::MAX",
                ));
            }
        }
        for field in RESERVED {
            if object.contains_key(field) {
                return Err(invalid_profile_price(
                    path,
                    &format!("models.{id}.prices.{field}"),
                    "is reserved for generated price-book provenance",
                ));
            }
        }
    }
    Ok(())
}

fn invalid_profile_price(path: &Path, field: &str, problem: &str) -> miette::Report {
    miette!(
        help = "provide currency, effective_at, and all four integer micro-currency rates; omit reserved generated-entry keys",
        "invalid profile price in settings '{}': {field} {problem}", path.display()
    )
}
