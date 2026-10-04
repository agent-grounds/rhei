use serde_json::{json, Map, Value};

use rhei_core::ast::{Rhei, Task, TaskId, TaskIdSegment};
use rhei_core::metadata::{frontmatter_to_json, UnrepresentableValue};

use crate::rhei_output::PlanOutputGenerator;

/// Every frontmatter value this document cannot carry, or the document.
/// §FS-rhei-render.3.1.1
type JsonResult<T> = Result<T, Vec<UnrepresentableValue>>;

/// The state machine one rhei of a merged project resolves.
///
/// A merged project flattens every rhei's tickets into one qualified task list
/// while the machine stays a per-rhei property, so the top-level `states` field
/// alone cannot say what a given task's state name means.
// §FS-rhei-render.3.1 §DA-per-rhei-state-machines
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RheiMachine {
    /// Rhei id — the first segment of every ticket id it owns.
    pub id: String,
    /// The `name:` of the machine the rhei resolves: its own root's
    /// `states.yaml`, else the project default.
    pub states: String,
}

pub struct JsonOutput {
    pub pretty: bool,
    /// The resolved project default's name, for the top-level `states` field.
    /// §FS-rhei-render.3.1
    pub states: String,
    /// Per-rhei machine attribution, in presentation order. Empty for a plan
    /// that is not a merged project, whose one `states` field already says
    /// everything.
    pub rheis: Vec<RheiMachine>,
}

impl Default for JsonOutput {
    /// A document no machine was resolved for names the built-in machine.
    fn default() -> Self {
        Self { pretty: false, states: BUILTIN_MACHINE.to_string(), rheis: Vec::new() }
    }
}

/// The built-in machine's name, which a plan with no `states.yaml` runs under.
/// §FS-rhei-plan-language.1.3
const BUILTIN_MACHINE: &str = "rhei";

impl PlanOutputGenerator for JsonOutput {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> JsonResult<serde_json::Value> {
        rhei_json(rhei, &self.states, &self.rheis)
    }
}

/// Convert a parsed Rhei into a serde_json::Value.
pub fn to_json_value(rhei: &rhei_core::ast::Rhei) -> JsonResult<serde_json::Value> {
    JsonOutput::default().generate_rhei(rhei)
}

/// [`to_json_value`] carrying the resolved project default and, for a merged
/// project, each rhei's machine. §FS-rhei-render.3.1
pub fn to_json_value_with_rheis(
    rhei: &rhei_core::ast::Rhei,
    states: &str,
    rheis: Vec<RheiMachine>,
) -> JsonResult<serde_json::Value> {
    JsonOutput { pretty: false, states: states.to_string(), rheis }.generate_rhei(rhei)
}

/// Convert a parsed Rhei into a pretty-printed JSON string.
pub fn to_json_string_pretty(rhei: &rhei_core::ast::Rhei) -> JsonResult<String> {
    to_json_string_pretty_with_rheis(rhei, BUILTIN_MACHINE, Vec::new())
}

/// [`to_json_string_pretty`] for a resolved plan. §FS-rhei-render.3.1
pub fn to_json_string_pretty_with_rheis(
    rhei: &rhei_core::ast::Rhei,
    states: &str,
    rheis: Vec<RheiMachine>,
) -> JsonResult<String> {
    let v = to_json_value_with_rheis(rhei, states, rheis)?;
    Ok(serde_json::to_string_pretty(&v).expect("pretty JSON serialization"))
}

// -----------------------------------------------------------------------------
// Internal helpers
// -----------------------------------------------------------------------------

fn id_segment_json(seg: &TaskIdSegment) -> Value {
    match seg {
        TaskIdSegment::Number(n) => json!({ "number": n }),
        TaskIdSegment::Named(s) => json!({ "named": s }),
    }
}

fn task_id_json(id: &TaskId) -> Value {
    let segments: Vec<Value> = id.segments.iter().map(id_segment_json).collect();
    json!({
        "path": id.to_string(),
        "segments": segments,
    })
}

fn task_json(t: &Task) -> Value {
    let prior = t.prior.iter().map(task_id_json).collect::<Vec<Value>>();
    let children = t.children.iter().map(task_json).collect::<Vec<Value>>();

    let mut obj = Map::new();
    obj.insert("id".to_string(), task_id_json(&t.id));
    obj.insert("kind".to_string(), Value::String(t.kind.clone()));
    obj.insert("title".to_string(), Value::String(t.title.clone()));
    obj.insert("state".to_string(), Value::String(t.state.clone()));
    obj.insert("prior".to_string(), Value::Array(prior));
    if let Some(inherit) = &t.inherits {
        // Report authored metadata only; state defaults remain in the machine.
        // §FS-rhei-render.3.1
        obj.insert("inherits".to_string(), Value::String(inherit.normalized()));
    }
    let exclusions = t
        .excludes
        .iter()
        .map(|entry| match entry {
            rhei_core::ast::TaskExclusion::Checkout { path, recursive: false } => {
                json!({ "kind": "checkout", "path": path })
            }
            rhei_core::ast::TaskExclusion::Artifact { path, recursive: false } => {
                json!({ "kind": "artifact", "path": path })
            }
            rhei_core::ast::TaskExclusion::Checkout { path, recursive: true } => {
                json!({ "kind": "checkout", "path": path, "recursive": true })
            }
            rhei_core::ast::TaskExclusion::Artifact { path, recursive: true } => {
                json!({ "kind": "artifact", "path": path, "recursive": true })
            }
            rhei_core::ast::TaskExclusion::Export(export) => json!({
                "kind": "export", "task": export.task.to_string(), "name": export.name
            }),
        })
        .collect();
    obj.insert("excludes".to_string(), Value::Array(exclusions));
    if let Some(ref assignee) = t.assignee {
        obj.insert("assignee".to_string(), Value::String(assignee.clone()));
    }
    if let Some(ref model) = t.model {
        obj.insert("model".to_string(), Value::String(model.clone()));
    }
    if let Some(ref target) = t.target {
        obj.insert("target".to_string(), Value::String(target.clone()));
    }
    if !t.content.is_empty() {
        obj.insert("content".to_string(), Value::String(t.content.clone()));
    }
    obj.insert("children".to_string(), Value::Array(children));
    Value::Object(obj)
}

fn rhei_json(rhei: &Rhei, states: &str, rheis: &[RheiMachine]) -> JsonResult<Value> {
    let content_sections = rhei
        .content_sections
        .iter()
        .map(|s| {
            json!({
                "title": s.title,
                "content": s.content,
            })
        })
        .collect::<Vec<Value>>();

    let tasks = rhei.tasks.iter().map(task_json).collect::<Vec<Value>>();

    let mut obj = Map::new();
    obj.insert("title".to_string(), Value::String(rhei.title.clone()));
    obj.insert("states".to_string(), Value::String(states.to_string()));
    // One machine per rhei: the `states` field above is only the project
    // default. Resolve a task through the first segment of its id.
    // §FS-rhei-render.3.1
    if !rheis.is_empty() {
        obj.insert(
            "rheis".to_string(),
            Value::Array(
                rheis
                    .iter()
                    .map(|entry| {
                        json!({
                            "id": entry.id,
                            "states": entry.states,
                        })
                    })
                    .collect(),
            ),
        );
    }
    obj.insert(
        "structure".to_string(),
        json!({
            "max_levels": rhei.structure.max_levels,
            "node_kinds": rhei.structure.node_kinds,
        }),
    );
    // The whole parsed frontmatter document, deliberately unfiltered: `render`
    // exports the document, and a document with its counters removed is not the
    // document. `null` where the plan declared none. §FS-rhei-render.3.1
    obj.insert(
        "frontmatter".to_string(),
        match rhei.metadata.as_ref() {
            Some(metadata) => frontmatter_to_json(metadata)?,
            None => Value::Null,
        },
    );
    obj.insert("content_sections".to_string(), Value::Array(content_sections));
    obj.insert("tasks".to_string(), Value::Array(tasks));
    Ok(Value::Object(obj))
}
