//! Abstract Syntax Tree (AST) definitions for the Markdown Plan Compiler.
//!
//! These types model the hierarchical structure parsed from plan markdown,
//! aligning with the plan language AST.
//!
//! High-level model:
//! - `Rhei`: holds the plan title, free-form content blocks prior to the Tasks
//!   section, the plan-level `Structure`, and the list of root task nodes.
//! - `Task`: a task node with a hierarchical id, a node kind, metadata, and
//!   zero or more child task nodes.
//! - `ContentSection`: a named H2 section with title and content.

// §FS-rhei-plan-language.5: AST data model.

use serde::{Deserialize, Serialize};
use std::fmt;

pub type Metadata = serde_yaml::Mapping;

/// Default node kind when `structure.nodeKinds` is omitted.
pub const DEFAULT_NODE_KIND: &str = "task";

/// Default structural depth limit when `structure.maxLevels` is omitted.
pub const DEFAULT_MAX_LEVELS: u8 = 2;

/// Hard cap on structural depth that a markdown plan can represent
/// (matches Markdown heading depth H3..=H6).
pub const MAX_ALLOWED_LEVELS: u8 = 4;

/// One segment of a hierarchical task id.
///
/// Ids use dotted paths such as `1.2` or `api.cache.fix`. Each segment is
/// either numeric (`NUMBER`) or named (`IDENTIFIER`) per the grammar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskIdSegment {
    Number(u32),
    Named(String),
}

impl fmt::Display for TaskIdSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskIdSegment::Number(n) => write!(f, "{n}"),
            TaskIdSegment::Named(s) => write!(f, "{s}"),
        }
    }
}

/// Hierarchical task identifier.
///
/// Root tasks have a single segment (`1`, `api`). Child tasks append one
/// segment per level of nesting (`1.2`, `api.cache.fix`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId {
    pub segments: Vec<TaskIdSegment>,
}

impl TaskId {
    /// Build an id from an explicit list of segments. At least one segment
    /// must be present; callers are responsible for that invariant.
    pub fn from_segments(segments: Vec<TaskIdSegment>) -> Self {
        Self { segments }
    }

    /// Build a single-segment numeric id (`1`, `42`).
    pub fn number(n: u32) -> Self {
        Self { segments: vec![TaskIdSegment::Number(n)] }
    }

    /// Build a single-segment named id (`api`, `fix-cache`).
    pub fn named<S: Into<String>>(s: S) -> Self {
        Self { segments: vec![TaskIdSegment::Named(s.into())] }
    }

    /// Depth of the id path (number of segments). Always `>= 1` for
    /// well-formed ids.
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Whether this id is a proper extension of `other` by exactly one
    /// segment.
    pub fn extends(&self, other: &TaskId) -> bool {
        self.segments.len() == other.segments.len() + 1
            && self.segments[..other.segments.len()] == other.segments[..]
    }

    /// The parent id, if any (strips the trailing segment).
    pub fn parent(&self) -> Option<TaskId> {
        if self.segments.len() <= 1 {
            return None;
        }
        Some(Self { segments: self.segments[..self.segments.len() - 1].to_vec() })
    }

    /// Extract the single segment of a top-level id, if any.
    pub fn as_single(&self) -> Option<&TaskIdSegment> {
        if self.segments.len() == 1 {
            Some(&self.segments[0])
        } else {
            None
        }
    }

    /// Return the numeric value if this id has a single numeric segment.
    pub fn as_number(&self) -> Option<u32> {
        match self.as_single() {
            Some(TaskIdSegment::Number(n)) => Some(*n),
            _ => None,
        }
    }

    /// Return the named value if this id has a single named segment.
    pub fn as_named(&self) -> Option<&str> {
        match self.as_single() {
            Some(TaskIdSegment::Named(s)) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            seg.fmt(f)?;
        }
        Ok(())
    }
}

impl From<u32> for TaskId {
    fn from(value: u32) -> Self {
        TaskId::number(value)
    }
}

impl From<&str> for TaskId {
    fn from(value: &str) -> Self {
        TaskId::named(value)
    }
}

impl From<String> for TaskId {
    fn from(value: String) -> Self {
        TaskId::named(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSection {
    pub title: String,
    pub content: String,
    /// Rhei that contributed this section, set when a Panta project merges its
    /// rheis into one graph. `None` for a section authored on the project
    /// manifest itself, or for any plan loaded outside a project.
    ///
    /// Renderers need it to put a ticket back under the rhei it came from: the
    /// merged task list is flat, and a heading block followed by every rhei's
    /// tickets in one run is not a document anyone can read.
    // §AR-rhei-panta.3: merged ticket ids are qualified by their owning rhei.
    pub rhei: Option<String>,
}

/// Plan-level structural configuration.
///
/// Parsed from the `structure` block of the plan's YAML frontmatter. When the
/// frontmatter omits a `structure` block, the defaults are `max_levels = 2`
/// and `node_kinds = ["task"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    /// Maximum allowed node depth (1..=4). Defaults to `DEFAULT_MAX_LEVELS`.
    pub max_levels: u8,
    /// Declared node-kind keywords (canonical lowercase). Always non-empty;
    /// defaults to `["task"]`.
    pub node_kinds: Vec<String>,
}

impl Default for Structure {
    fn default() -> Self {
        Self { max_levels: DEFAULT_MAX_LEVELS, node_kinds: vec![DEFAULT_NODE_KIND.to_string()] }
    }
}

impl Structure {
    /// Returns true if the given heading keyword is declared as a node kind.
    /// Comparison is case-insensitive.
    pub fn accepts_kind(&self, kind: &str) -> bool {
        self.node_kinds.iter().any(|k| k.eq_ignore_ascii_case(kind))
    }
}

/// One `**Consumes:** <task-id>:<name>` reference: the task that publishes the
/// export, and the export's name in that task's `**Provides:**`.
// §FS-rhei-plan-language.3.12: Exports flow along the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedExport {
    /// Task that publishes the export, as authored (qualified on load for a
    /// Panta project, exactly as `prior` references are).
    pub task: TaskId,
    /// Export name, matching an entry in the producing task's `**Provides:**`.
    pub name: String,
}

/// Authored task-level snapshot inheritance control.
///
/// Absence preserves the state's rule, `Disabled` is the explicit `none`
/// form, and `Rule` replaces only the effective name and lineage axis.
// §FS-rhei-plan-language.3.13: Task snapshot inheritance overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSnapshotInherit {
    Disabled,
    Rule { name: String, from_axis: String },
}

impl TaskSnapshotInherit {
    /// The normalized markdown/JSON value of the authored rule.
    pub fn normalized(&self) -> String {
        match self {
            Self::Disabled => "none".to_string(),
            Self::Rule { name, from_axis } => format!("{name} from {from_axis}"),
        }
    }
}

/// One task-authored read exclusion. Paths retain their portable authored
/// spelling; resolution against the checkout or execution root happens at the
/// invocation boundary. §FS-rhei-plan-language.3.13
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskExclusion {
    Checkout { path: String, recursive: bool },
    Artifact { path: String, recursive: bool },
    Export(ConsumedExport),
}

impl TaskExclusion {
    /// Render the entry in the plan language's canonical spelling.
    pub fn authored(&self) -> String {
        match self {
            Self::Checkout { path, .. } => format!("checkout={path}"),
            Self::Artifact { path, .. } => format!("artifact={path}"),
            Self::Export(export) => format!("{}:{}", export.task, export.name),
        }
    }
}

/// A single node in the task tree.
///
/// Every authored `### Task <id>: ...`, `#### Task <id>: ...`, or equivalent
/// heading produces a `Task`. Child nodes are stored recursively in
/// `children`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Full hierarchical id as declared (number of segments matches the
    /// heading depth).
    pub id: TaskId,
    /// Number of synthetic project-prefix id segments that should not count
    /// toward rhei-local node-policy depth. This is zero for authored rheis
    /// and one for tickets loaded through Panta project qualification.
    pub profile_depth_offset: u8,
    /// Heading keyword in canonical lowercase form (`task`, `bug`, ...).
    pub kind: String,
    /// Title captured from the node heading.
    pub title: String,
    /// State value (raw string; semantic validation deferred to the validator).
    pub state: String,
    /// Prior dependencies as full task ids.
    pub prior: Vec<TaskId>,
    /// Node-kind keyword authored on each `**Prior:**` reference, aligned
    /// index-for-index with `prior` (`None` for bare references), kept as
    /// authored so errors can quote the source.
    // §FS-rhei-plan-language.3.1: validated against the referenced node's kind.
    pub prior_kinds: Vec<Option<String>>,
    /// Per-task snapshot inheritance overlay from `**Inherits:**`, if present.
    // §FS-rhei-plan-language.3.13: Omission, overlay, and explicit opt-out.
    pub inherits: Option<TaskSnapshotInherit>,
    /// Export names this task publishes, from `**Provides:**`, in the order
    /// they were authored.
    // §FS-rhei-plan-language.3.12: Task exports are a plan-level handoff.
    pub provides: Vec<String>,
    /// Exports this task reads from prior tasks, from `**Consumes:**`, in the
    /// order they were authored.
    // §FS-rhei-plan-language.3.12: Task exports are a plan-level handoff.
    pub consumes: Vec<ConsumedExport>,
    /// Files, directories, and declared exports this task must not read.
    // §FS-rhei-plan-language.3.13: Task-level read exclusions.
    pub excludes: Vec<TaskExclusion>,
    /// Assignee value captured from the optional `**Assignee:**` metadata
    /// field. `None` when the field is absent.
    pub assignee: Option<String>,
    /// Per-task model override from `**Model:**`, if present.
    // §FS-rhei-plan-language.3.11: Task-level model override.
    pub model: Option<String>,
    /// Per-task full execution identity override from `**Target:**`, if present.
    // §FS-rhei-plan-language.3.11: Task-level target override.
    pub target: Option<String>,
    /// Free-form content accumulated from lines between the metadata and the
    /// first child heading (or the next sibling / end of file).
    pub content: String,
    /// Child nodes nested under this node.
    pub children: Vec<Task>,
}

impl Task {
    /// Depth used for state-machine node-policy selectors. §AR-rhei-panta.3
    pub fn profile_level(&self) -> u8 {
        (self.id.depth() as u8).saturating_sub(self.profile_depth_offset).max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rhei {
    /// Title captured from the `# Rhei: <title>` header.
    pub title: String,
    /// Name of the state machine to use. Defaults to "rhei".
    pub states: String,
    /// Whether the `**States:**` line was authored explicitly.
    pub states_declared: bool,
    /// Plan-level structural configuration (max depth, allowed node kinds).
    pub structure: Structure,
    /// Optional YAML frontmatter metadata associated with the plan.
    pub metadata: Option<Metadata>,
    /// Content sections appearing before the "## Tasks" section.
    pub content_sections: Vec<ContentSection>,
    /// Collection of root task nodes defined under the "## Tasks" section.
    pub tasks: Vec<Task>,
}

/// State name used in state machine transition definitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateName(pub String);

/// Unique transition identifier built from source and target state names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransitionKey {
    /// Source state for the transition (`from` in YAML/JSON).
    pub from: StateName,
    /// Target state for the transition (`to` in YAML/JSON).
    pub to: StateName,
}

/// Reference to a platform-prefixed callback identifier.
///
/// Examples: `cli:validate_preconditions`, `js:prepareProcessing`,
/// `py:validate_dataset`, `java:com.example.Handler::method`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallbackRef(pub String);

/// Declarative transition rule from the state machine configuration.
///
/// Field names and serde mappings follow the transition YAML/JSON shape from
/// the formal transition docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRule {
    /// Source state name, or `"*"` for wildcard semantics.
    pub from: StateName,
    /// Optional owner-independent wildcard source set. §FS-rhei-transitions.4.6
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    /// Target state name.
    pub to: StateName,
    /// Optional callback invoked before leaving the source state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_leave: Option<CallbackRef>,
    /// Optional callback invoked after entering the target state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_enter: Option<CallbackRef>,
    /// Optional condition expression for system-triggered transitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Optional timeout duration (for example `24h`, `30m`, `45s`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Optional exit-code condition for transitions from program states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<serde_yaml::Value>,
    /// Optional tooling-unavailable trigger for required MCP servers.
    ///
    /// `true` matches any required MCP unavailability; a list matches only
    /// when one of the listed ids failed its availability check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_unavailable: Option<serde_yaml::Value>,
    /// Optional tooling-unavailable trigger for required skills. Same shape as `mcp_unavailable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_unavailable: Option<serde_yaml::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn task_id_number_and_named_display() {
        assert_eq!(TaskId::number(3).to_string(), "3");
        assert_eq!(TaskId::named("api").to_string(), "api");
    }

    #[test]
    fn task_id_dotted_display_and_parent() {
        let id = TaskId::from_segments(vec![
            TaskIdSegment::Number(1),
            TaskIdSegment::Number(2),
            TaskIdSegment::Named("fix".into()),
        ]);
        assert_eq!(id.to_string(), "1.2.fix");
        assert_eq!(id.depth(), 3);
        let parent = id.parent().expect("has parent");
        assert_eq!(parent.to_string(), "1.2");
        let grandparent = parent.parent().expect("has parent");
        assert_eq!(grandparent.to_string(), "1");
        assert_eq!(grandparent.parent(), None);
    }

    #[test]
    fn task_id_extends_checks_one_segment_extension() {
        let parent = TaskId::number(1);
        let child = TaskId::from_segments(vec![TaskIdSegment::Number(1), TaskIdSegment::Number(2)]);
        assert!(child.extends(&parent));
        let grandchild = TaskId::from_segments(vec![
            TaskIdSegment::Number(1),
            TaskIdSegment::Number(2),
            TaskIdSegment::Number(3),
        ]);
        assert!(!grandchild.extends(&parent));
        assert!(grandchild.extends(&child));
    }

    #[test]
    fn task_id_single_segment_helpers() {
        let n = TaskId::number(7);
        assert_eq!(n.as_number(), Some(7));
        assert_eq!(n.as_named(), None);

        let name = TaskId::named("api");
        assert_eq!(name.as_number(), None);
        assert_eq!(name.as_named(), Some("api"));

        let dotted =
            TaskId::from_segments(vec![TaskIdSegment::Number(1), TaskIdSegment::Number(2)]);
        assert_eq!(dotted.as_number(), None);
        assert_eq!(dotted.as_named(), None);
    }

    #[test]
    fn structure_defaults() {
        let s = Structure::default();
        assert_eq!(s.max_levels, 2);
        assert_eq!(s.node_kinds, vec!["task".to_string()]);
        assert!(s.accepts_kind("task"));
        assert!(s.accepts_kind("Task"));
        assert!(!s.accepts_kind("bug"));
    }

    #[test]
    fn state_name_equality_and_roundtrip_json() {
        let a = StateName("pending".to_string());
        let b = StateName("pending".to_string());
        let c = StateName("running".to_string());

        assert_eq!(a, b);
        assert_ne!(a, c);

        let encoded = serde_json::to_string(&a).expect("serialize StateName");
        assert_eq!(encoded, "\"pending\"");

        let decoded: StateName = serde_json::from_str(&encoded).expect("deserialize StateName");
        assert_eq!(decoded, a);
    }

    #[test]
    fn callback_ref_roundtrip_json() {
        let callback = CallbackRef("cli:validate_preconditions".to_string());

        let encoded = serde_json::to_string(&callback).expect("serialize CallbackRef");
        assert_eq!(encoded, "\"cli:validate_preconditions\"");

        let decoded: CallbackRef = serde_json::from_str(&encoded).expect("deserialize CallbackRef");
        assert_eq!(decoded, callback);
    }

    #[test]
    fn transition_key_construction_and_json() {
        let key = TransitionKey {
            from: StateName("draft".to_string()),
            to: StateName("pending".to_string()),
        };

        let value = serde_json::to_value(&key).expect("serialize TransitionKey");
        assert_eq!(
            value,
            json!({
                "from": "draft",
                "to": "pending"
            })
        );

        let decoded: TransitionKey =
            serde_json::from_value(value).expect("deserialize TransitionKey");
        assert_eq!(decoded, key);
    }

    #[test]
    fn transition_rule_serializes_with_optional_fields_omitted() {
        let rule = TransitionRule {
            sources: None,
            from: StateName("pending".to_string()),
            to: StateName("running".to_string()),
            on_leave: None,
            on_enter: None,
            condition: None,
            timeout: None,
            exit_code: None,
            mcp_unavailable: None,
            skill_unavailable: None,
        };

        let value = serde_json::to_value(&rule).expect("serialize TransitionRule");
        assert_eq!(
            value,
            json!({
                "from": "pending",
                "to": "running"
            })
        );
    }

    #[test]
    fn transition_rule_full_roundtrip_with_callbacks_and_edge_values() {
        let original = TransitionRule {
            sources: None,
            from: StateName("*".to_string()),
            to: StateName("cancelled".to_string()),
            on_leave: Some(CallbackRef("js:package_for_review".to_string())),
            on_enter: Some(CallbackRef(
                "java:com.example.workflow.WorkflowHandlers::recordCompletion".to_string(),
            )),
            condition: Some("retryCount >= 3".to_string()),
            timeout: Some("24h".to_string()),
            exit_code: None,
            mcp_unavailable: None,
            skill_unavailable: None,
        };

        let value = serde_json::to_value(&original).expect("serialize TransitionRule");
        assert_eq!(
            value,
            json!({
                "from": "*",
                "to": "cancelled",
                "on_leave": "js:package_for_review",
                "on_enter": "java:com.example.workflow.WorkflowHandlers::recordCompletion",
                "condition": "retryCount >= 3",
                "timeout": "24h"
            })
        );

        let decoded: TransitionRule =
            serde_json::from_value(value).expect("deserialize TransitionRule");
        assert_eq!(decoded, original);
    }

    #[test]
    fn transition_rule_deserializes_without_optional_fields() {
        let input = json!({
            "from": "queued",
            "to": "processing"
        });

        let rule: TransitionRule =
            serde_json::from_value(input).expect("deserialize TransitionRule");
        assert_eq!(rule.from, StateName("queued".to_string()));
        assert_eq!(rule.to, StateName("processing".to_string()));
        assert_eq!(rule.on_leave, None);
        assert_eq!(rule.on_enter, None);
        assert_eq!(rule.condition, None);
        assert_eq!(rule.timeout, None);
    }

    #[test]
    fn transition_rule_rejects_non_string_callback_ref() {
        let input = json!({
            "from": "pending",
            "to": "running",
            "on_leave": { "invalid": true }
        });

        let err =
            serde_json::from_value::<TransitionRule>(input).expect_err("expected invalid callback");
        let message = err.to_string();
        assert!(message.contains("string"), "error should mention string type, got: {message}");
    }

    #[test]
    fn transition_rule_json_field_names_match_spec() {
        let rule = TransitionRule {
            sources: None,
            from: StateName("in-progress".to_string()),
            to: StateName("human-review".to_string()),
            on_leave: Some(CallbackRef("cli:package_for_review".to_string())),
            on_enter: Some(CallbackRef("cli:notify_reviewers".to_string())),
            condition: Some("priority == \"high\"".to_string()),
            timeout: Some("30m".to_string()),
            exit_code: None,
            mcp_unavailable: None,
            skill_unavailable: None,
        };

        let obj = serde_json::to_value(&rule).expect("serialize TransitionRule");
        let keys = match obj {
            Value::Object(map) => map.keys().cloned().collect::<Vec<_>>(),
            _ => panic!("expected object"),
        };

        assert!(keys.contains(&"from".to_string()));
        assert!(keys.contains(&"to".to_string()));
        assert!(keys.contains(&"on_leave".to_string()));
        assert!(keys.contains(&"on_enter".to_string()));
        assert!(keys.contains(&"condition".to_string()));
        assert!(keys.contains(&"timeout".to_string()));
    }
}
