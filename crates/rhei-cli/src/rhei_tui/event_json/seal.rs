//! The `seal_halt` record, in both directions: a strict run's refusal of a
//! ticket whose rhei's sealed inputs changed. §FS-rhei-seal.8.4 §FS-rhei-run-json.2.1

use serde_json::{json, Value};

use crate::rhei_tui::event::RunEvent;

/// The one reason a `seal_halt` has today, stable for a reader to route on.
const SEALED_INPUT_CHANGED: &str = "sealed_input_changed";

pub(super) fn put_seal_halt(event: &RunEvent, put: &mut dyn FnMut(&str, Value)) {
    let RunEvent::SealHalt { task, rhei, state, generation, changes, remedy } = event else {
        return;
    };
    put("event", json!("seal_halt"));
    put("task", json!(task));
    put("rhei", json!(rhei));
    put("state", json!(state));
    put("reason_code", json!(SEALED_INPUT_CHANGED));
    put("generation", json!(generation));
    let changes: Vec<Value> =
        changes.iter().map(|(path, kind)| json!({ "path": path, "kind": kind })).collect();
    put("changes", Value::Array(changes));
    put("remedy", json!(remedy));
}

/// `None` for any other `kind`, so the record dispatch can fall through to it.
pub(super) fn decode_seal_halt(kind: &str, v: &Value) -> Option<RunEvent> {
    if kind != "seal_halt" {
        return None;
    }
    let text = |key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let changes = v
        .get("changes")?
        .as_array()?
        .iter()
        .filter_map(|change| {
            let field = |key: &str| change.get(key)?.as_str().map(str::to_string);
            Some((field("path")?, field("kind")?))
        })
        .collect();
    Some(RunEvent::SealHalt {
        task: text("task"),
        rhei: text("rhei"),
        state: text("state"),
        generation: v.get("generation")?.as_u64()? as u32,
        changes,
        remedy: text("remedy"),
    })
}
