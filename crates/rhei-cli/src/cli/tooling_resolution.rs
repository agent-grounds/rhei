
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
    /// Task-named ids a withholding state left out of the set; an id the
    /// state or the defaults supply is never here. §FS-rhei-task-tooling.4
    mcp_withheld: Vec<String>,
    skills_withheld: Vec<String>,
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

/// Compute one invocation's effective tooling: the defaults, then the state's
/// list, then the task's own entries unless the state withholds them.
/// §FS-rhei-task-tooling.3 §FS-rhei-task-tooling.4
fn resolve_tooling(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    task: &TaskTooling,
    settings: &RheiSettings,
) -> ResolvedTooling {
    let state_def = machine.states.get(state_name);
    let withhold = state_def.and_then(|d| d.withhold_task_tooling) == Some(true);

    // MCP: start from defaults (if any), then override/extend with state-level.
    let mcp_entries = effective_mcp_entries(
        settings.defaults.mcp_servers.as_deref().unwrap_or(&[]),
        state_def.and_then(|d| d.mcp_servers.as_deref()),
    );
    let mut mcp_servers: Vec<ResolvedMcpEntry> = mcp_entries
        .into_iter()
        .map(|entry| resolve_mcp_entry(&entry, &settings.mcp_servers))
        .collect();
    let mcp_withheld = add_task_entries(&mut mcp_servers, &task.mcp_servers, withhold, |entry| {
        let object = StateMcpEntryObject {
            id: entry.id.clone(),
            optional: entry.optional,
            ..Default::default()
        };
        resolve_mcp_entry(&StateMcpEntry::Object(object), &settings.mcp_servers)
    });

    let skill_entries = effective_skill_entries(
        settings.defaults.skills.as_deref().unwrap_or(&[]),
        state_def.and_then(|d| d.skills.as_deref()),
    );
    let mut skills: Vec<ResolvedSkillEntry> = skill_entries
        .into_iter()
        .map(|entry| resolve_skill_entry(&entry, &settings.skills))
        .collect();
    let skills_withheld = add_task_entries(&mut skills, &task.skills, withhold, |entry| {
        let object = StateSkillEntryObject {
            id: entry.id.clone(),
            optional: entry.optional,
            ..Default::default()
        };
        resolve_skill_entry(&StateSkillEntry::Object(object), &settings.skills)
    });

    ResolvedTooling { mcp_servers, skills, mcp_withheld, skills_withheld }
}

/// A resolved entry a task's entry can meet in the set it is added to.
trait TaskAddedEntry {
    fn id(&self) -> &str;
    fn optional_mut(&mut self) -> &mut bool;
}

impl TaskAddedEntry for ResolvedMcpEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn optional_mut(&mut self) -> &mut bool {
        &mut self.optional
    }
}

impl TaskAddedEntry for ResolvedSkillEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn optional_mut(&mut self) -> &mut bool {
        &mut self.optional
    }
}

/// Adds a task's entries last and returns the ids a withholding state left
/// out. An id already in the set keeps its definition, and stays optional only
/// if the task's entry is optional too. §FS-rhei-task-tooling.3
fn add_task_entries<E: TaskAddedEntry>(
    set: &mut Vec<E>,
    task: &[TaskToolingEntry],
    withhold: bool,
    resolve: impl Fn(&TaskToolingEntry) -> E,
) -> Vec<String> {
    let mut withheld = Vec::new();
    for entry in task {
        match set.iter_mut().find(|held| held.id() == entry.id) {
            // The state's own entry resolves as if the task named nothing.
            Some(_) if withhold => {}
            Some(held) => *held.optional_mut() &= entry.optional,
            None if withhold => withheld.push(entry.id.clone()),
            None => set.push(resolve(entry)),
        }
    }
    withheld
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
    /// The built-in this invocation behaves as: the profile's declared
    /// `family`, or its own id when it declares none, so a built-in agent is
    /// its own family. Every behavioral branch that used to read the id reads
    /// this; everything that *names* the agent keeps reading `agent`.
    /// §FS-rhei-agents.1.1.2
    fn family(&self) -> &str {
        resolved_agent_family(&self.profile, self.agent.id())
    }

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
