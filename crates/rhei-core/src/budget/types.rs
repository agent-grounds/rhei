//! The identities every admission is expressed in, the counters a replay
//! derives, and the refusal codes a caller routes on.
//!
//! Nothing here prices, brokers, or qualifies anything. Two of the three
//! quantities this module adds up are integer counts; the third is an amount
//! of money in integer micro-units, read from a record written elsewhere.
//! §FS-rhei-budgets.1 §AR-neural-admission.2

use super::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Consumption and outstanding claims stay distinct even after a crash: a
/// reservation whose process may have started is neither spent nor free.
/// §FS-rhei-budgets.6.2
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Counter {
    pub consumed: u64,
    pub reserved: u64,
}

impl Counter {
    /// What the bound is checked against: `consumed + outstanding`.
    /// §FS-rhei-budgets.1
    pub fn exposure(&self) -> Result<u64> {
        add(self.consumed, self.reserved)
    }

    /// Headroom under `bound`, never a wrapped credit. §FS-rhei-budgets.8
    pub fn remaining(&self, bound: u64) -> Result<u64> {
        Ok(bound.saturating_sub(self.exposure()?))
    }
}

/// Which of the three bounds a number is about. They are reported by
/// different names because they stop different things, and only two of them
/// are counts. §FS-rhei-budgets.1
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    Travel,
    Invocations,
    /// The day's measured spend, in micro-units of the account's currency.
    /// §FS-rhei-budgets.3.4
    Spend,
}

impl Dimension {
    /// The label a halt prints, and the words `rhei budget show` uses.
    /// §FS-rhei-budgets.8
    pub fn label(self) -> &'static str {
        match self {
            Self::Travel => "ticket travel",
            Self::Invocations => "project invocations",
            Self::Spend => "project spend",
        }
    }

    /// The settings key that bounds this dimension. §FS-rhei-budgets.2.1
    pub fn settings_key(self) -> &'static str {
        match self {
            Self::Travel => "transition_limit",
            Self::Invocations => "invocations_per_day",
            Self::Spend => "spend_per_day",
        }
    }

    /// Whether this dimension's numbers are money rather than a count, which
    /// is what decides how every surface writes them. §FS-rhei-budgets.1
    pub fn is_money(self) -> bool {
        matches!(self, Self::Spend)
    }
}

/// How much of a day's spend was charged at the worst case rather than
/// measured, told apart by *why*.
///
/// Three marks rather than one number, because the three send a reader
/// somewhere different: `unpriced` wants a price-book entry, `unmeasurable`
/// wants an accounting extractor for the agent, and `unsettled` is a run that
/// went away before it could say what it spent. A day with none of them was
/// measured outright and says nothing at all. §FS-rhei-budgets.6.2
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpendMarks {
    pub unpriced: u64,
    pub unmeasurable: u64,
    pub unsettled: u64,
}

impl SpendMarks {
    pub fn any(&self) -> bool {
        self.unpriced > 0 || self.unmeasurable > 0 || self.unsettled > 0
    }

    /// `2 unpriced, 1 unsettled` — a mark with no members is left out rather
    /// than printed as a zero. `None` where the whole day was measured.
    /// §FS-rhei-budgets.8 §FS-rhei-budgets.10
    pub fn phrase(&self) -> Option<String> {
        let members = [
            (self.unpriced, "unpriced"),
            (self.unmeasurable, "unmeasurable"),
            (self.unsettled, "unsettled"),
        ];
        let written: Vec<String> = members
            .iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, name)| format!("{count} {name}"))
            .collect();
        (!written.is_empty()).then(|| written.join(", "))
    }

    /// The whole row, in the words §FS-rhei-budgets.8 fixes, given the worst
    /// case each marked invocation was charged.
    pub fn row(&self, reserve: u64, currency: Option<&str>) -> Option<String> {
        let written = self.phrase()?;
        Some(format!(
            "{written} (charged at {} each)",
            crate::money::format_micro(reserve, currency)
        ))
    }
}

/// What a settle said about one invocation's cost, and why.
///
/// `Measured` is the only one that revises the reserve; the other two leave it
/// standing as the charge, because an amount nobody could measure is not an
/// amount of nothing. §FS-rhei-budgets.6.2
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpendBasis {
    Measured,
    Unpriced,
    Unmeasurable,
}

impl SpendBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Unpriced => "unpriced",
            Self::Unmeasurable => "unmeasurable",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "measured" => Some(Self::Measured),
            "unpriced" => Some(Self::Unpriced),
            "unmeasurable" => Some(Self::Unmeasurable),
            _ => None,
        }
    }
}

/// The accounting contract a project's account holds. Exactly one at a time,
/// and no surface describes one as the other. §FS-rhei-budgets.3
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Contract {
    /// The default: a rate, bounded per UTC calendar day.
    Window,
    /// What `rhei budget init` establishes: a persistent lifetime total.
    Lifetime { allowance: u64 },
}

impl Contract {
    pub fn is_lifetime(&self) -> bool {
        matches!(self, Self::Lifetime { .. })
    }

    /// The word `rhei budget show` and the run report use for this contract.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Lifetime { .. } => "lifetime",
        }
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Window => serde_json::json!({"mode": "window"}),
            Self::Lifetime { allowance } => {
                serde_json::json!({"mode": "lifetime", "invocations": allowance})
            }
        }
    }

    pub(crate) fn from_json(value: &serde_json::Value) -> Result<Self> {
        match value.get("mode").and_then(serde_json::Value::as_str) {
            Some("window") => Ok(Self::Window),
            Some("lifetime") => {
                let allowance =
                    value.get("invocations").and_then(serde_json::Value::as_u64).ok_or_else(
                        || BudgetError::corrupt("lifetime contract without an allowance"),
                    )?;
                if allowance == 0 {
                    return Err(BudgetError::corrupt("a lifetime allowance must be positive"));
                }
                Ok(Self::Lifetime { allowance })
            }
            _ => Err(BudgetError::corrupt("unknown accounting contract")),
        }
    }
}

/// The ledger view every surface renders from. The bounds themselves are *not*
/// in here: they resolve from settings at read time, so lowering a ceiling
/// rewrites nothing. §AR-neural-admission.5
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub project_id: String,
    pub contract: Contract,
    /// The day key this snapshot was derived under, guarded against a clock
    /// moved backwards. §FS-rhei-budgets.3.3
    pub day: String,
    /// Invocations under the **active** contract: today's under a window,
    /// every one ever under a lifetime allowance.
    pub invocations: Counter,
    /// Every invocation the account has ever admitted, whatever the contract.
    /// `rhei budget show` reports both so a window account can still say what
    /// it has spent in total. §FS-rhei-budgets.10
    pub lifetime_invocations: Counter,
    /// Applied and held travel, per ticket identity. §FS-rhei-budgets.4.1
    pub travel: BTreeMap<String, Counter>,
    /// The day's measured spend in micro-units: settled amounts consumed,
    /// unsettled reserves outstanding. Always the **day**, whichever
    /// invocation contract the account holds. §FS-rhei-budgets.3.4
    pub spend: Counter,
    /// The account's currency, absent until the first receipt carrying an
    /// amount fixed it. §FS-rhei-budgets.5.5
    pub currency: Option<String>,
    /// How much of the day was charged at the worst case rather than
    /// measured. `unsettled` is not in here: it needs the owning run's lock
    /// to tell a dead reserve from one in flight, which is a question the
    /// chain alone cannot answer. §FS-rhei-budgets.6.2
    pub spend_marks: SpendMarks,
    /// The execution roots of started reservations still holding a spend
    /// reserve, from which a caller that can take those locks derives the
    /// `unsettled` mark. §FS-rhei-budgets.6.2
    pub spend_unsettled_roots: Vec<String>,
    pub reservations: BTreeMap<String, serde_json::Value>,
    pub health: &'static str,
}

impl Snapshot {
    pub fn travel_for(&self, ticket: &str) -> Counter {
        self.travel.get(ticket).copied().unwrap_or_default()
    }

    /// The invocation bound in force: the ledger's own allowance under a
    /// lifetime contract, the day's resolved rate under a window.
    /// §FS-rhei-budgets.3
    pub fn invocation_bound(&self, per_day: u64) -> u64 {
        match &self.contract {
            Contract::Window => per_day,
            Contract::Lifetime { allowance } => *allowance,
        }
    }

    /// The three marks, with `unsettled` resolved by the same "is the owning
    /// run gone" proof the release step performs: a started reserve whose run
    /// is still live is in flight, not unsettled. §FS-rhei-budgets.6.2
    pub fn marks(&self, is_run_gone: &dyn Fn(&str) -> bool) -> SpendMarks {
        let unsettled = self.spend_unsettled_roots.iter().filter(|root| is_run_gone(root)).count();
        SpendMarks { unsettled: unsettled as u64, ..self.spend_marks }
    }
}

/// What a refused admission knows about the bound that stopped it, so the
/// caller can render the halt without re-deriving any of it.
/// §FS-rhei-budgets.8
#[derive(Clone, Debug)]
pub struct Exhaustion {
    pub dimension: Dimension,
    /// The ticket's display id, or the project's name.
    pub subject: String,
    pub counter: Counter,
    pub bound: u64,
    pub contract: Contract,
    pub day: String,
    /// The account's currency, present exactly when the refused dimension's
    /// numbers are money. §FS-rhei-budgets.5.5
    pub currency: Option<String>,
}

/// Stable refusal code plus a concrete, human-readable reason.
/// §FS-rhei-budgets.8 §AR-neural-admission.1
#[derive(Debug, Clone)]
pub struct BudgetError {
    pub reason_code: String,
    pub message: String,
    /// Present exactly when a bound is what refused.
    ///
    /// Boxed because every refusal in this module travels in a `Result` and
    /// most of them carry none: an unboxed variant would make the ordinary
    /// `Ok` path pay for the rare one. §FS-rhei-budgets.8
    pub exhaustion: Option<Box<Exhaustion>>,
}

impl BudgetError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self { reason_code: code.into(), message: message.into(), exhaustion: None }
    }

    pub(crate) fn exhausted(code: &str, message: impl Into<String>, spent: Exhaustion) -> Self {
        Self {
            reason_code: code.into(),
            message: message.into(),
            exhaustion: Some(Box::new(spent)),
        }
    }

    pub(crate) fn bounds(message: impl Into<String>) -> Self {
        Self::new("missing_bound", message)
    }

    pub(crate) fn corrupt(message: impl std::fmt::Display) -> Self {
        Self::new("untrustworthy_ledger", format!("budget journal is untrustworthy: {message}"))
    }

    /// A path this module needed and could not reach, which says nothing about
    /// whether the account is sound.
    ///
    /// Separate from [`Self::corrupt`] because the two send an operator to
    /// different places: `untrustworthy_ledger` means a chain failed to verify
    /// and the witness is how it is restored, while this one means the file or
    /// directory was not there — and so it names it, rather than blaming a
    /// ledger that is fine. §FS-rhei-budgets.5.4
    pub(crate) fn unreachable(path: &std::path::Path, error: &std::io::Error) -> Self {
        Self::new(
            "unreachable_budget_path",
            format!("cannot reach the budget path {}: {error}", path.display()),
        )
    }

    /// Whether this refusal is a spent bound rather than a broken account.
    pub fn is_exhaustion(&self) -> bool {
        self.exhaustion.is_some()
    }
}

impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BudgetError {}

/// An io failure carries no path, so it cannot claim a particular ledger is
/// damaged: it reports that the account could not be read or written, and the
/// sites that do know the path say which one with
/// [`BudgetError::unreachable`]. §FS-rhei-budgets.5.4
impl From<std::io::Error> for BudgetError {
    fn from(error: std::io::Error) -> Self {
        Self::new(
            "unreachable_budget_path",
            format!("the project's budget account could not be read or written: {error}"),
        )
    }
}

impl From<serde_json::Error> for BudgetError {
    fn from(error: serde_json::Error) -> Self {
        Self::corrupt(error)
    }
}

/// No saturation or wrapping may manufacture capacity. §FS-rhei-budgets.1
pub(crate) fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| BudgetError::corrupt("budget arithmetic overflow"))
}

/// Every identity this module mints and accepts is a lowercase RFC 4122 UUID,
/// so a hand-written one is a refusal rather than a new account.
pub(crate) fn uuid(value: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value).map_err(BudgetError::corrupt)?;
    if parsed.to_string() != value || parsed.get_variant() != uuid::Variant::RFC4122 {
        return Err(BudgetError::corrupt("identity must be a lowercase RFC 4122 UUID"));
    }
    Ok(())
}
