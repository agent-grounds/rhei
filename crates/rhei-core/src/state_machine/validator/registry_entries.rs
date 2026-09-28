// The `settings.json` registry vocabulary: what an agent, an MCP server, or a
// skill entry is, and the two state-level entries that may inline one.
//
// Their own part because they are the *authored* surface a site writes,
// validated by the CLI that loads them rather than by the state-machine checks
// around them.

// §AR-source-file-size.3 §FS-rhei-agents.1.1.2

/// Reference to an agent defined in the `agents` registry.
///
/// In `states.yaml` (`agent:`) and `settings.json` (`defaults.agent`), agents
/// are always referenced by string id. The concrete transport profile lives
/// in the `agents` registry in `settings.json` (global or project). See ADR
/// 0003 for the rationale.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AgentConfig(pub String);

impl AgentConfig {
    /// Return the agent identifier.
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl From<String> for AgentConfig {
    fn from(id: String) -> Self {
        AgentConfig(id)
    }
}

impl From<&str> for AgentConfig {
    fn from(id: &str) -> Self {
        AgentConfig(id.to_string())
    }
}

/// Agent transport profile. An entry in the `agents` registry in
/// `settings.json` (global or project), or the value of a built-in profile.
///
/// The registry key is the agent id; the id is not repeated inside this
/// value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct CustomAgentProfile {
    /// The id of the built-in agent this profile belongs to. Every field the
    /// entry does not write is taken from that built-in, and every behavior
    /// Rhei selects per built-in agent is selected on the resolved family —
    /// the declared value, or the profile's own id when it declares none.
    // §FS-rhei-agents.1.1.2: A profile may name the built-in family it belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Base command and fixed arguments. Required unless `family` supplies it.
    // §FS-rhei-agents.1.1.2: `command` is required only when `family` is absent.
    #[serde(default)]
    pub command: Vec<String>,
    /// Flag to pass the prompt (e.g., `"--prompt"`, `"-p"`). Omit if using stdin.
    #[serde(default)]
    pub prompt_flag: Option<String>,
    /// Flag to pass the model. Omit if the agent doesn't support model selection.
    #[serde(default)]
    pub model_flag: Option<String>,
    /// When `true`, the prompt is piped to stdin instead of passed via flag.
    #[serde(default)]
    pub stdin_prompt: bool,
    /// When `true`, stdin is held open after the initial prompt so the live
    /// dashboard can deliver intervention messages. Only valid for agents that
    /// start work without waiting for stdin EOF.
    #[serde(default)]
    pub intervene_stdin: bool,
    /// Default timeout for this agent (e.g., `"30m"`).
    #[serde(default)]
    pub timeout: Option<String>,
    /// Flag used to attach one MCP server per occurrence (e.g., `"--mcp"`).
    /// Mutually exclusive with `mcp_config_flag`.
    #[serde(default)]
    pub mcp_flag: Option<String>,
    /// Flag used to attach a generated MCP config file (e.g., `"--mcp-config"`).
    /// Mutually exclusive with `mcp_flag`.
    #[serde(default)]
    pub mcp_config_flag: Option<String>,
    /// Flag used to enable one skill per occurrence (e.g., `"--skill"`).
    /// Omit to declare the agent does not support skills.
    #[serde(default)]
    pub skill_flag: Option<String>,
    /// Optional adapter that accepts one repeated absolute path flag and owns
    /// denial for the spawned process tree. §FS-rhei-agents.1.1.2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_read: Option<DenyReadAdapter>,
    /// Named modes. Each mode is an ordered flag list appended to the
    /// command at spawn time. A well-known mode name is `yolo`, but any
    /// name is allowed — Rhei does not interpret mode names.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub modes: IndexMap<String, Vec<String>>,
    /// Optional native translation for the portable state effort vocabulary.
    // §FS-rhei-agents.1.1.2: Agent profiles own effort capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<AgentEffortProfile>,
    /// Optional snapshot session block describing resume / fork / interactive
    /// continuation support and transcript layout for the agent. The schema
    /// is authoritative for `CustomAgentProfile.session`; the field is retained here so settings
    /// round-trip without rejecting unknown keys, and so the snapshot module
    /// can inspect it at runtime without re-parsing the file.
    // §FS-rhei-snapshots.9.1: CustomAgentProfile.session schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DenyReadAdapter {
    pub path_flag: String,
}

/// Registry entry for an MCP server profile.
///
/// An entry must declare exactly one of `command` (local subprocess) or `url`
/// (remote transport). This is enforced at load time by the profile validator.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct McpServerProfile {
    /// Command and arguments to launch a local MCP server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// URL of a remote MCP server. Requires `transport`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Transport for remote servers (`"sse"`, `"websocket"`). Ignored for command-based servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Environment variables for the server process. Values may reference host env via `${VAR}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Working directory for the server process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Maximum time to wait for the server's MCP handshake (e.g., `"10s"`). Default: `"10s"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<String>,
}

/// Registry entry for a skill profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SkillProfile {
    /// Filesystem path to the skill bundle. Leading `~` expands to the user's home directory.
    pub path: String,
    /// Human-readable description of the skill's purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One entry in a state-level `mcp_servers` list or in `defaults.mcp_servers`.
///
/// Strings are registry ids with `optional: false`; objects allow `optional: true`
/// and inline definitions that do not require a registry entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StateMcpEntry {
    /// Shorthand for `{ id: "<name>" }` with `optional: false`.
    Id(String),
    /// Full object form with optional inline definition fields.
    Object(StateMcpEntryObject),
}

/// Object form of a state-level MCP entry.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct StateMcpEntryObject {
    /// Stable identifier. Must match a registry entry unless inline fields are provided.
    pub id: String,
    /// When `true`, a missing server does not block agent spawn.
    #[serde(default)]
    pub optional: bool,
    /// Inline command form (mutually exclusive with `url`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Inline url form (mutually exclusive with `command`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<String>,
}

impl StateMcpEntry {
    /// Registry id for this entry (always present).
    pub fn id(&self) -> &str {
        match self {
            StateMcpEntry::Id(id) => id,
            StateMcpEntry::Object(obj) => &obj.id,
        }
    }

    /// Whether this entry is marked optional.
    pub fn is_optional(&self) -> bool {
        match self {
            StateMcpEntry::Id(_) => false,
            StateMcpEntry::Object(obj) => obj.optional,
        }
    }

    /// Whether this entry carries an inline definition (rather than referring to a registry id).
    pub fn is_inline(&self) -> bool {
        match self {
            StateMcpEntry::Id(_) => false,
            StateMcpEntry::Object(obj) => obj.command.is_some() || obj.url.is_some(),
        }
    }
}

/// One entry in a state-level `skills` list or in `defaults.skills`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StateSkillEntry {
    /// Shorthand for `{ id: "<name>" }` with `optional: false`.
    Id(String),
    /// Full object form with optional inline definition fields.
    Object(StateSkillEntryObject),
}

/// Object form of a state-level skill entry.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct StateSkillEntryObject {
    pub id: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl StateSkillEntry {
    pub fn id(&self) -> &str {
        match self {
            StateSkillEntry::Id(id) => id,
            StateSkillEntry::Object(obj) => &obj.id,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            StateSkillEntry::Id(_) => false,
            StateSkillEntry::Object(obj) => obj.optional,
        }
    }

    pub fn is_inline(&self) -> bool {
        match self {
            StateSkillEntry::Id(_) => false,
            StateSkillEntry::Object(obj) => obj.path.is_some(),
        }
    }
}
