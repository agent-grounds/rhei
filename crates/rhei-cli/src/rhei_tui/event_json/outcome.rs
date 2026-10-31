//! The `outcome` of a `slot_released` record and the fields that travel with
//! it, in both directions: the one vocabulary the journal, the stream and an
//! attached reader share. §FS-rhei-run-json.2.1 §FS-rhei-run-tui.1.7

use serde_json::{json, Value};

use crate::rhei_tui::event::TaskOutcome;

/// Write `outcome` and what it carries: a failure's `reason`, and a provider
/// wait's `provider` and `next_attempt_at`. §FS-rhei-run-json.2.1
pub(super) fn put_outcome(outcome: &TaskOutcome, put: &mut dyn FnMut(&str, Value)) {
    put("outcome", json!(outcome_name(outcome)));
    match outcome {
        TaskOutcome::Failed(reason) => put("reason", json!(reason)),
        TaskOutcome::ProviderLimited { provider, next_attempt_at } => {
            put("provider", json!(provider));
            put("next_attempt_at", json!(next_attempt_at));
        }
        // Both absent when the charge spent the visit's last attempt and no
        // wait was written. §FS-rhei-run-json.2.1
        TaskOutcome::ProviderUnavailable { provider, next_attempt_at } => {
            if let Some(provider) = provider {
                put("provider", json!(provider));
            }
            if let Some(next_attempt_at) = next_attempt_at {
                put("next_attempt_at", json!(next_attempt_at));
            }
        }
        _ => {}
    }
}

/// The `outcome` vocabulary of a `slot_released` record. §FS-rhei-run-json.2.1
fn outcome_name(outcome: &TaskOutcome) -> &str {
    match outcome {
        TaskOutcome::Completed => "completed",
        TaskOutcome::Failed(_) => "failed",
        TaskOutcome::Waiting => "waiting",
        TaskOutcome::ProviderLimited { .. } => "provider_limited",
        TaskOutcome::ProviderUnavailable { .. } => "provider_unavailable",
        TaskOutcome::Cancelled => "cancelled",
        TaskOutcome::TimedOut => "timeout",
        TaskOutcome::Interrupted => "interrupted",
        // Written back exactly as it was read, which is what makes a reader
        // that re-encodes — `rhei attach --json` — pass the record on
        // unchanged. §FS-rhei-run-json.2.2
        TaskOutcome::Unrecognized(name) => name,
    }
}

/// Read an `outcome` back off the stream, so an attached reader shows the run
/// the words the run itself wrote. A value this build has no variant for is
/// kept under its own name rather than rewritten to one that is: the vocabulary
/// is additive under one `schema`, a reader that stopped on a word it had not
/// met would turn that into an outage, and one that renamed it would report the
/// run as something it never said.
// §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2
pub(super) fn decode_outcome(
    name: &str,
    reason: Option<String>,
    provider: Option<String>,
    next_attempt_at: Option<String>,
) -> TaskOutcome {
    match name {
        "completed" => TaskOutcome::Completed,
        "failed" => TaskOutcome::Failed(reason.unwrap_or_default()),
        "waiting" => TaskOutcome::Waiting,
        "provider_limited" => TaskOutcome::ProviderLimited {
            provider: provider.unwrap_or_default(),
            next_attempt_at: next_attempt_at.unwrap_or_default(),
        },
        "provider_unavailable" => TaskOutcome::ProviderUnavailable { provider, next_attempt_at },
        "cancelled" => TaskOutcome::Cancelled,
        "timeout" => TaskOutcome::TimedOut,
        "interrupted" => TaskOutcome::Interrupted,
        other => TaskOutcome::Unrecognized(other.to_string()),
    }
}
