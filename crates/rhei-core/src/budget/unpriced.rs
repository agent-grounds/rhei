//! What an `unpriced` count holds: the pair each receipt could not price and
//! the book that could not price it.
//!
//! A count alone sends a reader hunting through the journal, the machine's
//! targets and the price book side by side. Price entries match provider and
//! model exactly, so naming the pair and the book names the fix, and every
//! surface that counts `unpriced` names them from here, in one set of words.
//! §FS-rhei-budgets.8.1 §FS-rhei-budgets.5.2

use super::journal::{Audit, Journal};
use super::types::SpendBasis;
use super::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The `pricing_context` an unpriced `spend` receipt carries: the invocation's
/// final pair and the book the run selected. Every member may be unknown.
///
/// The fields are declared in the order `unpriced_details` entries sort by,
/// so the derived ordering is that order, `None` first. §FS-rhei-budgets.10
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PricingContext {
    /// `file`, `builtin`, `profiles` or `none`. §FS-rhei-budgets.5.2
    pub price_book_source: Option<String>,
    pub price_book_id: Option<String>,
    /// The caller-owned book's path, never the run's copy of it.
    pub price_book_path: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// `true` only where a known pair was checked and the book had no entry.
    pub missing_entry: Option<bool>,
}

impl PricingContext {
    /// The receipt member, every key present and `null` where unknown.
    /// §FS-rhei-budgets.5.2
    pub fn to_json(&self) -> Value {
        json!({
            "provider": self.provider,
            "model": self.model,
            "price_book_id": self.price_book_id,
            "price_book_path": self.price_book_path,
            "price_book_source": self.price_book_source,
            "missing_entry": self.missing_entry,
        })
    }

    /// Read a receipt's member back. Absent, or not an object, is the unknown
    /// context: a receipt written before contexts existed. A member of the
    /// wrong type is unknown too, never a guess. §FS-rhei-budgets.5.2
    pub fn from_json(value: Option<&Value>) -> Self {
        let Some(value) = value.filter(|value| value.is_object()) else {
            return Self::default();
        };
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        Self {
            price_book_source: text("price_book_source"),
            price_book_id: text("price_book_id"),
            price_book_path: text("price_book_path"),
            provider: text("provider"),
            model: text("model"),
            missing_entry: value.get("missing_entry").and_then(Value::as_bool),
        }
    }
}

/// One distinct pricing context and how many of the day's unpriced receipts
/// share it. The counts of a day's details sum to its `unpriced` mark.
/// §FS-rhei-budgets.10
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnpricedDetail {
    pub context: PricingContext,
    pub count: u64,
}

impl UnpricedDetail {
    /// One `unpriced_details` entry: the six context members plus `count`.
    /// §FS-rhei-budgets.10 §FS-rhei-run-json.2.1
    pub fn to_json(&self) -> Value {
        let mut value = self.context.to_json();
        value["count"] = self.count.into();
        value
    }

    /// Read one entry back; `None` where it carries no count.
    pub fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            context: PricingContext::from_json(Some(value)),
            count: value.get("count")?.as_u64()?,
        })
    }
}

/// The details as the JSON surfaces carry them, in their order.
/// §FS-rhei-budgets.10 §FS-rhei-run-json.2.1
pub fn details_json(details: &[UnpricedDetail]) -> Value {
    Value::Array(details.iter().map(UnpricedDetail::to_json).collect())
}

/// Fold a day's contexts into details, one per distinct context, ordered.
pub(crate) fn tally(contexts: BTreeMap<PricingContext, u64>) -> Vec<UnpricedDetail> {
    contexts.into_iter().map(|(context, count)| UnpricedDetail { context, count }).collect()
}

/// A book group's key: what the text says once per line. A context with no
/// source is the unknown group whatever else it holds. §FS-rhei-budgets.8.1
type GroupKey = (Option<String>, Option<String>, Option<String>, Option<bool>);

fn group_key(context: &PricingContext) -> GroupKey {
    if context.price_book_source.is_none() {
        return (None, None, None, None);
    }
    (
        context.price_book_source.clone(),
        context.price_book_id.clone(),
        context.price_book_path.clone(),
        context.missing_entry,
    )
}

/// `price book <id> (<where>)`, the way the halt and the validate warning
/// both write a book. `None` for a source that names no book.
/// §FS-rhei-budgets.8.1 §FS-rhei-validate.4.6
pub fn book_phrase(source: &str, id: Option<&str>, path: Option<&str>) -> Option<String> {
    let place = match source {
        "file" => path.unwrap_or("an unknown path").to_string(),
        "builtin" => "built-in".to_string(),
        "profiles" => "composed from profile prices".to_string(),
        _ => return None,
    };
    Some(format!("price book {} ({place})", id.unwrap_or("with no id")))
}

/// The continuation lines' text, one per book group in their order, without
/// indentation: each surface aligns them under its own value column, or joins
/// them into one line where it cannot hold several. §FS-rhei-budgets.8.1
pub fn group_lines(details: &[UnpricedDetail]) -> Vec<String> {
    let mut groups: BTreeMap<GroupKey, BTreeMap<Option<(String, String)>, u64>> = BTreeMap::new();
    for detail in details {
        let pair = match (&detail.context.provider, &detail.context.model) {
            (Some(provider), Some(model)) => Some((provider.clone(), model.clone())),
            _ => None,
        };
        let count = groups.entry(group_key(&detail.context)).or_default().entry(pair).or_default();
        *count = count.saturating_add(detail.count);
    }
    groups.into_iter().map(|(key, pairs)| group_line(&key, &pairs)).collect()
}

fn group_line(key: &GroupKey, pairs: &BTreeMap<Option<(String, String)>, u64>) -> String {
    let (source, id, path, missing) = key;
    let Some(source) = source.as_deref() else {
        let count = pairs.values().fold(0u64, |sum, count| sum.saturating_add(*count));
        return format!("{count} with no pricing context (settled before rhei recorded one)");
    };
    // An unknown model sorts first, as `None` does. §FS-rhei-budgets.8.1
    let written: Vec<String> = pairs
        .iter()
        .map(|(pair, count)| match pair {
            Some((provider, model)) => format!("{provider}/{model} ({count})"),
            None => format!("an unknown model ({count})"),
        })
        .collect();
    let listed = written.join(", ");
    let Some(book) = book_phrase(source, id.as_deref(), path.as_deref()) else {
        return format!("{listed}: no price book was selected");
    };
    match missing {
        Some(true) => {
            let verb = if pairs.len() == 1 { "has" } else { "have" };
            format!("{listed} {verb} no entry in {book}")
        }
        Some(false) => {
            format!("{listed}: {book} has an entry, but no measured usage could be priced")
        }
        None => format!("{listed}: could not be checked against {book}"),
    }
}

impl Journal {
    /// [`Journal::settle_spend`], keeping what the settle knows about an
    /// unpriced invocation: its receipt carries the pricing context, so the
    /// journal alone names the pair and the book. A context on any other basis
    /// is dropped, and carrying one changes neither the amount nor the mark.
    /// §FS-rhei-budgets.6.2 §FS-rhei-budgets.5.2 §FS-rhei-budgets.8.1
    pub fn settle_spend_in_context(
        &mut self,
        reservation: &str,
        amount_micro: u64,
        currency: &str,
        basis: SpendBasis,
        context: Option<&PricingContext>,
        audit: &Audit,
    ) -> Result<()> {
        let mut payload = json!({
            "reservation_id": reservation,
            "amount_micro": amount_micro,
            "currency": currency,
            "basis": basis.as_str(),
        });
        if let (SpendBasis::Unpriced, Some(context)) = (basis, context) {
            payload["pricing_context"] = context.to_json();
        }
        self.append("spend", payload, audit)
    }
}
