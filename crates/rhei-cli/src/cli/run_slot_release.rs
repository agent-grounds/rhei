// The slot release a finished invocation owes the run's surfaces, carried from
// where its process is reaped to where its transition is known.
//
// Its own part because those are not the same place, and in the worker pool not
// even the same thread: a poll state's self-loop makes the attempt a wait, and
// which transition was selected is decided only after the program has run and
// the engine has re-read the plan.

// §AR-source-file-size.3 §FS-rhei-states.2.2

/// What a finished invocation reports to the run's surfaces.
///
/// Every value on it is read where the process was reaped, so the timings stay
/// the worker's own however much later the event is emitted.
// §FS-rhei-run-tui.1.1
struct SlotRelease {
    slot: rhei_tui::Slot,
    task: String,
    /// The state at assignment and the state the invocation was worked in.
    /// Equal while it did not move the ticket, which is how the journal tells
    /// an `end@<state>` from a transition. §FS-rhei-run-tui.1.7
    from: String,
    to: String,
    log_path: PathBuf,
    outcome: rhei_tui::TaskOutcome,
    finished_at: Instant,
    wall_clock: std::time::SystemTime,
    exit_code: Option<i32>,
    duration_ms: u64,
}

/// A [`SlotRelease`] not emitted yet, because its outcome is not settled yet.
///
/// What an attempt earns is not a function of its exit status alone: a poll
/// state's self-loop means "not done yet" whatever exit matched it, so that
/// attempt is a wait rather than a failure or a completion
/// (§FS-rhei-states.2.2), and the engine knows which transition it selected
/// only after the program has run. The release is therefore held across the
/// branches that decide what the exit meant and emitted when this value drops —
/// exactly once, on every one of them, including the early returns and the `?`
/// that leave the decision half made.
///
/// It owns its sink rather than borrowing one so that it can travel: the worker
/// pool hands the release to the main thread down the completion channel, and a
/// run aborting on another worker's error abandons that channel. Owning the
/// sink means such a message still writes its `end@` line as it is dropped,
/// rather than leaving a `start@` the journal never closes.
struct PendingSlotRelease {
    sink: Arc<dyn rhei_tui::EventSink>,
    /// `None` only once [`Drop`] has taken it to emit.
    release: Option<SlotRelease>,
}

impl PendingSlotRelease {
    fn hold(sink: Arc<dyn rhei_tui::EventSink>, release: SlotRelease) -> Self {
        Self { sink, release: Some(release) }
    }

    /// The engine selected this state's poll self-loop and scheduled the next
    /// attempt, so what is being released is a handled wait rather than the
    /// failure or completion its exit code would otherwise read as.
    // §FS-rhei-states.2.2
    fn waiting(&mut self) {
        if let Some(release) = self.release.as_mut() {
            release.outcome = rhei_tui::TaskOutcome::Waiting;
        }
    }

    /// A recognized provider refusal is a calm, typed wait with the same
    /// provider and UTC deadline on every event surface. §FS-rhei-run-tui.1.1
    fn provider_limited(&mut self, limit: &ProviderLimit) {
        if let Some(release) = self.release.as_mut() {
            release.outcome = rhei_tui::TaskOutcome::ProviderLimited {
                provider: limit.identity.provider.clone(),
                next_attempt_at: limit.next_attempt_at.clone(),
            };
        }
    }
}

impl Drop for PendingSlotRelease {
    fn drop(&mut self) {
        let Some(release) = self.release.take() else { return };
        self.sink.emit(rhei_tui::RunEvent::SlotReleased {
            slot: release.slot,
            task: release.task,
            from: release.from,
            to: release.to,
            log_path: release.log_path,
            outcome: release.outcome,
            finished_at: release.finished_at,
            wall_clock: release.wall_clock,
            exit_code: release.exit_code,
            duration_ms: release.duration_ms,
        });
    }
}
