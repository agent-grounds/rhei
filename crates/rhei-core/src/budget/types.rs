//! Checked allowance arithmetic and the public inspection snapshot.
//! §FS-rhei-budgets.1 §FS-rhei-budgets.10

use super::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Exact money. A currency is never silently converted. §FS-rhei-budgets.1
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Money {
    pub currency: String,
    pub amount_micro: u64,
}

impl Money {
    /// Positive authored amounts only; zero is reserved for accounting.
    /// §FS-rhei-budgets.2.1
    pub fn validate(&self) -> Result<()> {
        if self.amount_micro == 0 {
            return Err(BudgetError::bounds("money must be a positive integer"));
        }
        // SIX ISO-4217 List One, retrieved 2026-09-18; XTS/XXX cannot
        // denominate provider charges. §FS-rhei-budgets.1
        const CURRENCIES: &str = "AED AFN ALL AMD AOA ARS AUD AWG AZN BAM BBD BDT BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRU MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SOS SRD SSP STN SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UYW UZS VED VES VND VUV WST XAD XAF XAG XAU XBA XBB XBC XBD XCD XCG XDR XOF XPD XPF XPT XSU XUA YER ZAR ZMW ZWG";
        if !CURRENCIES.split_whitespace().any(|c| c == self.currency) {
            return Err(BudgetError::bounds("currency must be a supported ISO-4217 code"));
        }
        Ok(())
    }
}

/// The immutable unit pair used at every admission boundary. §FS-rhei-budgets.1
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Allowance {
    pub invocations: u64,
    pub spend: Money,
}

impl Allowance {
    pub fn validate(&self) -> Result<()> {
        self.spend.validate()?;
        if self.invocations == 0 {
            return Err(BudgetError::bounds("invocations must be a positive integer"));
        }
        Ok(())
    }
}

/// Consumption and outstanding claims are distinct even after a crash.
/// §FS-rhei-budgets.7
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Counter {
    pub consumed: u64,
    pub reserved: u64,
}

impl Counter {
    pub fn exposure(&self) -> Result<u64> {
        add(self.consumed, self.reserved)
    }
}

/// A ledger view; breach overages remain visible, never clamped to a ceiling.
/// §FS-rhei-budgets.9 §FS-rhei-budgets.10
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snapshot {
    #[serde(default)]
    pub ledger_health: String,
    pub project_id: String,
    pub allowance: Allowance,
    pub consumed: Allowance,
    pub reserved: Allowance,
    pub remaining: Allowance,
    pub travel: BTreeMap<String, Counter>,
    #[serde(default)]
    pub travel_limits: BTreeMap<String, u64>,
    #[serde(default)]
    pub travel_remaining: BTreeMap<String, u64>,
    pub reservations: BTreeMap<String, serde_json::Value>,
    pub breached: bool,
    pub audit: Vec<serde_json::Value>,
}

impl Snapshot {
    /// Resolution adds current profile limits without touching the ledger.
    /// §FS-rhei-budgets.10
    pub fn set_travel_limit(&mut self, ticket: &str, ceiling: u64) -> Result<()> {
        let travel = self.travel.entry(ticket.into()).or_default();
        self.travel_remaining.insert(ticket.into(), ceiling.saturating_sub(travel.exposure()?));
        self.travel_limits.insert(ticket.into(), ceiling);
        Ok(())
    }

    pub fn summary(&self) -> String {
        format!("Panta lifetime budget {}: invocations {} consumed + {} reserved / {}; {} remaining; spend {} consumed + {} reserved / {} micro-{}; {} remaining (verified history)",
            self.project_id, self.consumed.invocations, self.reserved.invocations,
            self.allowance.invocations, self.remaining.invocations,
            self.consumed.spend.amount_micro, self.reserved.spend.amount_micro,
            self.allowance.spend.amount_micro, self.allowance.spend.currency,
            self.remaining.spend.amount_micro)
    }

    /// Shared textual facts for inspection, TUI, and the dashboard.
    /// §FS-rhei-budgets.10
    pub fn detail_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Panta lifetime budget {} (verified history)", self.project_id),
            format!(
                "Invocations: {} consumed + {} reserved / {}; {} remaining",
                self.consumed.invocations,
                self.reserved.invocations,
                self.allowance.invocations,
                self.remaining.invocations
            ),
            format!(
                "Spend: {} consumed + {} reserved / {} micro-{}; {} remaining",
                self.consumed.spend.amount_micro,
                self.reserved.spend.amount_micro,
                self.allowance.spend.amount_micro,
                self.allowance.spend.currency,
                self.remaining.spend.amount_micro
            ),
        ];
        for (ticket, travel) in &self.travel {
            lines.push(format!(
                "Travel {ticket}: {} applied + {} reserved / {}; {} remaining",
                travel.consumed,
                travel.reserved,
                self.travel_limits
                    .get(ticket)
                    .map(u64::to_string)
                    .unwrap_or_else(|| "unknown".into()),
                self.travel_remaining
                    .get(ticket)
                    .map(u64::to_string)
                    .unwrap_or_else(|| "unknown".into())
            ));
        }
        lines.extend(self.reservation_lines());
        if self.breached {
            lines.push("Budget breach retained; requalification required".into());
        }
        lines
    }

    pub fn reservation_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (id, row) in &self.reservations {
            lines.push(format!(
                "{id}: T={} R={} FWC={} settled={} micro-{}; grade={}; state={}; ancestor={}",
                row["threshold_micro"],
                row["residual_micro"],
                row["fwc_micro"],
                row["settled_charge_micro"],
                self.allowance.spend.currency,
                row["qualification_grade"],
                row["containment_state"],
                row["parent_reservation"]
            ));
            lines.push(format!(
                "Qualification: {}; evidence={}",
                row["qualification"], row["qualification_evidence_hash"]
            ));
        }
        lines
    }
}

/// Stable refusal code plus a concrete, human-readable reason.
/// §FS-rhei-budgets.9
#[derive(Debug, Clone, Serialize)]
pub struct BudgetError {
    pub reason_code: String,
    pub message: String,
}

impl BudgetError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self { reason_code: code.into(), message: message.into() }
    }

    pub(crate) fn bounds(message: &str) -> Self {
        Self::new("missing_bound", message)
    }

    pub(crate) fn corrupt(message: impl std::fmt::Display) -> Self {
        Self::new("untrustworthy_ledger", format!("budget journal is untrustworthy: {message}"))
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

pub(crate) fn uuid(value: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value).map_err(BudgetError::corrupt)?;
    if parsed.to_string() != value || parsed.get_variant() != uuid::Variant::RFC4122 {
        return Err(BudgetError::corrupt("identity must be a lowercase RFC 4122 UUID"));
    }
    Ok(())
}
