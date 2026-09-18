
/// One fully-resolved MCP server entry in a state's effective set.
///
/// `definition` is `Some` when the entry resolves against the merged registry
/// or carries inline fields, and `None` only when the id is unknown — callers
/// treat the latter as a validation error.
#[derive(Debug, Clone)]
struct ResolvedMcpEntry {
    id: String,
    /// `optional: true` on the declaring entry. Used by Half B to decide
    /// whether a failed availability check blocks the agent or is downgraded
    /// to a warning. Carried in Half A so the resolution path is complete.
    #[allow(dead_code)]
    optional: bool,
    definition: Option<McpServerProfile>,
}

/// One fully-resolved skill entry in a state's effective set.
#[derive(Debug, Clone)]
struct ResolvedSkillEntry {
    id: String,
    #[allow(dead_code)]
    optional: bool,
    definition: Option<SkillProfile>,
}

/// The tooling a state contributes to the agent subprocess.
///
/// Half A: availability is computed from registry resolution only — an entry
/// whose id resolves (or carries an inline definition) is reported available.
/// Half B will hook actual MCP handshake checks and skill-path probes into
/// the same struct, leaving call sites unchanged.
#[derive(Debug, Clone, Default)]
struct ResolvedTooling {
    mcp_servers: Vec<ResolvedMcpEntry>,
    skills: Vec<ResolvedSkillEntry>,
}

impl ResolvedTooling {
    /// Ids whose definition resolved — used for `{mcp.<name>.available}` and
    /// the `RHEI_MCP_<NAME>_AVAILABLE` env vars.
    fn mcp_available(&self, id: &str) -> bool {
        self.mcp_servers.iter().any(|e| e.id == id && e.definition.is_some())
    }

    fn skill_available(&self, id: &str) -> bool {
        self.skills.iter().any(|e| e.id == id && e.definition.is_some())
    }

    /// Comma-separated ids of resolved MCP servers (available only).
    fn mcp_servers_csv(&self) -> String {
        self.mcp_servers
            .iter()
            .filter(|e| e.definition.is_some())
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn skills_csv(&self) -> String {
        self.skills
            .iter()
            .filter(|e| e.definition.is_some())
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Normalize an id into the env-var segment used by `RHEI_*_<NAME>_AVAILABLE`.
fn env_id_segment(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect()
}

fn slugify_target_value(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if matches!(ch, '.' | '_' | '-') {
            slug.push(ch);
            last_was_dash = ch == '-';
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Compute the effective tooling set for a state given the merged settings.
fn resolve_tooling(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
) -> ResolvedTooling {
    let state_def = machine.states.get(state_name);

    // MCP: start from defaults (if any), then override/extend with state-level.
    let mcp_entries = effective_mcp_entries(
        settings.defaults.mcp_servers.as_deref().unwrap_or(&[]),
        state_def.and_then(|d| d.mcp_servers.as_deref()),
    );
    let mcp_servers: Vec<ResolvedMcpEntry> = mcp_entries
        .into_iter()
        .map(|entry| resolve_mcp_entry(&entry, &settings.mcp_servers))
        .collect();

    let skill_entries = effective_skill_entries(
        settings.defaults.skills.as_deref().unwrap_or(&[]),
        state_def.and_then(|d| d.skills.as_deref()),
    );
    let skills: Vec<ResolvedSkillEntry> = skill_entries
        .into_iter()
        .map(|entry| resolve_skill_entry(&entry, &settings.skills))
        .collect();

    ResolvedTooling { mcp_servers, skills }
}

/// Union `defaults.mcp_servers` with a state's `mcp_servers`, deduped by id.
///
/// `None` on the state = inherit defaults. `Some(empty)` = clear defaults.
/// `Some(non-empty)` = append/override defaults by id (state wins).
fn effective_mcp_entries(
    defaults: &[StateMcpEntry],
    state: Option<&[StateMcpEntry]>,
) -> Vec<StateMcpEntry> {
    match state {
        None => defaults.to_vec(),
        Some([]) => Vec::new(),
        Some(list) => {
            let mut out: Vec<StateMcpEntry> = defaults.to_vec();
            for entry in list {
                if let Some(pos) = out.iter().position(|e| e.id() == entry.id()) {
                    out[pos] = entry.clone();
                } else {
                    out.push(entry.clone());
                }
            }
            out
        }
    }
}

fn effective_skill_entries(
    defaults: &[StateSkillEntry],
    state: Option<&[StateSkillEntry]>,
) -> Vec<StateSkillEntry> {
    match state {
        None => defaults.to_vec(),
        Some([]) => Vec::new(),
        Some(list) => {
            let mut out: Vec<StateSkillEntry> = defaults.to_vec();
            for entry in list {
                if let Some(pos) = out.iter().position(|e| e.id() == entry.id()) {
                    out[pos] = entry.clone();
                } else {
                    out.push(entry.clone());
                }
            }
            out
        }
    }
}

/// Resolve one entry against the registry. Inline definitions on the entry
/// take precedence over registry lookups.
fn resolve_mcp_entry(
    entry: &StateMcpEntry,
    registry: &BTreeMap<String, McpServerProfile>,
) -> ResolvedMcpEntry {
    let id = entry.id().to_string();
    let optional = entry.is_optional();
    let inline = match entry {
        StateMcpEntry::Object(obj) if obj.command.is_some() || obj.url.is_some() => {
            Some(inline_mcp_profile(obj))
        }
        _ => None,
    };
    let definition = inline.or_else(|| registry.get(&id).cloned());
    ResolvedMcpEntry { id, optional, definition }
}

fn resolve_skill_entry(
    entry: &StateSkillEntry,
    registry: &BTreeMap<String, SkillProfile>,
) -> ResolvedSkillEntry {
    let id = entry.id().to_string();
    let optional = entry.is_optional();
    let inline = match entry {
        StateSkillEntry::Object(obj) if obj.path.is_some() => Some(SkillProfile {
            path: obj.path.clone().unwrap_or_default(),
            description: obj.description.clone(),
        }),
        _ => None,
    };
    let mut definition = inline.or_else(|| registry.get(&id).cloned());
    if let Some(def) = definition.as_mut() {
        // Expand leading `~` so subsequent existence checks and on-disk
        // probes see the absolute path. The expansion happens once, here.
        def.path = expand_home(&def.path);
        // Best-effort spawn-time availability check: a skill bundle is
        // available only when its path exists. When it does not, drop the
        // definition so `available` is reported `false` to env vars and
        // the `?` suffix appears in the log header. Required-vs-optional
        // escalation lives further up the run loop (deferred Half B).
        let path = Path::new(&def.path);
        if !path.exists() {
            definition = None;
        }
    }
    ResolvedSkillEntry { id, optional, definition }
}

/// Expand a leading `~` (or `~/`) into the current user's home directory.
/// Unchanged when no home is set or when the input does not start with `~`.
fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest).display().to_string();
        }
    } else if path == "~" {
        if let Ok(home) = home_dir() {
            return home.display().to_string();
        }
    }
    path.to_string()
}

fn inline_mcp_profile(obj: &StateMcpEntryObject) -> McpServerProfile {
    McpServerProfile {
        command: obj.command.clone(),
        url: obj.url.clone(),
        transport: obj.transport.clone(),
        env: obj.env.clone(),
        working_directory: obj.working_directory.clone(),
        startup_timeout: obj.startup_timeout.clone(),
    }
}

/// Resolved agent and model for a specific task invocation.
#[derive(Clone)]
struct ResolvedAgent {
    /// Agent id (key into the merged `agents` registry).
    agent: AgentConfig,
    /// The registry-resolved transport profile for `agent`.
    profile: CustomAgentProfile,
    /// Resolved mode name, or `None` if the agent has no modes or none was
    /// selected.
    mode: Option<String>,
    /// Inline execution target selector, when the state resolves via `target`
    /// or `all_targets`.
    target: Option<ExecutionTarget>,
    /// Resolved model profile id (the key into `models` if the registry knows
    /// it, otherwise the literal string from settings or state). This is what
    /// appears in logs, in `RHEI_MODEL`, and in template variables.
    model: Option<String>,
    /// Resolved provider id (e.g. `anthropic`, `openai`). Comes from the
    /// `models` registry, or from an `ExecutionTarget` when one is in play.
    model_provider: Option<String>,
    /// Resolved concrete provider model name (e.g. `claude-sonnet-4-6`).
    /// This is what gets passed to the agent's `model_flag` and exposed as
    /// `RHEI_MODEL_NAME`. Falls back to the model id when the registry has
    /// no entry.
    model_name: Option<String>,
    timeout_secs: Option<u64>,
    /// `models.<id>.agents.<agent-id>.autonomous_args` — ordered flag list
    /// appended after the mode flags when `rhei run` launches the agent
    /// §FS-rhei-agents.1.1.3 §FS-rhei-agents.2.2: Autonomous model-agent flags.
    autonomous_args: Vec<String>,
}

impl ResolvedAgent {
    /// The selected named profile, kept distinct from a literal target. For a
    /// named model over a target, `model` is the profile id while the target
    /// carries the final concrete model. §FS-rhei-agents.1.4
    fn model_profile_id(&self) -> Option<&str> {
        match self.target.as_ref() {
            Some(target) => target.model_profile.as_deref(),
            None => self.model.as_deref(),
        }
    }
}

#[derive(Clone)]
enum ProgramCommand {
    Shell(String),
    Exec(Vec<String>),
}

#[derive(Clone)]
struct ProgramSpec {
    command: ProgramCommand,
    env: BTreeMap<String, String>,
    working_directory: Option<String>,
    shell: bool,
}

#[derive(Clone)]
struct ResolvedProgram {
    program: ProgramSpec,
    timeout_secs: Option<u64>,
}
