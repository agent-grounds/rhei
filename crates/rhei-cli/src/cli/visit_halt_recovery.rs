// One recovery clause across scheduling and completed attempts. §FS-rhei-agents.5.2.1

/// The halt an exhausted budget prints, wherever it is printed from.
///
/// Two moments reach it: the pass that declines to spawn because the budget is
/// gone, and the attempt that spent the last of it, which knows one run earlier
/// that no later pass will run the state again. One function so the operator
/// reads one sentence rather than two that disagree about what happens next.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
fn budget_spent_halt_line(task_id: &str, state_name: &str, budget: u64, owed: &str) -> String {
    render_spent_visit_halt(
        task_id,
        state_name,
        budget,
        owed,
        HaltRecovery::Visit { configured: budget, checkpoints: None },
    )
}

/// What the run will do about a ticket whose attempt just stalled.
///
/// The stall message predicts engine behaviour, so it has to be conditioned on
/// the thing that decides that behaviour. Conditioning it on the completion
/// condition alone made the run promise a retry it had already ruled out — the
/// same class of untruth as the result stub that said no agent had run.
// §FS-rhei-agents.3.2.1
#[derive(Clone, Copy)]
enum RetryOutlook {
    /// A later pass will spawn this invocation again.
    AttemptsLeft,
    /// The attempt that just finished was the last one this visit gets.
    BudgetSpent { budget: u64 },
    /// Effective spent allowance and configured recovery grant. §FS-rhei-retry.3
    BudgetSpentWithRecovery { budget: u64, recovery: HaltRecovery },
}

impl RetryOutlook {
    /// The halt line for this outlook, given what the completion condition is
    /// still owed. §FS-rhei-agents.3.2.1
    fn halt_line(self, task_id: &str, state_name: &str, missing: &[String]) -> String {
        match self {
            RetryOutlook::AttemptsLeft => format!(
                "  halting Task {task_id} in state '{state_name}': the completion condition is \
                 not met, so no transition fires; a later pass runs the state again."
            ),
            RetryOutlook::BudgetSpent { budget } => {
                budget_spent_halt_line(task_id, state_name, budget, &completion_debt_label(missing))
            }
            RetryOutlook::BudgetSpentWithRecovery { budget, recovery } => render_spent_visit_halt(
                task_id,
                state_name,
                budget,
                &completion_debt_label(missing),
                recovery,
            ),
        }
    }
}

/// The configured refill and supervisor context, independent of effective spend.
/// Poll exhaustion offers no visit grant. §FS-rhei-retry.3
#[derive(Clone, Copy)]
enum HaltRecovery {
    Visit { configured: u64, checkpoints: Option<usize> },
    Poll,
}

/// Preserve the existing ending verbatim and append the applicable recovery.
/// §FS-rhei-agents.5.2.1 §FS-rhei-retry.3
fn render_spent_visit_halt(
    task: &str,
    state: &str,
    spent: u64,
    owed: &str,
    recovery: HaltRecovery,
) -> String {
    let mut line = format!("  halting Task {task} in state '{state}': {spent} attempts spent on this visit and the completion condition is still unmet: {owed}. The ticket stays in '{state}'.");
    match recovery {
        HaltRecovery::Visit { configured, checkpoints: None } => line.push_str(&format!(" To give this visit {configured} more attempts, run: rhei retry {task}.")),
        HaltRecovery::Visit { configured, checkpoints: Some(count) } => line.push_str(&format!(" Its subtree is still held with {count} unread checkpoint{}. To give this visit {configured} more attempts and keep the hold, run: rhei retry {task} (its self-loop would release the subtree instead).", if count == 1 { "" } else { "s" })),
        HaltRecovery::Poll => {},
    }
    line
}

/// Read supervisor events from the same loaded plan used to schedule the task.
/// §FS-rhei-retry.3 §FS-rhei-supervision.3.5
fn visit_halt_recovery(
    budget: AttemptBudget,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
) -> HaltRecovery {
    match budget {
        AttemptBudget::Visit(configured) => HaltRecovery::Visit {
            configured,
            checkpoints: task_is_supervising(task, machine)
                .then(|| supervision_checkpoints(metadata, &task.id).len()),
        },
        AttemptBudget::Poll { .. } => HaltRecovery::Poll,
    }
}

impl RetryOutlook {
    /// Attach context before an attempt leaves the scheduling thread. §FS-rhei-agents.5.2.1
    fn with_recovery(self, recovery: HaltRecovery) -> Self {
        match self {
            Self::BudgetSpent { budget } | Self::BudgetSpentWithRecovery { budget, .. } => {
                Self::BudgetSpentWithRecovery { budget, recovery }
            }
            left => left,
        }
    }
}
