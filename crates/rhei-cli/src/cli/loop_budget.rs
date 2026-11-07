// What a refused counted-loop re-entry was measured against: which state's
// budget the check consulted, which kind it was, and how far it is spent.
// §FS-rhei-errors.1.5

/// Which budget a counted-loop re-entry was weighed against. The two are
/// mutually exclusive and are raised by different keys, so the refusal says
/// which one it measured rather than leaving the reader to guess the field.
/// §FS-rhei-errors.1.5
#[derive(Debug, Clone, Copy)]
enum LoopBudgetKind {
    /// A state's `visits:` budget, counted in visits.
    Visits,
    /// A poll state's `poll.max_attempts:` budget, counted in attempts.
    PollAttempts,
}

impl LoopBudgetKind {
    /// The word naming the budget, matching the key that raises it.
    fn noun(self) -> &'static str {
        match self {
            Self::Visits => "visit",
            Self::PollAttempts => "poll",
        }
    }

    /// The unit the budget is counted in, for the `used/limit` figure.
    fn unit(self) -> &'static str {
        match self {
            Self::Visits => "visits",
            Self::PollAttempts => "attempts",
        }
    }
}

/// A loop budget that is already spent: the state it belongs to, which kind it
/// was, and how far it is spent. Carried out of the check that measured it so
/// the refusal cannot re-derive the subject and arrive at a different state.
/// §FS-rhei-errors.1.5
#[derive(Debug, Clone)]
struct SpentLoopBudget {
    state: String,
    kind: LoopBudgetKind,
    used: u64,
    limit: u64,
}

impl SpentLoopBudget {
    /// Report the budget only when it is exhausted; `None` means the re-entry
    /// is still permitted.
    fn when_exhausted(
        state: &str,
        kind: LoopBudgetKind,
        used: u64,
        limit: u64,
    ) -> Option<SpentLoopBudget> {
        (used >= limit).then(|| SpentLoopBudget {
            state: state.to_string(),
            kind,
            used,
            limit,
        })
    }
}

/// The wording is normative: §FS-rhei-errors.1.5 fixes both the subject and the
/// `used/limit` figure, so changing this string is a specification change.
impl std::fmt::Display for SpentLoopBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} budget for state '{}' is exhausted ({}/{} {})",
            self.kind.noun(),
            self.state,
            self.used,
            self.limit,
            self.kind.unit()
        )
    }
}

/// Measure the budget a loop re-entry would spend, and report it when it is
/// already spent.
///
/// Two budgets can block a re-entry and they belong to different states: a
/// self-loop into a `poll:` state spends that state's `poll.max_attempts`,
/// while every other loop-back spends the **destination** state's `visits:`
/// (§FS-rhei-transitions.4.3) — which, looping back out of a gate, is not the
/// state the task is sitting in. The one actually measured is returned so the
/// refusal names it instead of naming wherever the caller stood.
/// §FS-rhei-errors.1.5
fn spent_loop_budget(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    to_state: &str,
) -> Option<SpentLoopBudget> {
    if current_state == to_state {
        if let Some(poll) = machine.states.get(current_state).and_then(|def| def.poll.as_ref()) {
            let used = current_state_visit_count(
                metadata,
                task_id,
                current_state,
                current_state_raw,
                machine,
            );
            return SpentLoopBudget::when_exhausted(
                to_state,
                LoopBudgetKind::PollAttempts,
                used,
                u64::from(poll.max_attempts),
            );
        }
    }

    let limit = state_visit_limit(machine, to_state)?;

    let mut used = task_visit_count(metadata, task_id, to_state);
    if current_state == to_state {
        used = used.max(raw_state_visit_count(current_state_raw, machine, to_state));
    }
    SpentLoopBudget::when_exhausted(to_state, LoopBudgetKind::Visits, used, limit)
}

/// The yes-or-no of [`spent_loop_budget`], for the callers that gate on a
/// re-entry without reporting why it was refused.
fn loop_reentry_allowed(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    to_state: &str,
) -> bool {
    spent_loop_budget(machine, metadata, task_id, current_state, current_state_raw, to_state)
        .is_none()
}
