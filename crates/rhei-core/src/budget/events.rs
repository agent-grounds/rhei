//! Budget events are projections of durable receipts, never authority.
//!
//! A surface that renders one is reading what the ledger already committed; no
//! implementation may reopen the locked ledger to answer an event.
//! §FS-rhei-budgets.9 §AR-neural-admission.3

use super::journal::Journal;
use super::types::{Contract, Dimension, SpendMarks};
use super::Result;
use crate::money;

/// One dimension's line on a live surface: what it is, what it has spent, and
/// what is left. §FS-rhei-budgets.9
#[derive(Clone, Debug)]
pub struct BudgetLine {
    pub dimension: &'static str,
    pub bound: u64,
    pub consumed: u64,
    pub outstanding: u64,
    pub remaining: u64,
    pub mode: String,
    /// The account's currency, present exactly on the line whose numbers are
    /// money rather than a count. §FS-rhei-budgets.5.5
    pub currency: Option<String>,
    /// How much of this line was charged at the worst case rather than
    /// measured. Empty on a count. §FS-rhei-budgets.6.2
    pub marks: SpendMarks,
}

impl BudgetLine {
    fn is_money(&self) -> bool {
        self.dimension == Dimension::Spend.label()
    }

    fn write(&self, value: u64) -> String {
        if self.is_money() {
            return money::format_micro(value, self.currency.as_deref());
        }
        value.to_string()
    }

    /// The one line every surface that reports a dimension prints.
    /// §FS-rhei-budgets.9 §FS-rhei-budgets.10
    pub fn render(&self) -> String {
        format!(
            "{}: {} consumed + {} outstanding / {}; {} remaining ({})",
            self.dimension,
            self.write(self.consumed),
            self.write(self.outstanding),
            self.write(self.bound),
            self.write(self.remaining),
            self.mode
        )
    }

    /// The marks, on their own line beneath the one above, in the words the
    /// halt uses. Absent where the whole day was measured.
    /// §FS-rhei-budgets.10
    pub fn marks_line(&self) -> Option<String> {
        let written = self.marks.row(super::built_in::SPEND_RESERVE, self.currency.as_deref())?;
        Some(format!("  estimated:   {written}"))
    }
}

#[derive(Clone, Debug)]
pub enum BudgetEvent {
    /// Where every dimension stands, emitted as receipts are written rather
    /// than only at exhaustion. §FS-rhei-budgets.9
    BudgetSnapshot { lines: Vec<BudgetLine> },
    /// A bound refused a spawn, with the full halt text the run prints.
    /// §FS-rhei-budgets.8
    BudgetHalt { task_id: Option<String>, reason_code: String, halt: String },
}

impl BudgetEvent {
    pub fn message(&self) -> String {
        match self {
            Self::BudgetHalt { halt, .. } => halt.clone(),
            Self::BudgetSnapshot { lines } => {
                lines.iter().map(BudgetLine::render).collect::<Vec<_>>().join("; ")
            }
        }
    }
}

impl Journal {
    /// A snapshot projection for the run's live surfaces, given the bounds the
    /// caller resolved.
    ///
    /// `is_run_gone` is what tells an unsettled amount from one still in
    /// flight, and it is the caller's because the proof is a lock on another
    /// run's execution root. §FS-rhei-budgets.9 §FS-rhei-budgets.6.2
    pub fn lines(
        &self,
        ticket: Option<&str>,
        per_day: u64,
        travel_bound: u64,
        spend_per_day: u64,
        is_run_gone: &dyn Fn(&str) -> bool,
    ) -> Result<Vec<BudgetLine>> {
        let snapshot = self.snapshot()?;
        let invocation_bound = snapshot.invocation_bound(per_day);
        let mut lines = vec![BudgetLine {
            dimension: Dimension::Invocations.label(),
            bound: invocation_bound,
            consumed: snapshot.invocations.consumed,
            outstanding: snapshot.invocations.reserved,
            remaining: snapshot.invocations.remaining(invocation_bound)?,
            mode: match snapshot.contract {
                Contract::Window => format!("window ({}Z)", snapshot.day),
                Contract::Lifetime { .. } => "lifetime".into(),
            },
            currency: None,
            marks: SpendMarks::default(),
        }];
        // Always the day, whichever invocation contract the account holds.
        // §FS-rhei-budgets.3.4
        lines.push(BudgetLine {
            dimension: Dimension::Spend.label(),
            bound: spend_per_day,
            consumed: snapshot.spend.consumed,
            outstanding: snapshot.spend.reserved,
            remaining: snapshot.spend.remaining(spend_per_day)?,
            mode: format!("window ({}Z)", snapshot.day),
            currency: snapshot.currency.clone(),
            marks: snapshot.marks(is_run_gone),
        });
        if let Some(ticket) = ticket {
            let travel = snapshot.travel_for(ticket);
            lines.push(BudgetLine {
                dimension: Dimension::Travel.label(),
                bound: travel_bound,
                consumed: travel.consumed,
                outstanding: travel.reserved,
                remaining: travel.remaining(travel_bound)?,
                mode: "per ticket identity".into(),
                currency: None,
                marks: SpendMarks::default(),
            });
        }
        Ok(lines)
    }
}
