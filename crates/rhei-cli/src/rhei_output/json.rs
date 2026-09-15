use serde_json::{json, Map, Value};

use rhei_core::ast::{Rhei, Task, TaskId, TaskIdSegment};

use crate::rhei_output::PlanOutputGenerator;

/// The state machine one rhei of a merged project runs under.
///
/// A merged project flattens every rhei's tickets into one qualified task list
/// while the machine stays a per-rhei property, so the top-level `states` field
/// alone cannot say what a given task's state name means.
// §FS-rhei-render.3.1 §DA-per-rhei-state-machines
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RheiMachine {
    /// Rhei id — the first segment of every ticket id it owns.
    pub id: String,
    /// Effective machine name: the rhei's own declaration, or the project
    /// default when it declares none.
    pub states: String,
    /// Whether the rhei declared the machine itself rather than inheriting it.
    pub declared: bool,
}

#[derive(Default)]
pub struct JsonOutput {
    pub pretty: bool,
    /// Per-rhei machine attribution, in presentation order. Empty for a plan
    /// that is not a merged project, whose one `states` field already says
    /// everything.
    pub rheis: Vec<RheiMachine>,
}

impl PlanOutputGenerator for JsonOutput {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
        rhei_json(rhei, &self.rheis)
    }
}

/// Convert a parsed Rhei into a serde_json::Value.
pub fn to_json_value(rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
    JsonOutput::default().generate_rhei(rhei)
}

/// [`to_json_value`] for a merged project, carrying each rhei's machine.
/// §FS-rhei-render.3.1
pub fn to_json_value_with_rheis(
    rhei: &rhei_core::ast::Rhei,
    rheis: Vec<RheiMachine>,
) -> serde_json::Value {
    JsonOutput { pretty: false, rheis }.generate_rhei(rhei)
}

/// Convert a parsed Rhei into a pretty-printed JSON string.
pub fn to_json_string_pretty(rhei: &rhei_core::ast::Rhei) -> String {
    to_json_string_pretty_with_rheis(rhei, Vec::new())
}

/// [`to_json_string_pretty`] for a merged project. §FS-rhei-render.3.1
pub fn to_json_string_pretty_with_rheis(
    rhei: &rhei_core::ast::Rhei,
    rheis: Vec<RheiMachine>,
) -> String {
    let v = to_json_value_with_rheis(rhei, rheis);
    serde_json::to_string_pretty(&v).expect("pretty JSON serialization")
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

fn rhei_json(rhei: &Rhei, rheis: &[RheiMachine]) -> Value {
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
    obj.insert("states".to_string(), Value::String(rhei.states.clone()));
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
                            "states_declared": entry.declared,
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
    obj.insert(
        "frontmatter".to_string(),
        rhei.metadata
            .as_ref()
            .and_then(|metadata| serde_json::to_value(metadata).ok())
            .unwrap_or(Value::Null),
    );
    obj.insert("content_sections".to_string(), Value::Array(content_sections));
    obj.insert("tasks".to_string(), Value::Array(tasks));
    Value::Object(obj)
}
