// What the Transition Ledger says a spawned invocation came to: the outcome
// mirrored off its slot release, the short reason its row gives, and whether a
// transition was taken at all.
//
// Its own part because it is a vocabulary — one word per outcome the stream
// can carry — and the renderer around it only places the words.

// §AR-source-file-size.3 §FS-rhei-run-report.4

/// The terminal disposition of a spawned invocation, mirrored from
/// [`rhei_tui::TaskOutcome`] so the renderer does not depend on the TUI enum.
#[derive(Debug, Clone)]
enum LedgerOutcome {
    Completed,
    Failed(String),
    /// The invocation selected its state's poll self-loop, so it neither failed
    /// nor finished the state: it waited. §FS-rhei-states.2.2
    Waiting,
    ProviderLimited { provider: String, next_attempt_at: String },
    /// A recognized provider unavailability, with the wait it wrote, if any.
    /// §FS-rhei-run-report.4
    ProviderUnavailable { provider: Option<String>, next_attempt_at: Option<String> },
    Cancelled,
    TimedOut,
    /// The run was interrupted and the engine ended the invocation; no
    /// transition was selected. §FS-rhei-run.3.2
    Interrupted,
    /// An outcome word this build has no variant for, kept as it was read.
    /// Reachable only from a decoded `--json` stream — a run's own events never
    /// carry one. §FS-rhei-run-json.2.2
    Unrecognized(String),
}

impl From<rhei_tui::TaskOutcome> for LedgerOutcome {
    fn from(outcome: rhei_tui::TaskOutcome) -> Self {
        match outcome {
            rhei_tui::TaskOutcome::Completed => LedgerOutcome::Completed,
            rhei_tui::TaskOutcome::Failed(msg) => LedgerOutcome::Failed(msg),
            rhei_tui::TaskOutcome::Waiting => LedgerOutcome::Waiting,
            rhei_tui::TaskOutcome::ProviderLimited { provider, next_attempt_at } => {
                LedgerOutcome::ProviderLimited { provider, next_attempt_at }
            }
            rhei_tui::TaskOutcome::ProviderUnavailable { provider, next_attempt_at } => {
                LedgerOutcome::ProviderUnavailable { provider, next_attempt_at }
            }
            rhei_tui::TaskOutcome::Cancelled => LedgerOutcome::Cancelled,
            rhei_tui::TaskOutcome::TimedOut => LedgerOutcome::TimedOut,
            rhei_tui::TaskOutcome::Interrupted => LedgerOutcome::Interrupted,
            rhei_tui::TaskOutcome::Unrecognized(name) => LedgerOutcome::Unrecognized(name),
        }
    }
}

impl LedgerOutcome {
    /// An interruption, like a provider wait, takes no transition: its row's
    /// destination is `-`. §FS-rhei-run-report.4
    fn unrouted(&self) -> bool {
        matches!(
            self,
            LedgerOutcome::ProviderLimited { .. }
                | LedgerOutcome::ProviderUnavailable { .. }
                | LedgerOutcome::Interrupted
        )
    }
}

/// A short reason string for a spawned invocation, from its outcome and exit.
fn ledger_outcome_reason(outcome: &LedgerOutcome, exit_code: Option<i32>) -> String {
    match outcome {
        LedgerOutcome::Completed => match exit_code {
            Some(0) | None => "exit 0".to_string(),
            Some(code) => format!("exit {code}"),
        },
        LedgerOutcome::Failed(msg) => {
            let msg = msg.lines().next().unwrap_or("").trim();
            match exit_code {
                Some(code) if msg.is_empty() => format!("failed, exit {code}"),
                Some(code) => format!("exit {code}: {msg}"),
                None if msg.is_empty() => "failed".to_string(),
                None => format!("failed: {msg}"),
            }
        }
        // The reason a poll wait gives is the wait, with the exit the machine
        // declared as "come back later" kept beside it. §FS-rhei-states.2.2
        LedgerOutcome::Waiting => match exit_code {
            Some(code) => format!("poll delay, exit {code}"),
            None => "poll delay".to_string(),
        },
        LedgerOutcome::ProviderLimited { provider, next_attempt_at } => {
            format!("provider {provider} limited until {next_attempt_at}")
        }
        // Without a wait the charge spent the visit, and its halt says the rest.
        // §FS-rhei-run-report.4
        LedgerOutcome::ProviderUnavailable { provider, next_attempt_at } => {
            match (provider, next_attempt_at) {
                (Some(provider), Some(at)) => format!("provider {provider} unavailable until {at}"),
                _ => "provider unavailable".to_string(),
            }
        }
        LedgerOutcome::Cancelled => "cancelled".to_string(),
        LedgerOutcome::TimedOut => "timed out".to_string(),
        // Not a verdict on the ticket: the run stopped the worker. §FS-rhei-run.3.2
        LedgerOutcome::Interrupted => "interrupted".to_string(),
        // The word the stream used, kept rather than translated into one this
        // build knows. §FS-rhei-run-json.2.2
        LedgerOutcome::Unrecognized(name) => match exit_code {
            Some(code) => format!("{name}, exit {code}"),
            None => name.clone(),
        },
    }
}
