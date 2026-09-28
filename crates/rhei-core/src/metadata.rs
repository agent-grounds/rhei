//! The keys rhei writes into a task's metadata, and the one conversion every
//! JSON surface that publishes frontmatter uses.
//!
//! `metadata.tasks.<id>` is one flat map holding an author's fields beside
//! rhei's own bookkeeping, with nothing in the shape to tell them apart
//! (§FS-rhei-transitions.2.5). The register below is what tells them apart, so a
//! surface offering a caller "this task's metadata" as a stable contract offers
//! the author's layer alone.
//!
//! The conversion lives beside it because the two surfaces that publish
//! frontmatter — `rhei list --json` and `rhei render --format json` — must never
//! disagree about what a stored value looks like (§FS-rhei-render.3.1.1).

use serde_json::{Map, Value};
use serde_yaml::{Mapping, Number, Value as YamlValue};

use crate::ast::Metadata;

/// Entry counters per state, for a counted loop or a poll's attempts.
/// §FS-rhei-transitions.2.3
pub const STATE_VISITS_KEY: &str = "stateVisits";
/// The instant a polling state may be attempted again, per state.
/// §FS-rhei-states.2.2
pub const POLL_NEXT_ATTEMPT_AT_KEY: &str = "pollNextAttemptAt";
/// A parked provider-limit wait, per state. §FS-rhei-run.3.3
pub const PROVIDER_LIMITS_KEY: &str = "providerLimits";
/// A supervising task's phase and its undelivered checkpoints.
/// §FS-rhei-supervision.3.3
pub const SUPERVISION_KEY: &str = "supervision";
/// The ticket's budget identity. §FS-rhei-budgets.5.2
pub const BUDGET_TICKET_ID_KEY: &str = "budgetTicketId";

/// One key rhei itself writes into a task's `metadata.tasks.<id>` map.
/// §FS-rhei-transitions.2.5
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RheiWrittenKey {
    /// The key as it is stored, spelled exactly as rhei writes it.
    pub name: &'static str,
    /// Whether `rhei reset` deletes it — the register's own column, which is
    /// what reset reads rather than keeping a second list. §FS-rhei-reset.2
    pub cleared_by_reset: bool,
}

/// The whole set of keys rhei writes into a task's metadata map.
///
/// A key here is never published as author metadata, and a point introducing a
/// new runtime key adds a row — otherwise the next key added would be published
/// by omission, on a read-only command, which is the one place a mistake becomes
/// a contract. The register reserves nothing: a plan may author one of these
/// names and still validate, and the key keeps its runtime meaning.
/// §FS-rhei-transitions.2.5
pub const RHEI_WRITTEN_KEYS: &[RheiWrittenKey] = &[
    RheiWrittenKey { name: STATE_VISITS_KEY, cleared_by_reset: true },
    RheiWrittenKey { name: POLL_NEXT_ATTEMPT_AT_KEY, cleared_by_reset: false },
    RheiWrittenKey { name: PROVIDER_LIMITS_KEY, cleared_by_reset: true },
    RheiWrittenKey { name: SUPERVISION_KEY, cleared_by_reset: true },
    RheiWrittenKey { name: BUDGET_TICKET_ID_KEY, cleared_by_reset: false },
];

/// Whether the register names this key, which is what holds it back from every
/// surface that publishes author metadata. §FS-rhei-transitions.2.5
pub fn is_rhei_written_key(name: &str) -> bool {
    RHEI_WRITTEN_KEYS.iter().any(|key| key.name == name)
}

/// Every register key `rhei reset` deletes, read off the register's own column.
/// §FS-rhei-reset.2 §FS-rhei-transitions.2.5
pub fn keys_cleared_by_reset() -> impl Iterator<Item = &'static str> {
    RHEI_WRITTEN_KEYS.iter().filter(|key| key.cleared_by_reset).map(|key| key.name)
}

/// Why JSON has no image for a frontmatter value. §FS-rhei-render.3.1.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unrepresentable {
    /// A sequence or a mapping used as a mapping key: JSON cannot name it.
    NonScalarKey,
    /// `.inf`, `-.inf` or `.nan`, carried as the text the report names it by:
    /// JSON has no number for any of them.
    NonFiniteFloat(String),
}

/// One frontmatter value JSON cannot hold, located well enough for an author to
/// go and edit it. §FS-rhei-render.3.1.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrepresentableValue {
    /// Keys from the frontmatter root down to the value, each already spelled
    /// the way the report names it: a scalar key in quotes, a non-scalar key by
    /// the shape used as one, a sequence position by its index.
    pub path: Vec<String>,
    /// What JSON could not hold, and how the report says so.
    pub reason: Unrepresentable,
}

/// The two path steps that make a value a task's rather than the document's.
const METADATA_TASKS_PATH: [&str; 2] = ["\"metadata\"", "\"tasks\""];

impl UnrepresentableValue {
    /// The site the report names: the project-qualified task id when the value
    /// sits under `metadata.tasks.<id>` — the id every other surface prints —
    /// and the document itself otherwise. §FS-rhei-render.3.1.1
    pub fn site(&self) -> String {
        match self.task_id() {
            Some(id) => id.to_string(),
            None => "frontmatter".to_string(),
        }
    }

    /// The path below the site, which is the key the author has to go and edit.
    pub fn key_path(&self) -> String {
        let below = if self.task_id().is_some() { 3 } else { 0 };
        self.path[below..].join(".")
    }

    /// One line of the report: where the value is, and what JSON could not do
    /// with it. §FS-rhei-render.3.1.1
    pub fn describe(&self) -> String {
        let (site, key) = (self.site(), self.key_path());
        match &self.reason {
            Unrepresentable::NonScalarKey => {
                format!("{site}: {key} used as a mapping key, which JSON cannot name")
            }
            Unrepresentable::NonFiniteFloat(text) => {
                format!("{site}: {key} holds {text}, which JSON has no number for")
            }
        }
    }

    /// The task whose metadata holds the value, when one does. A value at
    /// `metadata.tasks.<id>` itself has no owning task: it *is* the id.
    fn task_id(&self) -> Option<&str> {
        if self.path.len() <= METADATA_TASKS_PATH.len() + 1 {
            return None;
        }
        if self.path[..METADATA_TASKS_PATH.len()] != METADATA_TASKS_PATH {
            return None;
        }
        unquote(&self.path[METADATA_TASKS_PATH.len()])
    }
}

/// Convert a plan's parsed frontmatter to JSON, or name **every** value in it
/// that JSON has no image for.
///
/// One run reports them all (§FS-rhei-errors.1.1): an author fixing frontmatter
/// by hand should not have to run the command once per offending key.
/// §FS-rhei-render.3.1.1
pub fn frontmatter_to_json(document: &Metadata) -> Result<Value, Vec<UnrepresentableValue>> {
    let mut found = Vec::new();
    let mut path = Vec::new();
    let value = convert_mapping(document, &mut path, &mut found);
    if found.is_empty() {
        Ok(value)
    } else {
        Err(found)
    }
}

/// A task's persisted author metadata, read off an already-converted
/// frontmatter document: its `metadata.tasks.<id>` entry with every key the
/// register names filtered out.
///
/// `None` in the three cases a caller need not distinguish — no frontmatter
/// entry for the task, and an entry holding only registered keys — because
/// presence is the declaration on this field rather than emptiness.
/// §FS-rhei-list.4.2 §FS-rhei-transitions.2.5
pub fn author_task_metadata(frontmatter: &Value, task_id: &str) -> Option<Value> {
    let stored = frontmatter.get("metadata")?.get("tasks")?.get(task_id)?.as_object()?;
    let author: Map<String, Value> = stored
        .iter()
        .filter(|(key, _)| !is_rhei_written_key(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    (!author.is_empty()).then_some(Value::Object(author))
}

fn convert(
    value: &YamlValue,
    path: &mut Vec<String>,
    found: &mut Vec<UnrepresentableValue>,
) -> Value {
    match value {
        YamlValue::Null => Value::Null,
        YamlValue::Bool(flag) => Value::Bool(*flag),
        YamlValue::Number(number) => convert_number(number, path, found),
        YamlValue::String(text) => Value::String(text.clone()),
        YamlValue::Sequence(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    path.push(format!("[{index}]"));
                    let converted = convert(item, path, found);
                    path.pop();
                    converted
                })
                .collect(),
        ),
        YamlValue::Mapping(map) => convert_mapping(map, path, found),
        // A tagged value becomes a one-key object naming the tag, which is what
        // this surface has always emitted for one. §FS-rhei-render.3.1.1
        YamlValue::Tagged(tagged) => {
            let tag = tagged.tag.to_string();
            path.push(quote(&tag));
            let inner = convert(&tagged.value, path, found);
            path.pop();
            Value::Object(Map::from_iter([(tag, inner)]))
        }
    }
}

fn convert_mapping(
    map: &Mapping,
    path: &mut Vec<String>,
    found: &mut Vec<UnrepresentableValue>,
) -> Value {
    let mut object = Map::new();
    for (key, value) in map {
        // JSON has only string keys, so the author's own spelling is the one
        // name every reader can agree on. §FS-rhei-render.3.1.1
        let Some(name) = scalar_key_text(key) else {
            path.push(non_scalar_shape(key).to_string());
            found.push(UnrepresentableValue {
                path: path.clone(),
                reason: Unrepresentable::NonScalarKey,
            });
            path.pop();
            continue;
        };
        path.push(quote(&name));
        // A key is named before its value is walked, so a non-finite float used
        // as one is reported where the author wrote it. §FS-rhei-render.3.1.1
        if let YamlValue::Number(number) = key {
            if !is_finite(number) {
                found.push(UnrepresentableValue {
                    path: path.clone(),
                    reason: Unrepresentable::NonFiniteFloat(non_finite_text(number)),
                });
                path.pop();
                continue;
            }
        }
        let converted = convert(value, path, found);
        path.pop();
        object.insert(name, converted);
    }
    Value::Object(object)
}

fn convert_number(
    number: &Number,
    path: &[String],
    found: &mut Vec<UnrepresentableValue>,
) -> Value {
    if !is_finite(number) {
        found.push(UnrepresentableValue {
            path: path.to_vec(),
            reason: Unrepresentable::NonFiniteFloat(non_finite_text(number)),
        });
        return Value::Null;
    }
    if let Some(signed) = number.as_i64() {
        return Value::Number(signed.into());
    }
    if let Some(unsigned) = number.as_u64() {
        return Value::Number(unsigned.into());
    }
    number.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null)
}

/// The YAML text of a scalar key, or `None` for a shape JSON cannot name.
fn scalar_key_text(key: &YamlValue) -> Option<String> {
    match key {
        YamlValue::Null => Some("null".to_string()),
        YamlValue::Bool(flag) => Some(flag.to_string()),
        YamlValue::Number(number) => {
            Some(if is_finite(number) { number.to_string() } else { non_finite_text(number) })
        }
        YamlValue::String(text) => Some(text.clone()),
        _ => None,
    }
}

/// How the report names a key that has no name to give. §FS-rhei-render.3.1.1
fn non_scalar_shape(key: &YamlValue) -> &'static str {
    match key {
        YamlValue::Sequence(_) => "a sequence",
        YamlValue::Mapping(_) => "a mapping",
        _ => "a tagged value",
    }
}

fn is_finite(number: &Number) -> bool {
    number.as_f64().is_none_or(f64::is_finite)
}

/// The YAML spelling of a float JSON has no number for, so the report names what
/// the author wrote rather than a Rust rendering of it.
fn non_finite_text(number: &Number) -> String {
    match number.as_f64() {
        Some(value) if value.is_nan() => ".nan".to_string(),
        Some(value) if value.is_sign_negative() => "-.inf".to_string(),
        _ => ".inf".to_string(),
    }
}

fn quote(name: &str) -> String {
    format!("\"{name}\"")
}

fn unquote(step: &str) -> Option<&str> {
    step.strip_prefix('"')?.strip_suffix('"')
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;
