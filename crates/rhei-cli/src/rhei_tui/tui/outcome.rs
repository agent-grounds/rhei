//! How a released slot's outcome reads in the journal pane: its mark, the level
//! it claims, and what the line carries beside the task. One vocabulary, kept
//! apart from the run model that applies it. §FS-rhei-run-tui.1.7

use crate::rhei_tui::event::{MessageLevel, TaskOutcome};

/// The mark the journal line opens with.
pub(super) fn outcome_mark(outcome: &TaskOutcome) -> &'static str {
    match outcome {
        TaskOutcome::Completed => "✓",
        TaskOutcome::Failed(_) => "✗",
        // The state is not done and nothing went wrong with it: the
        // machine asked for another attempt. §FS-rhei-states.2.2
        TaskOutcome::Waiting => "⏳",
        TaskOutcome::ProviderLimited { .. } => "⏳",
        // Calm only while it waits; a spent visit halts. §FS-rhei-run-tui.1.7
        TaskOutcome::ProviderUnavailable { next_attempt_at: Some(_), .. } => "⏳",
        TaskOutcome::ProviderUnavailable { next_attempt_at: None, .. } => "✗",
        TaskOutcome::Cancelled => "⊘",
        TaskOutcome::TimedOut => "⏱",
        // Distinct from cancelled's `⊘`: nothing about the ticket
        // is wrong, the run simply stopped. §FS-rhei-run.3.2
        TaskOutcome::Interrupted => "⏹",
        // A word from a newer run than this reader.
        // §FS-rhei-run-json.2.2
        TaskOutcome::Unrecognized(_) => "?",
    }
}

/// A handled wait claims no attention: it reads at the level a completion
/// does, not a failure's. §FS-rhei-states.2.2 §FS-rhei-run-tui.1.7
pub(super) fn outcome_level(outcome: &TaskOutcome) -> MessageLevel {
    match outcome {
        TaskOutcome::Completed
        | TaskOutcome::Waiting
        | TaskOutcome::ProviderLimited { .. }
        | TaskOutcome::ProviderUnavailable { next_attempt_at: Some(_), .. } => MessageLevel::Info,
        _ => MessageLevel::Warn,
    }
}

/// What the line carries after the task: a provider wait's provider and
/// deadline, and an unknown word as the stream spelled it, so the line says
/// what the run said even where the symbol cannot. §FS-rhei-run-json.2.2
pub(super) fn outcome_suffix(outcome: &TaskOutcome) -> String {
    match outcome {
        TaskOutcome::ProviderLimited { provider, next_attempt_at } => {
            format!(" {provider} until {next_attempt_at}")
        }
        TaskOutcome::ProviderUnavailable {
            provider: Some(provider),
            next_attempt_at: Some(next_attempt_at),
        } => format!(" {provider} unavailable until {next_attempt_at}"),
        TaskOutcome::Unrecognized(name) => format!(" {name}"),
        _ => String::new(),
    }
}
