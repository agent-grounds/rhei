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
    /// A scan of the text as YAML rejects it. §FS-rhei-errors.1.2.2
    Yaml(serde_yaml::Error),
    /// The scan accepts the text, but it does not load as a state machine. §FS-rhei-errors.1.2.2
    Shape {
        /// The deserializer's refusal, before applying the fault's diagnostic wording.
        error: serde_yaml::Error,
        /// What that refusal asks of the file.
        fault: StateMachineShapeFault,
    },
    Invalid(String),
}

/// A top-level key every state machine must carry, as §FS-rhei-states.1.1 lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequiredTopLevelField {
    /// The key as written in the file.
    pub key: &'static str,
    /// The value the key holds, in words a help can quote.
    pub shape: &'static str,
}

/// The required top-level keys, in the order [`StateMachine`] declares them: the
/// order in which its deserializer reports the first one absent.
/// §FS-rhei-states.1.1
pub const REQUIRED_TOP_LEVEL_FIELDS: [RequiredTopLevelField; 3] = [
    RequiredTopLevelField { key: "name", shape: "a string" },
    RequiredTopLevelField { key: "version", shape: "a number or string" },
    RequiredTopLevelField { key: "states", shape: "a map of state name to state definition" },
];

/// What a machine the YAML scan accepts lacks, as far as its help is concerned.
/// §FS-rhei-errors.1.2.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateMachineShapeFault {
    /// The refusal is this required top-level field being absent.
    MissingTopLevelField(RequiredTopLevelField),
    /// The document is empty, only comments, `null`, a scalar or a sequence.
    NoTopLevelMapping,
    /// Anything else: the refusal itself names the key and what is wrong with it.
    Other,
    /// The text holds more than one document, and a machine is one.
    SeveralDocuments,
    /// The text is one document that cannot be read into a YAML value, for a reason
    /// the refusal names: an integer too large, a limit, a duplicate key.
    Unreadable,
}

impl StateMachineShapeFault {
    /// The human-facing cause shared by load-error reporters and Display consumers.
    /// A document without a mapping names the required YAML shape; all other
    /// faults retain the deserializer's cause. §FS-rhei-errors.1.2.2
    pub fn cause(&self, error: &serde_yaml::Error) -> String {
        match self {
            Self::NoTopLevelMapping => "the document must be a mapping".to_owned(),
            _ => error.to_string(),
        }
    }
}

impl std::fmt::Display for StateMachineLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateMachineLoadError::Io(e) => write!(f, "I/O error: {e}"),
            StateMachineLoadError::Yaml(e) => write!(f, "YAML error: {e}"),
            // Not "YAML error": the scan accepts this file. §FS-rhei-errors.1.2.2
            StateMachineLoadError::Shape { error, fault } => {
                write!(f, "invalid state machine: {}", fault.cause(error))
            }
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
    /// Named profile which overrode this otherwise literal target. This is
    /// runtime provenance, not part of the selector's serialized identity.
    /// §FS-rhei-agents.1.4
    #[serde(skip)]
    pub model_profile: Option<String>,
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
        model_profile: None,
    })
}
