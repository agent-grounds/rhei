//! Budget events are projections of durable receipts, never authority.
//!
//! A surface that renders one is reading what the ledger already committed; no
//! implementation may reopen the locked ledger to answer an event.
//! §FS-rhei-budgets.9 §AR-neural-admission.3

use super::journal::Journal;
use super::types::Contract;
use super::Result;

/// One count's line on a live surface: what it is, what it has spent, and what
/// is left. §FS-rhei-budgets.9
#[derive(Clone, Debug)]
pub struct BudgetLine {
    pub dimension: &'static str,
    pub bound: String,
    pub consumed: u64,
    pub outstanding: u64,
    pub remaining: u64,
    pub mode: String,
}

#[derive(Clone, Debug)]
pub enum BudgetEvent {
    /// Where both counts stand, emitted as receipts are written rather than
    /// only at exhaustion. §FS-rhei-budgets.9
    BudgetSnapshot { lines: Vec<BudgetLine> },
    /// A bound refused a spawn, with the full halt text the run prints.
    /// §FS-rhei-budgets.8
    BudgetHalt { task_id: Option<String>, reason_code: String, halt: String },
}

impl BudgetEvent {
    pub fn message(&self) -> String {
        match self {
            Self::BudgetHalt { halt, .. } => halt.clone(),
            Self::BudgetSnapshot { lines } => lines
                .iter()
                .map(|line| {
                    format!(
                        "{}: {} consumed + {} outstanding / {}; {} remaining ({})",
                        line.dimension,
                        line.consumed,
                        line.outstanding,
                        line.bound,
                        line.remaining,
                        line.mode
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        }
    }
}

impl Journal {
    /// A snapshot projection for the run's live surfaces, given the bounds the
    /// caller resolved. §FS-rhei-budgets.9
    pub fn lines(
        &self,
        ticket: Option<&str>,
        per_day: u64,
        travel_bound: u64,
    ) -> Result<Vec<BudgetLine>> {
        let snapshot = self.snapshot()?;
        let invocation_bound = snapshot.invocation_bound(per_day);
        let mut lines = vec![BudgetLine {
            dimension: "project invocations",
            bound: invocation_bound.to_string(),
            consumed: snapshot.invocations.consumed,
            outstanding: snapshot.invocations.reserved,
            remaining: snapshot.invocations.remaining(invocation_bound)?,
            mode: match snapshot.contract {
                Contract::Window => format!("window ({}Z)", snapshot.day),
                Contract::Lifetime { .. } => "lifetime".into(),
            },
        }];
        if let Some(ticket) = ticket {
            let travel = snapshot.travel_for(ticket);
            lines.push(BudgetLine {
                dimension: "ticket travel",
                bound: travel_bound.to_string(),
                consumed: travel.consumed,
                outstanding: travel.reserved,
                remaining: travel.remaining(travel_bound)?,
                mode: "per ticket identity".into(),
            });
        }
        Ok(lines)
    }
}
