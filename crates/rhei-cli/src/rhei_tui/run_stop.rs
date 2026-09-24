//! Why a run stopped, in the one vocabulary every surface of it uses.
//!
//! The exit code, the console result line, the durable report's header and
//! `run_finished.summary.stop` say the same thing about the same run because
//! they are all rendered from the value below. It travels on `RunSummary` and
//! is carried only by a run that selected `--until-idle`: an unselected run's
//! stream stays byte-identical.
//!
//! Both vocabularies are closed, and both are written here rather than as bare
//! strings at the emit sites, so a word cannot be spelled two ways.
// §FS-rhei-run-json.2.1 §FS-rhei-run-report.3.1

/// Why the loop ended. Ranked, where two could claim the same run:
/// interruption outranks everything, attention outranks idle, and idle
/// outranks a plain completion only in that a complete plan never produces it.
/// §FS-rhei-run.3
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Every in-scope task is terminal.
    Complete,
    /// Work remains and every bit of it is deliberately waiting.
    Idle,
    /// Work remains that needs a person or a repair.
    Attention,
    /// The run itself errored.
    Failed,
    /// A signal ended it.
    Interrupted,
}

impl StopReason {
    /// The wire word. snake_case because the stream's sibling enumeration,
    /// `outcome`, already is. §FS-rhei-run-json.2.1
    pub fn name(self) -> &'static str {
        match self {
            StopReason::Complete => "complete",
            StopReason::Idle => "idle",
            StopReason::Attention => "attention",
            StopReason::Failed => "failed",
            StopReason::Interrupted => "interrupted",
        }
    }

    /// The inverse of [`StopReason::name`], for a reader decoding the stream.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "complete" => Some(StopReason::Complete),
            "idle" => Some(StopReason::Idle),
            "attention" => Some(StopReason::Attention),
            "failed" => Some(StopReason::Failed),
            "interrupted" => Some(StopReason::Interrupted),
            _ => None,
        }
    }
}

/// Which kind of wait dominates an idle return. `Mixed` is what more than one
/// kind at once reads as; it is never a fifth kind of blocker.
/// §FS-rhei-run-report.3.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleBlocker {
    /// A human gate.
    Gate,
    /// A future poll deadline.
    Poll,
    /// A recognized provider limit. §FS-rhei-run.3.3
    ProviderLimit,
    /// More than one of the above across the in-scope plan.
    Mixed,
}

impl IdleBlocker {
    /// The wire word, shared by the console line and the stream so a script
    /// reading either meets one vocabulary. §FS-rhei-run-report.3.1
    pub fn name(self) -> &'static str {
        match self {
            IdleBlocker::Gate => "gate",
            IdleBlocker::Poll => "poll",
            IdleBlocker::ProviderLimit => "provider_limit",
            IdleBlocker::Mixed => "mixed",
        }
    }

    /// The inverse of [`IdleBlocker::name`].
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "gate" => Some(IdleBlocker::Gate),
            "poll" => Some(IdleBlocker::Poll),
            "provider_limit" => Some(IdleBlocker::ProviderLimit),
            "mixed" => Some(IdleBlocker::Mixed),
            _ => None,
        }
    }

    /// The report and rich-summary phrasing for this wait, which is the
    /// vocabulary `result_phrase` publishes. §FS-rhei-run-report.3.1
    pub fn result_phrase(self, next_attempt_at: Option<&str>) -> String {
        match self {
            IdleBlocker::Gate => "idle \u{2014} waiting on a person".to_string(),
            IdleBlocker::Poll => match next_attempt_at {
                Some(instant) => format!("idle \u{2014} retry at {instant}"),
                // A poll wait whose deadline the only-blocker filter dropped
                // has no instant to name, and inventing one would publish a
                // time nothing is scheduled for. §FS-rhei-run.5.1
                None => "idle \u{2014} waiting on a timed retry".to_string(),
            },
            IdleBlocker::ProviderLimit => "idle \u{2014} waiting on a provider limit".to_string(),
            IdleBlocker::Mixed => "idle \u{2014} mixed waits".to_string(),
        }
    }
}

/// The stop a selected run reports, on every surface at once.
///
/// Every field is present, with `None` where it does not apply: null says this
/// run has no such value, absent would say this build does not tell you, and
/// only the first is a fact about the run. §FS-rhei-run-json.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStop {
    pub reason: StopReason,
    /// Present exactly when `reason` is [`StopReason::Idle`].
    pub idle_blocker: Option<IdleBlocker>,
    /// RFC 3339 UTC, the same spelling as `slot_released`'s own
    /// `next_attempt_at`. `None` when no in-scope task contributes one.
    pub next_attempt_at: Option<String>,
    /// The integer the process exits with. §FS-rhei-run-json.5
    pub exit_code: i32,
}

impl RunStop {
    /// The wire form, written here rather than at the encoder because this is
    /// a value object with a closed vocabulary of its own — the same division
    /// the accounting rollup already keeps, where the record encoder asks the
    /// value for its JSON instead of respelling its fields.
    // §FS-rhei-run-json.2.1
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "reason": self.reason.name(),
            "idle_blocker": self.idle_blocker.map(IdleBlocker::name),
            "next_attempt_at": self.next_attempt_at,
            "exit_code": self.exit_code,
        })
    }

    /// The inverse. A payload whose `reason` this build has no word for reads
    /// as no stop at all rather than as one of the words it does know:
    /// reporting the run as something it never said is the worse failure.
    // §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(serde_json::Value::as_str);
        Some(RunStop {
            reason: StopReason::from_name(text("reason")?)?,
            idle_blocker: text("idle_blocker").and_then(IdleBlocker::from_name),
            next_attempt_at: text("next_attempt_at").map(str::to_string),
            exit_code: value.get("exit_code").and_then(serde_json::Value::as_i64).unwrap_or(0)
                as i32,
        })
    }
}
