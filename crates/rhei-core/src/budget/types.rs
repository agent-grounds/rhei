//! The identities every admission is expressed in, the counters a replay
//! derives, and the refusal codes a caller routes on.
//!
//! Nothing here prices, brokers, or qualifies anything: the two quantities this
//! module knows how to add up are both integer counts.
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

/// Which of the two counts a number is about. They are reported by different
/// names because they stop different things. §FS-rhei-budgets.1
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    Travel,
    Invocations,
}

impl Dimension {
    /// The label a halt prints, and the words `rhei budget show` uses.
    /// §FS-rhei-budgets.8
    pub fn label(self) -> &'static str {
        match self {
            Self::Travel => "ticket travel",
            Self::Invocations => "project invocations",
        }
    }

    /// The settings key that bounds this dimension. §FS-rhei-budgets.2.1
    pub fn settings_key(self) -> &'static str {
        match self {
            Self::Travel => "transition_limit",
            Self::Invocations => "invocations_per_day",
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
                let allowance = value
                    .get("invocations")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| BudgetError::corrupt("lifetime contract without an allowance"))?;
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
}

/// Stable refusal code plus a concrete, human-readable reason.
/// §FS-rhei-budgets.8 §AR-neural-admission.1
#[derive(Debug, Clone)]
pub struct BudgetError {
    pub reason_code: String,
    pub message: String,
    /// Present exactly when a bound is what refused. §FS-rhei-budgets.8
    pub exhaustion: Option<Exhaustion>,
}

impl BudgetError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self { reason_code: code.into(), message: message.into(), exhaustion: None }
    }

    pub(crate) fn exhausted(code: &str, message: impl Into<String>, spent: Exhaustion) -> Self {
        Self { reason_code: code.into(), message: message.into(), exhaustion: Some(spent) }
    }

    pub(crate) fn bounds(message: impl Into<String>) -> Self {
        Self::new("missing_bound", message)
    }

    pub(crate) fn corrupt(message: impl std::fmt::Display) -> Self {
        Self::new("untrustworthy_ledger", format!("budget journal is untrustworthy: {message}"))
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

impl From<std::io::Error> for BudgetError {
    fn from(error: std::io::Error) -> Self {
        Self::corrupt(error)
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
