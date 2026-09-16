use indexmap::IndexMap;
use regex::Regex;
pub use crate::ast::{CallbackRef, StateName, TransitionRule};
use crate::ast::{Rhei, Structure, Task, TaskId, TaskIdSegment};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

/// Returns the crate version reported by Cargo metadata.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Result of validating a parsed plan.
///
/// Errors indicate invalid input. Warnings indicate accepted input with
/// noteworthy conditions, such as subtasks under named task identifiers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Validation failures that should cause command execution to fail.
    pub errors: Vec<String>,
    /// Non-fatal validation observations.
    pub warnings: Vec<String>,
    /// Guidance that accompanies an error without being part of it.
    ///
    /// Anything an error message would otherwise enumerate from the *whole*
    /// project — its rhei ids, the node kinds and depth limit its rheis merge
    /// into one structure — belongs here. Those lists are the reason a create
    /// elsewhere in the project changes the text of an error it never touched,
    /// and a create decides what it introduced by comparing error strings.
    /// Keeping the volatile half out of the error keeps the error's identity
    /// stable, and the user still reads every word of it.
    // §FS-rhei-new.5.2
    pub help: Vec<String>,
}

impl ValidationReport {
    /// Construct an empty, successful report.
    pub fn ok() -> Self {
        Self { errors: Vec::new(), warnings: Vec::new(), help: Vec::new() }
    }

    /// Returns true if any errors are present.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Merge another report into this one.
    pub fn extend(&mut self, other: ValidationReport) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        self.help.extend(other.help);
    }
}

/// Trait for validating a value into a [`ValidationReport`].
///
/// Implementations can be provided for values that do not require external
/// context. For markdown plan validation against allowed states, prefer
/// [`Validator`] or [`validate_with_machine`].
pub trait Validate {
    /// Validate `self` and collect any errors or warnings.
    fn validate(&self) -> ValidationReport;
}

/// A no-op validator implementation useful for smoke tests.
impl Validate for () {
    fn validate(&self) -> ValidationReport {
        ValidationReport::ok()
    }
}

// ===============================
// States Loader (Task 4)
// ===============================

/// Error returned when loading a [`StateMachine`] from YAML text or a file.
#[derive(Debug)]
pub enum StateMachineLoadError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Invalid(String),
}

impl std::fmt::Display for StateMachineLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateMachineLoadError::Io(e) => write!(f, "I/O error: {e}"),
            StateMachineLoadError::Yaml(e) => write!(f, "YAML error: {e}"),
            StateMachineLoadError::Invalid(message) => {
                write!(f, "invalid state machine: {message}")
            }
        }
    }
}

impl std::error::Error for StateMachineLoadError {}

impl From<std::io::Error> for StateMachineLoadError {
    fn from(e: std::io::Error) -> Self {
        StateMachineLoadError::Io(e)
    }
}

impl From<serde_yaml::Error> for StateMachineLoadError {
    fn from(e: serde_yaml::Error) -> Self {
        StateMachineLoadError::Yaml(e)
    }
}

/// One entry from the `states` map in a YAML states file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateArtifactDef {
    /// Stable identifier for the artifact within a state.
    pub name: String,
    /// Workspace-relative artifact path template.
    pub path: String,
    /// Optional artifact role. `handoff` marks an output artifact as
    /// same-task state handoff prompt context.
    // §FS-rhei-states.3.2: Handoff artifacts are output artifact roles.
    #[serde(default)]
    pub kind: Option<String>,
    /// Optional human-readable description of the artifact.
    #[serde(default)]
    pub description: Option<String>,
    /// When `true`, a missing file does not block state entry.
    /// Only valid on `inputs` entries; declaring `optional: true` on an
    /// `outputs` entry is a validation error.
    #[serde(default)]
    pub optional: bool,
}

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
    /// Base command and fixed arguments.
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

/// Parsed inline execution target selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionTarget {
    /// Agent id that executes the target.
    pub agent: String,
    /// Optional named mode selected on the agent.
    pub mode: Option<String>,
    /// Optional provider segment carried by the selector.
    pub provider: Option<String>,
    /// Model identifier segment carried by the selector.
    pub model: String,
}

impl ExecutionTarget {
    /// §FS-rhei-snapshots.7.1: Normalize execution target selectors for storage.
    ///
    /// Return a filesystem-safe slug for this selector.
    pub fn slug(&self) -> String {
        let mut slug = String::new();
        let mut last_was_dash = false;

        for ch in self.selector().chars() {
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

    /// Reconstruct the normalized selector string.
    pub fn selector(&self) -> String {
        let mut selector = self.agent.clone();
        if let Some(mode) = &self.mode {
            selector.push('[');
            selector.push_str(mode);
            selector.push(']');
        }
        selector.push(':');
        if let Some(provider) = &self.provider {
            selector.push_str(provider);
            selector.push(':');
        }
        selector.push_str(&self.model);
        selector
    }
}

/// A concrete, corrected selector shaped from what the caller typed. Callers own
/// the left-hand side, so this never guesses a `key=` prefix. §FS-rhei-errors.1.2
pub fn execution_target_example(selector: &str) -> String {
    let head = selector.split(':').next().unwrap_or_default().trim();
    let agent = head.split('[').next().unwrap_or_default().trim();
    let agent = if agent.is_empty() { "<agent>" } else { agent };
    format!("{agent}[yolo]:openai:gpt-5.5")
}

/// The accepted selector shapes plus a repair of what the caller typed, quoted
/// because a `[mode]` segment is a zsh glob. §FS-rhei-errors.1.2
fn execution_target_repair(selector: &str) -> String {
    // Unquoted, `codex[yolo]:openai:gpt-5.5` dies with `no matches found`
    // before rhei ever runs.
    let head = selector.split(':').next().unwrap_or_default().trim();
    let head = if head.is_empty() { "<agent>" } else { head };
    format!(
        "write it as '{head}:<model>' or '{head}:<provider>:<model>' \
         (a mode goes in brackets: '{head}[<mode>]:<provider>:<model>'). \
         A selector carrying a mode must be quoted in the shell, e.g. \
         '{}'",
        execution_target_example(selector)
    )
}

/// Parse an inline execution target selector.
pub fn parse_execution_target(selector: &str) -> Result<ExecutionTarget, String> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err(
            "execution target selector must not be empty; write it as '<agent>:<model>', \
             e.g. 'claude-code:claude-opus-4-7'"
                .to_string(),
        );
    }

    let parts: Vec<&str> = selector.split(':').collect();
    if parts.len() != 2 && parts.len() != 3 {
        let problem = if parts.len() == 1 {
            "is missing the model"
        } else {
            "has too many ':'-separated segments"
        };
        return Err(format!(
            "execution target selector '{selector}' {problem}; {}",
            execution_target_repair(selector)
        ));
    }

    let head = parts[0].trim();
    let (provider, model) = if parts.len() == 2 {
        (None, parts[1].trim())
    } else {
        (Some(parts[1].trim()), parts[2].trim())
    };

    if model.is_empty() {
        return Err(format!(
            "execution target selector '{selector}' is missing the model after ':'; {}",
            execution_target_repair(selector)
        ));
    }
    if let Some(provider) = provider {
        if provider.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' is missing the provider between the \
                 agent and the model; {}",
                execution_target_repair(selector)
            ));
        }
    }

    let (agent, mode) = if let Some(open) = head.find('[') {
        if !head.ends_with(']') {
            return Err(format!(
                "execution target selector '{selector}' has an unterminated mode segment; \
                 close the bracket and {}",
                execution_target_repair(selector)
            ));
        }
        let agent = head[..open].trim();
        let mode = head[open + 1..head.len() - 1].trim();
        if agent.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' is missing the agent before '['; {}",
                execution_target_repair(selector)
            ));
        }
        if mode.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' has an empty '[]' mode; name a mode \
                 the agent declares, or drop the brackets entirely"
            ));
        }
        if mode.contains('[') || mode.contains(']') {
            return Err(format!(
                "execution target selector '{selector}' contains nested mode brackets; a mode \
                 is a single bracketed name, e.g. 'claude-code[yolo]:claude-opus-4-7'"
            ));
        }
        (agent, Some(mode))
    } else {
        let agent = head.trim();
        if agent.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' is missing the agent; {}",
                execution_target_repair(selector)
            ));
        }
        if agent.contains(']') {
            return Err(format!(
                "execution target selector '{selector}' contains an unexpected ']'; a mode \
                 must open with '[' too, e.g. 'claude-code[yolo]:claude-opus-4-7'"
            ));
        }
        (agent, None)
    };

    Ok(ExecutionTarget {
        agent: agent.to_string(),
        mode: mode.map(str::to_string),
        provider: provider.map(str::to_string),
        model: model.to_string(),
    })
}
