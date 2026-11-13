// Run-local and process shutdown tokens, signal delivery, and interruptible sleeps.
// §FS-rhei-run.3.2 §AR-source-file-size.3

// ---------------------------------------------------------------------------
// Stop token
// ---------------------------------------------------------------------------

/// The run's stop token: raised by the signal handler, polled by every loop
/// that would otherwise keep working.
///
/// A value rather than a set of loose statics so the wait routine can be
/// exercised against a token a test owns, instead of the process-wide one.
///
/// Two things are counted separately because they are two different facts.
/// `stopping` is "end every wait and start nothing more", which an operator's
/// signal and an error unwind both mean. `signals` is how many times the
/// *operator* asked, and only that: escalating to an immediate `SIGKILL` is
/// something a person asks for twice, never something a failed `?` on another
/// thread can arrange on their behalf.
// §FS-rhei-run.3.2: one interruption contract for the whole run.
struct StopToken {
    /// Whether the run is shutting down at all, whoever asked.
    stopping: std::sync::atomic::AtomicBool,
    /// How many times the operator has signalled. `>= 2` is the operator
    /// asking twice, which skips the termination grace.
    signals: std::sync::atomic::AtomicU8,
    /// The first signal that raised this token, `0` until one does. First one
    /// wins, so the exit code names the signal the operator actually sent.
    signal: std::sync::atomic::AtomicI32,
    /// Whether the shutdown notice has been written, so several workers
    /// noticing the same interrupt produce one line rather than one each.
    announced: std::sync::atomic::AtomicBool,
}

impl StopToken {
    const fn new() -> Self {
        Self {
            stopping: std::sync::atomic::AtomicBool::new(false),
            signals: std::sync::atomic::AtomicU8::new(0),
            signal: std::sync::atomic::AtomicI32::new(0),
            announced: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Raise the token without naming a signal, stopping the whole process's
    /// work: the one caller is the lost-console exit, which is leaving through
    /// `std::process::exit` and can no longer ask which run owned what. It
    /// should not change the exit code, and it is not the operator asking for
    /// the grace to be skipped.
    ///
    /// A run tearing down after its *own* failure does not come here — that is
    /// one run's business, and it says so through [`mark_run_stopping`].
    // §FS-rhei-run.3.2
    fn request(&self) {
        self.stopping.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Raise the token and record the signal that did it. Called from the
    /// signal handler, so everything it touches must be lock-free.
    ///
    /// Unix-only, because naming a signal is: off Unix nothing installs a
    /// handler and the run is stopped through [`StopToken::request`] alone.
    #[cfg(unix)]
    fn raise(&self, signum: i32) {
        let _ = self.signal.compare_exchange(
            0,
            signum,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        );
        // Saturating, so an operator leaning on Ctrl+C cannot wrap the counter
        // back round to "not interrupted".
        let _ = self.signals.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |count| Some(count.saturating_add(1)),
        );
        self.request();
    }

    /// Whether the run is shutting down: schedule nothing new, end every wait.
    fn is_set(&self) -> bool {
        self.stopping.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times the operator has signalled.
    fn signals_received(&self) -> u8 {
        self.signals.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether the operator asked twice, which means "kill the group now"
    /// rather than "ask it to stop and wait out the grace". §FS-rhei-run.3.2
    fn skip_grace(&self) -> bool {
        self.signals_received() >= 2
    }

    fn signal_number(&self) -> Option<i32> {
        match self.signal.load(std::sync::atomic::Ordering::SeqCst) {
            0 => None,
            signum => Some(signum),
        }
    }

    /// `128 + signal`, the status a shell reports for a process the signal
    /// killed. `None` when the token was raised without one. §FS-rhei-run.3.2
    fn exit_code(&self) -> Option<i32> {
        self.signal_number().map(|signum| 128 + signum)
    }

    /// The shutdown notice — what is being terminated, and that a second signal
    /// skips the grace — handed to **one** caller and `None` to every other, so
    /// several workers noticing the same interrupt produce one line rather than
    /// one each.
    ///
    /// Text, never I/O. Where the line belongs depends on what the operator is
    /// looking at — a live TUI journal pane, a terminal the TUI has already
    /// restored, a redirected stdout — and that is the frontend's question.
    /// Writing it here sent an external `SIGTERM` arriving mid-render straight
    /// into the alternate screen.
    ///
    /// A run tearing its own groups down after a failure raises the same token
    /// an operator's signal does, and gets nothing: telling that operator they
    /// interrupted something — and to press Ctrl+C *again*, when they have not
    /// pressed it once — describes a run that does not exist, and points away
    /// from the failure actually being reported.
    // §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
    fn take_announcement(&self) -> Option<String> {
        // Only a signal has an operator waiting to read this, and only they
        // can be told truthfully to press Ctrl+C again. §FS-rhei-run.3.2
        self.signal_number()?;
        if self.announced.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let labels = live_invocation_labels();
        Some(if labels.is_empty() {
            "\nInterrupted — no subprocess in flight; stopping the run.".to_string()
        } else {
            format!(
                "\nInterrupted — terminating {} invocation(s) ({}); \
                 press Ctrl+C again to kill immediately.",
                labels.len(),
                labels.join(", ")
            )
        })
    }
}

/// The process's one stop token. A `rhei run` process drives exactly one run,
/// so the signal that interrupts it interrupts all of it. §FS-rhei-run.3.2
static INTERRUPT: StopToken = StopToken::new();

/// The runs that are tearing their own groups down.
///
/// The global token means the *process* is stopping: an operator's signal, or
/// a lost console leaving through `exit`. A run unwinding from its own error
/// is not that. It has to stop its own waits — a worker must not go on waiting
/// out a deadline against a group its run is already tearing down — but it has
/// no business stopping a run beside it, and a process can drive more than one
/// (the in-process tests do). Ownership is already tracked per run; this is
/// the same scope, applied to the one fact that was still global.
// §FS-rhei-run.3.2
static STOPPING_RUNS: Mutex<BTreeSet<u64>> = Mutex::new(BTreeSet::new());

/// Mark one run as tearing down. Run `0` is "no run owns this thread", which
/// nothing can mark.
///
/// Never unmarked: a run that has torn down has torn down for good, and run ids
/// are handed out once, so a finished run's mark can never be mistaken for a
/// live one. The set therefore holds one entry per run the process has stopped
/// — one, outside the in-process tests.
// §FS-rhei-run.3.2
fn mark_run_stopping(owner: u64) {
    if owner == 0 {
        return;
    }
    if let Ok(mut stopping) = STOPPING_RUNS.lock() {
        stopping.insert(owner);
    }
}

/// Whether this particular run is tearing down.
///
/// A poisoned lock degrades to "not stopping" rather than panicking a shutdown
/// path — the global token still ends every wait on the paths that matter.
fn run_is_stopping(owner: u64) -> bool {
    owner != 0
        && STOPPING_RUNS.lock().map(|stopping| stopping.contains(&owner)).unwrap_or(false)
}

/// Whether the named run is shutting down: its own teardown, or a signal that
/// stopped the whole process. §FS-rhei-run.3.2
fn run_shutdown_requested(owner: u64) -> bool {
    INTERRUPT.is_set() || run_is_stopping(owner)
}

/// Whether the run this thread belongs to is shutting down: schedule nothing
/// new, end every wait.
fn interrupt_requested() -> bool {
    run_shutdown_requested(current_run_owner())
}

/// The one fact a live surface needs from the run behind it: whether that run
/// is ending on anything other than its own terms.
///
/// A value the run owns rather than a reading taken through the thread-local
/// owner, because the surface outlives the thread's ownership of the run: a
/// TUI shuts down from inside [`RunSubprocessGuard`]'s own unwind, by which
/// point the guard has handed that ownership back and a global reading would
/// answer "no run is stopping" for the very run that is.
// §FS-rhei-run-tui.1.5.7 §FS-rhei-run.3.2
#[derive(Clone, Default)]
struct RunShutdown(Arc<std::sync::atomic::AtomicBool>);

impl RunShutdown {
    /// The run is ending abnormally; a finished surface must leave rather than
    /// park on itself waiting for a `q` nobody is there to press.
    fn raise(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Asked by the frontend before it parks. The process-wide token counts
    /// too: a signal ends this run whether or not its guard has run yet.
    fn is_raised(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst) || INTERRUPT.is_set()
    }
}

/// Whether an operator's signal stopped this run, as opposed to the shutdown
/// guard stopping in-flight work on the way out of a failure.
///
/// Both raise the same token, because both mean "start nothing more and end
/// every wait". They mean opposite things to a *reader*, though: one says the
/// operator stopped a healthy run, the other that the run is unwinding from an
/// error it is about to report. Only the first belongs in the run's result.
// §FS-rhei-run.3.2 §FS-rhei-run-report.3.1
fn interrupted_by_signal() -> bool {
    INTERRUPT.signal_number().is_some()
}

/// The process exit status for an interrupted run. §FS-rhei-run.3.2
fn interrupt_exit_code() -> Option<i32> {
    INTERRUPT.exit_code()
}

/// The run's shutdown notice, for the first caller to ask. §FS-rhei-run.3.2
fn take_interruption_announcement() -> Option<String> {
    INTERRUPT.take_announcement()
}

/// A `notify` for [`Supervised::wait`] that puts the shutdown notice on the
/// run's event stream, where the frontend in use decides where it is legible.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
fn notify_through_sink(sink: &Arc<dyn rhei_tui::EventSink>) -> impl Fn(String) + '_ {
    move |text| {
        sink.emit(rhei_tui::RunEvent::Message { level: rhei_tui::MessageLevel::Warn, text })
    }
}

/// The one handler for every interrupting signal.
///
/// Async-signal-safe by construction: it touches two lock-free atomics and
/// does nothing else — no allocation, no locking, no I/O. Every decision that
/// follows from an interrupt is taken by the loops that poll the token.
// §FS-rhei-run.3.2: SIGINT, SIGTERM, and SIGHUP interrupt the run.
#[cfg(all(unix, not(test)))]
extern "C" fn interrupt_signal_handler(signum: std::ffi::c_int) {
    INTERRUPT.raise(signum);
}

/// Install the interruption handler for `SIGINT`, `SIGTERM`, and `SIGHUP`.
///
/// `SA_RESTART` so an interrupt does not turn every in-progress read in the
/// process into an `EINTR` failure. `SIGPIPE` is deliberately untouched: this
/// CLI writes to pipes it owns and needs `EPIPE` back as a value
/// (see [`install_quiet_broken_pipe_exit`]).
// §FS-rhei-run.3.2: one handler, installed for every `rhei run`.
#[cfg(all(unix, not(test)))]
fn install_interrupt_handlers() {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet};

    let action = SigAction::new(
        SigHandler::Handler(interrupt_signal_handler),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    for sig in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the handler is async-signal-safe (two atomics, no more).
        unsafe {
            let _ = signal::sigaction(sig, &action);
        }
    }
}

/// Nothing to install here.
///
/// Off Unix the run keeps the platform default. Under `cfg(test)` the reason
/// is different: signal dispositions are process-wide, and the in-process
/// tests that call `run_command` would leave the whole test binary swallowing
/// its own Ctrl+C into a token nothing reads. The handler is exercised where
/// it matters — the interruption e2e tests signal the real binary.
// §FS-rhei-run.3.2
#[cfg(any(not(unix), test))]
fn install_interrupt_handlers() {}

/// Sleep up to `total`, returning early once the run is interrupted.
///
/// The scheduler's idle waits are minutes long (a poll deadline, a human
/// gate); sleeping them out whole would keep a run alive long after the
/// operator asked it to stop.
// §FS-rhei-run.3.2: an interrupted run schedules nothing further.
fn interruptible_sleep(total: Duration) {
    let deadline = Instant::now() + total;
    loop {
        // This run's shutdown, not the process's alone: a run tearing down
        // after its own failure must not sleep out a poll deadline either.
        // §FS-rhei-run.3.2
        if interrupt_requested() {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        std::thread::sleep(SUPERVISED_SLEEP_SLICE.min(deadline - now));
    }
}
