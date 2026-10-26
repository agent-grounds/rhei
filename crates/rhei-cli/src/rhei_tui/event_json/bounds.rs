//! The count-bound codec, in both directions: one dimension's fields as the
//! `budget_snapshot` and `budget_halt` records carry them, and the
//! `ceiling_policy` naming the file that delegated a ceiling. Both records go
//! through these functions, so their bounds cannot be spelled two ways.
//! §FS-rhei-run-json.2.1

use serde_json::{json, Value};

use crate::rhei_tui::event::{BoundCeiling, BoundReport};

/// One count dimension's fields, shared by the snapshot record and the halt so
/// the two cannot drift apart. §FS-rhei-run-json.2.1
pub(super) fn bound_value(bound: &BoundReport) -> Value {
    let mut value = json!({
        "dimension": bound.dimension,
        "effective": bound.effective,
        "value_source": bound.value_source,
        "limiting_source": bound.limiting_source,
        "consumed": bound.consumed,
        "outstanding": bound.outstanding,
        "remaining": bound.remaining,
        "mode": bound.mode,
        "window": bound.window,
        // Present exactly on the dimension whose amounts are money, so a
        // reader never has to parse a rendered `$`. §FS-rhei-run-json.2.1
        "currency": bound.currency,
        "unpriced": bound.marks.map(|marks| marks.unpriced),
        "unmeasurable": bound.marks.map(|marks| marks.unmeasurable),
        "unsettled": bound.marks.map(|marks| marks.unsettled),
    });
    // Only on a delegated entry, so a machine that never set the switch emits
    // the records it always did. §FS-rhei-run-json.2.1
    if let Some(ceiling) = &bound.ceiling {
        value["ceiling"] = json!({
            "value": ceiling.value,
            "source": "project",
            "path": ceiling.path,
        });
    }
    value
}

/// `{ "delegated": [<keys>], "setting": "defaults.clamp_projects", "path" }`
/// over the delegated entries among `bounds`; `None` where there are none.
/// §FS-rhei-run-json.2.1
pub(super) fn ceiling_policy_value(bounds: &[BoundReport]) -> Option<Value> {
    let delegated: Vec<&BoundCeiling> =
        bounds.iter().filter_map(|bound| bound.ceiling.as_ref()).collect();
    let first = delegated.first()?;
    Some(json!({
        "delegated": delegated.iter().map(|ceiling| ceiling.key.as_str()).collect::<Vec<_>>(),
        "setting": rhei_core::budget::CLAMP_PROJECTS,
        "path": first.delegated_by,
    }))
}

/// Read a delegated entry's `ceiling` back, taking the key from its dimension
/// and the delegating file from the record's `ceiling_policy`.
/// §FS-rhei-run-json.2.1
fn bound_ceiling(entry: &Value, policy: Option<&Value>) -> Option<BoundCeiling> {
    let ceiling = entry.get("ceiling")?;
    let dimension = entry.get("dimension")?.as_str()?;
    let key = [rhei_core::budget::Dimension::Travel, rhei_core::budget::Dimension::Invocations]
        .into_iter()
        .find(|known| known.label() == dimension)?
        .settings_key();
    Some(BoundCeiling {
        key: key.to_string(),
        value: ceiling.get("value").and_then(Value::as_u64).unwrap_or_default(),
        path: ceiling.get("path").and_then(Value::as_str).map(str::to_owned),
        delegated_by: policy
            .and_then(|policy| policy.get("path"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

pub(super) fn bound_report(v: &Value, policy: Option<&Value>) -> Option<BoundReport> {
    Some(BoundReport {
        ceiling: bound_ceiling(v, policy),
        dimension: v.get("dimension")?.as_str()?.to_owned(),
        effective: v.get("effective").and_then(Value::as_u64).unwrap_or_default(),
        value_source: v.get("value_source").and_then(Value::as_str).unwrap_or("").to_owned(),
        limiting_source: v.get("limiting_source").and_then(Value::as_str).map(str::to_owned),
        consumed: v.get("consumed").and_then(Value::as_u64).unwrap_or_default(),
        outstanding: v.get("outstanding").and_then(Value::as_u64).unwrap_or_default(),
        remaining: v.get("remaining").and_then(Value::as_u64).unwrap_or_default(),
        mode: v.get("mode").and_then(Value::as_str).unwrap_or("").to_owned(),
        window: v.get("window").and_then(Value::as_str).map(str::to_owned),
        currency: v.get("currency").and_then(Value::as_str).map(str::to_owned),
        marks: v.get("unpriced").and_then(Value::as_u64).map(|unpriced| {
            rhei_core::budget::SpendMarks {
                unpriced,
                unmeasurable: v.get("unmeasurable").and_then(Value::as_u64).unwrap_or_default(),
                unsettled: v.get("unsettled").and_then(Value::as_u64).unwrap_or_default(),
            }
        }),
    })
}
