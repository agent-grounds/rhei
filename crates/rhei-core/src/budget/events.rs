//! Budget events are projections of durable receipts, never financial authority.
//! §FS-rhei-budgets.10

use super::{Journal, Receipt, Result, Snapshot};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BudgetEvent {
    BudgetSnapshot {
        budget: Snapshot,
    },
    BudgetReserved {
        receipt: Box<Receipt>,
        budget: Snapshot,
    },
    BudgetStarted {
        receipt: Box<Receipt>,
        budget: Snapshot,
    },
    BudgetContained {
        receipt: Box<Receipt>,
        budget: Snapshot,
    },
    BudgetSettled {
        receipt: Box<Receipt>,
        budget: Snapshot,
    },
    BudgetReleased {
        receipt: Box<Receipt>,
        budget: Snapshot,
    },
    BudgetBreach {
        receipt: Box<Receipt>,
        budget: Snapshot,
    },
    BudgetHalt {
        project_id: Option<String>,
        task_id: Option<String>,
        ticket_identity: Option<String>,
        reason_code: String,
        message: String,
        budget: Option<Snapshot>,
        dry_run: bool,
    },
}

impl BudgetEvent {
    pub fn snapshot(&self) -> Option<&Snapshot> {
        match self {
            Self::BudgetSnapshot { budget }
            | Self::BudgetReserved { budget, .. }
            | Self::BudgetStarted { budget, .. }
            | Self::BudgetContained { budget, .. }
            | Self::BudgetSettled { budget, .. }
            | Self::BudgetReleased { budget, .. }
            | Self::BudgetBreach { budget, .. } => Some(budget),
            Self::BudgetHalt { budget, .. } => budget.as_ref(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::BudgetHalt { task_id, reason_code, message, .. } => format!(
                "Budget halt {} [{reason_code}]: {message}; inspect `rhei budget show`",
                task_id.as_deref().unwrap_or("project"),
            ),
            Self::BudgetReserved { receipt, .. } => {
                format!("Budget reserved: {}", receipt.receipt_id)
            }
            Self::BudgetStarted { receipt, .. } => {
                format!("Budget invocation started: {}", receipt.receipt_id)
            }
            Self::BudgetContained { receipt, .. } => {
                format!("Budget containment: {}", receipt.payload["reason"])
            }
            Self::BudgetSettled { receipt, .. } => {
                format!("Budget settled: {}", receipt.receipt_id)
            }
            Self::BudgetReleased { receipt, .. } => {
                format!("Budget released: {}", receipt.receipt_id)
            }
            Self::BudgetBreach { receipt, .. } => {
                format!("Budget breach: {}; requalification required", receipt.receipt_id)
            }
            Self::BudgetSnapshot { budget } => budget.summary(),
        }
    }
}

/// The runtime adapts this sink to the same tee as its other run events.
/// Implementations must not reopen the locked ledger. §AR-neural-admission.3
pub trait BudgetEventSink: Send + Sync {
    fn emit(&self, event: BudgetEvent);
}

impl Journal {
    pub fn set_event_sink(&mut self, sink: Arc<dyn BudgetEventSink>) -> Result<()> {
        let snapshot = self.snapshot()?;
        self.event_sink = Some(sink.clone());
        sink.emit(BudgetEvent::BudgetSnapshot { budget: snapshot });
        Ok(())
    }

    pub(crate) fn emit_receipt(&self, receipt: &Receipt, budget: Snapshot) {
        let Some(sink) = &self.event_sink else { return };
        let event = match receipt.kind.as_str() {
            "reserve" => Some(BudgetEvent::BudgetReserved {
                receipt: Box::new(receipt.clone()),
                budget: budget.clone(),
            }),
            "start" => Some(BudgetEvent::BudgetStarted {
                receipt: Box::new(receipt.clone()),
                budget: budget.clone(),
            }),
            "containment" => Some(BudgetEvent::BudgetContained {
                receipt: Box::new(receipt.clone()),
                budget: budget.clone(),
            }),
            "settle" | "reconcile" => Some(BudgetEvent::BudgetSettled {
                receipt: Box::new(receipt.clone()),
                budget: budget.clone(),
            }),
            "release" => Some(BudgetEvent::BudgetReleased {
                receipt: Box::new(receipt.clone()),
                budget: budget.clone(),
            }),
            "breach" => Some(BudgetEvent::BudgetBreach {
                receipt: Box::new(receipt.clone()),
                budget: budget.clone(),
            }),
            _ => None,
        };
        if let Some(event) = event {
            sink.emit(event);
        }
        sink.emit(BudgetEvent::BudgetSnapshot { budget });
    }
}
