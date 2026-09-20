// Supervised subprocess groups.
//
// Every subprocess `rhei run` starts to do a ticket's work — agents, programs,
// and the snapshot redactor — is a process group it owns, with exactly one
// early-termination path and three reasons to take it: the invocation's
// deadline, an operator interrupt, or the supervisor's death. Timeout and
// shutdown are two triggers of the same routine, applied to the group.
//
// Three subprocesses are deliberately outside this, each for its own reason:
// a subprocess a *callback* starts is that callback's own child and the
// callback contract governs it; the `git` queries of the post-transition
// consistency check are short synchronous bookkeeping that ends before the
// call returns; and the editor the dashboard launches is detached on purpose,
// because it belongs to the operator and outliving the run is the point.

// §FS-rhei-run.3.2 §DA-supervised-process-groups

/// Grace between `SIGTERM` and `SIGKILL` when terminating a group — the same
/// 10 s whether a deadline or an interruption fired it. §FS-rhei-run.3.2
#[cfg(not(test))]
const SUPERVISED_TERMINATE_GRACE: Duration = Duration::from_secs(10);
#[cfg(test)]
const SUPERVISED_TERMINATE_GRACE: Duration = Duration::from_millis(50);

/// First poll interval of the supervised wait loop. Waiting is a poll rather
/// than a blocking `wait()` because the loop must also see the stop token, and
/// the price of that is latency the child never had: whatever the loop is
/// asleep for when the child exits.
const SUPERVISED_POLL_MIN: Duration = Duration::from_millis(10);

/// Ceiling the wait loop's poll interval ramps up to.
///
/// A single interval has to be either wasteful for an agent that runs for
/// minutes or slow for a redactor that finishes in tens of milliseconds. The
/// ramp is neither: short invocations are noticed almost at once, and a long
/// one settles into a cheap idle poll within a couple of seconds.
const SUPERVISED_POLL_MAX: Duration = Duration::from_millis(200);

/// The next poll interval after an idle pass: double, then stop at the cap.
fn next_poll_interval(current: Duration) -> Duration {
    (current * 2).min(SUPERVISED_POLL_MAX)
}

/// Poll interval while waiting out the termination grace.
const SUPERVISED_GRACE_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Longest a scheduler sleep may run before it re-reads the stop token.
const SUPERVISED_SLEEP_SLICE: Duration = Duration::from_millis(500);

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

// ---------------------------------------------------------------------------
// Live group registry
// ---------------------------------------------------------------------------

/// One live supervised subprocess: the run that owns it and the
/// `<task>@<state>` label of the invocation it belongs to.
#[cfg(unix)]
struct LiveGroup {
    owner: u64,
    label: String,
    /// Whether this group has already been asked to stop. Both the shutdown
    /// guard and the group's own waiter can reach the same group, and the
    /// second one to arrive has nothing to add by re-sending `SIGTERM`.
    asked_to_stop: bool,
}

/// Every supervised subprocess that has not been reaped, keyed by process-group
/// id.
///
/// This is what makes the shutdown paths a handler cannot reach — an early `?`
/// return, a panic unwind — able to tear down work owned by other threads, and
/// what lets the shutdown notice name what it is stopping.
// §FS-rhei-run.3.2: the supervisor's death ends its subprocesses.
#[cfg(unix)]
static LIVE_GROUPS: Mutex<BTreeMap<i32, LiveGroup>> = Mutex::new(BTreeMap::new());

/// Source of run ids. A `rhei run` process drives one run, so this only ever
/// hands out more than one id under the in-process tests.
static NEXT_RUN_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

thread_local! {
    /// The run whose subprocesses this thread spawns. `0` means "no run owns
    /// them" — a direct `spawn_and_wait_agent` from a test, say — and no
    /// shutdown guard will claim them. Worker threads inherit it explicitly
    /// from the thread that started them ([`inherit_run_owner`]).
    static RUN_OWNER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The run this thread's subprocesses belong to.
fn current_run_owner() -> u64 {
    RUN_OWNER.with(std::cell::Cell::get)
}

fn set_run_owner(owner: u64) {
    RUN_OWNER.with(|cell| cell.set(owner));
}

/// Adopt the calling run on a worker thread, so the subprocesses it spawns are
/// torn down by that run's guard and by no other. §FS-rhei-run.3.2
fn inherit_run_owner(owner: u64) {
    set_run_owner(owner);
}

#[cfg(unix)]
fn register_live_group(pgid: i32, label: &str) {
    if let Ok(mut live) = LIVE_GROUPS.lock() {
        live.insert(
            pgid,
            LiveGroup {
                owner: current_run_owner(),
                label: label.to_string(),
                asked_to_stop: false,
            },
        );
    }
}

/// Claim the right to send this group its `SIGTERM`, returning whether the
/// caller is the first to ask. A group nobody registered — one already reaped —
/// is nobody's to ask, so the claim fails.
#[cfg(unix)]
fn claim_group_termination(pgid: i32) -> bool {
    LIVE_GROUPS
        .lock()
        .map(|mut live| {
            live.get_mut(&pgid)
                .is_some_and(|group| !std::mem::replace(&mut group.asked_to_stop, true))
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn unregister_live_group(pgid: i32) {
    if let Ok(mut live) = LIVE_GROUPS.lock() {
        live.remove(&pgid);
    }
}

/// The live process-group ids, filtered to one run's own when `owner` names
/// one and taken wholesale when it is `None`.
///
/// A poisoned lock degrades to "none known" rather than panicking a shutdown
/// path. It cannot deadlock: nothing that holds this lock prints, allocates a
/// diagnostic, or can otherwise re-enter a shutdown path while holding it.
#[cfg(unix)]
fn live_group_ids(owner: Option<u64>) -> Vec<i32> {
    LIVE_GROUPS
        .lock()
        .map(|live| {
            live.iter()
                .filter(|(_, group)| owner.is_none_or(|owner| group.owner == owner))
                .map(|(pgid, _)| *pgid)
                .collect()
        })
        .unwrap_or_default()
}

/// The `<task>@<state>` labels of the invocations still running.
#[cfg(unix)]
fn live_invocation_labels() -> Vec<String> {
    LIVE_GROUPS
        .lock()
        .map(|live| live.values().map(|group| group.label.clone()).collect())
        .unwrap_or_default()
}

#[cfg(not(unix))]
fn live_invocation_labels() -> Vec<String> {
    Vec::new()
}

/// What a termination sequence acts on: something to ask, something to wait
/// for, and something to kill when the asking has run out of time.
///
/// The two callers differ only in these three answers. A group's own waiter
/// signals its group and watches its own child; the shutdown guard signals a
/// set of groups it does not own children for and watches the registry
/// instead. The sequence between them — and the grace it honours — is one.
// §FS-rhei-run.3.2: one termination sequence, whatever triggered it.
trait TerminationTarget {
    /// Ask it to stop.
    fn ask_to_stop(&mut self);
    /// Whether there is nothing left to wait for.
    fn is_gone(&mut self) -> std::io::Result<bool>;
    /// End it now.
    fn kill(&mut self);
}

/// The one early-termination sequence: `SIGTERM`, the 10 s grace, then
/// `SIGKILL` on whatever is left — unless the operator already asked twice,
/// which skips straight to the kill.
///
/// Every exit from here goes through the kill except the one that saw the
/// target gone. An error asking whether it is gone — `ECHILD` from a stray
/// reap, say — is a reason to stop *waiting*, never a reason to leave a group
/// alive: returning on it would skip the `SIGKILL` and leave the caller to
/// deregister a live group as if it had been reaped. The error is still
/// reported, after the kill, so the caller knows the reap is unreliable.
// §FS-rhei-run.3.2: one termination sequence, and it always ends the group.
fn run_termination_sequence(
    target: &mut dyn TerminationTarget,
    stop: &StopToken,
) -> std::io::Result<()> {
    let mut failure = None;
    if !stop.skip_grace() {
        target.ask_to_stop();
        let deadline = Instant::now() + SUPERVISED_TERMINATE_GRACE;
        while Instant::now() < deadline {
            match target.is_gone() {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(err) => {
                    failure = Some(err);
                    break;
                }
            }
            // A second interrupt mid-grace is the operator saying "now".
            if stop.skip_grace() {
                break;
            }
            std::thread::sleep(SUPERVISED_GRACE_POLL_INTERVAL);
        }
    }
    target.kill();
    match failure {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// The registry's side of the sequence: signal a run's groups, and watch for
/// their owning threads to reap them.
#[cfg(unix)]
struct LiveGroupsTarget {
    owner: Option<u64>,
}

#[cfg(unix)]
impl TerminationTarget for LiveGroupsTarget {
    fn ask_to_stop(&mut self) {
        for pgid in live_group_ids(self.owner) {
            // A group its own waiter has already asked is not asked twice.
            if claim_group_termination(pgid) {
                let _ = signal::killpg(Pid::from_raw(pgid), Signal::SIGTERM);
            }
        }
    }

    fn is_gone(&mut self) -> std::io::Result<bool> {
        Ok(live_group_ids(self.owner).is_empty())
    }

    fn kill(&mut self) {
        for pgid in live_group_ids(self.owner) {
            let _ = signal::killpg(Pid::from_raw(pgid), Signal::SIGKILL);
        }
    }
}

/// Terminate live groups with the shared sequence. `owner` scopes it to one
/// run's own groups; `None` takes every registered group. Signals only — each
/// group's owning thread reaps its own child.
// §FS-rhei-run.3.2: one termination sequence, whatever triggered it.
#[cfg(unix)]
fn terminate_live_groups(owner: Option<u64>) {
    if live_group_ids(owner).is_empty() {
        return;
    }
    let _ = run_termination_sequence(&mut LiveGroupsTarget { owner }, &INTERRUPT);
}

#[cfg(not(unix))]
fn terminate_live_groups(_owner: Option<u64>) {}

/// Terminate **every** live group, whoever started it.
///
/// For the one exit a destructor cannot reach: the process is leaving from
/// inside a failed print, through `std::process::exit`, which runs no `Drop`.
/// [`RunSubprocessGuard`] never gets its turn, so this stands in for it — and
/// it cannot ask which run owns what, because the thread that lost its output
/// need not be the thread that started the work.
// §FS-rhei-run.3.2: a run that loses its console ends its groups first.
fn terminate_all_live_groups() {
    // Stop the waits first: a worker that is about to re-enter its own
    // termination sequence should find the run already shutting down.
    INTERRUPT.request();
    terminate_live_groups(None);
}

/// Declared alongside [`RunReportGuard`] so it runs on **every** way out of an
/// execution mode — an early `?`, a panic unwind, or a normal end. Without it,
/// an error return after workers were spawned leaves exactly the orphans this
/// design exists to prevent.
///
/// It claims only the subprocesses of its own run: the registry is global, but
/// ownership is not, so the guard cannot reach into work it did not start.
///
/// **Declare it after the frontend**, so it drops *before* the frontend does.
/// A run unwinding from an error has to tell its surface so before that
/// surface decides whether to park on its finished screen; a TUI that parks
/// blocks the engine on the render thread, and the teardown below never gets
/// its turn until an operator who has already walked away presses `q`.
///
/// The run is marked as stopping whether or not it still holds a live group.
/// A worker that is already past the scheduler's own check — loading a plan,
/// resolving tooling, composing a prompt — has not registered anything yet, and
/// marking only when the registry is non-empty would let it spawn the agent the
/// shutdown had already ruled out.
// §FS-rhei-run.3.2: the supervisor's death ends its subprocesses.
// §FS-rhei-run-tui.1.5.7
struct RunSubprocessGuard {
    owner: u64,
    /// The surface's copy of "this run is ending abnormally". Raised on the
    /// way out unless the run said it finished on its own terms.
    // §FS-rhei-run-tui.1.5.7
    shutdown: RunShutdown,
    /// Whether the run's loop reached its own end. Set by [`Self::finished`]
    /// on the path that goes on to write a report.
    finished: bool,
}

impl RunSubprocessGuard {
    fn install(shutdown: RunShutdown) -> Self {
        let owner = NEXT_RUN_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        set_run_owner(owner);
        Self { owner, shutdown, finished: false }
    }

    /// The run's loop ended on its own terms, so a finished surface keeps its
    /// operator: they are there to read it and to press `q`. Anything else —
    /// an early `?`, a panic unwind — has nobody waiting on that screen while
    /// the engine blocks on the render thread behind it.
    // §FS-rhei-run-tui.1.5.7
    fn finished(&mut self) {
        self.finished = true;
    }
}

impl Drop for RunSubprocessGuard {
    fn drop(&mut self) {
        if !self.finished {
            // Before the frontend this guard is declared after gets its own
            // turn to drop. §FS-rhei-run-tui.1.5.7
            self.shutdown.raise();
        }
        // Unconditionally and before the teardown, so a worker already past
        // the scheduler's check is still refused at its spawn.
        // §FS-rhei-run.3.2
        mark_run_stopping(self.owner);
        terminate_live_groups(Some(self.owner));
        set_run_owner(0);
    }
}

// ---------------------------------------------------------------------------
// Invocation outcomes
// ---------------------------------------------------------------------------

/// What the run surfaces need from a finished invocation, whichever kind of
/// subprocess did the work. Agents and programs answer it identically, so they
/// are classified by one routine rather than two that have to be kept in step.
trait InvocationOutcome {
    fn was_interrupted(&self) -> bool;
    fn timed_out(&self) -> bool;
    fn status(&self) -> std::process::ExitStatus;
}

/// The slot outcome and exit code an invocation reports to the run surfaces.
///
/// Interruption is tested **first**, before success: the engine ended this
/// invocation, so whatever status it managed to exit with during the grace is
/// not a verdict on the ticket, and reporting it as completed would fire a
/// transition the operator never asked for.
// §FS-rhei-run.3.2: interruption is not a completion.
fn slot_outcome<T: InvocationOutcome>(
    result: &MietteResult<T>,
) -> (rhei_tui::TaskOutcome, Option<i32>) {
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(err) => return (rhei_tui::TaskOutcome::Failed(err.to_string()), None),
    };
    let code = outcome.status().code();
    let reported = if outcome.was_interrupted() {
        rhei_tui::TaskOutcome::Interrupted
    } else if outcome.status().success() {
        rhei_tui::TaskOutcome::Completed
    } else if outcome.timed_out() {
        rhei_tui::TaskOutcome::TimedOut
    } else {
        rhei_tui::TaskOutcome::Failed(format!("exit {}", code.unwrap_or(-1)))
    };
    (reported, code)
}

/// The operator-facing line for an invocation the run shut down, with the log
/// named when there is one. One wording for every path that can print it: a
/// ticket that reads one way in the sequential run and another in the parallel
/// one reads as two different situations.
// §FS-rhei-run.3.2
fn interrupted_task_warning(task_id: &str, state: &str, log: Option<&Path>) -> String {
    let head = format!("  Task {task_id} interrupted in '{state}'; state unchanged.");
    match log {
        Some(log) => format!("{head} Log: {}", log.display()),
        None => head,
    }
}

// ---------------------------------------------------------------------------
// Supervised subprocess
// ---------------------------------------------------------------------------

/// Why a supervised subprocess stopped. §FS-rhei-run.3.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndCause {
    /// It exited on its own.
    Exited,
    /// Its deadline fired and the engine terminated its group.
    TimedOut,
    /// The run was interrupted and the engine terminated its group.
    Interrupted,
}

/// A finished supervised invocation: the status reaped from the direct child,
/// and why it stopped.
#[derive(Debug)]
struct Ended {
    status: std::process::ExitStatus,
    cause: EndCause,
}

/// Hand the run's shutdown notice to `notify`, if this is the caller the token
/// gives it to. §FS-rhei-run.3.2
fn announce_shutdown(stop: &StopToken, notify: &dyn Fn(String)) {
    if let Some(notice) = stop.take_announcement() {
        notify(notice);
    }
}

/// Whether a spawn failed because the run had already been interrupted, rather
/// than because the command could not start. §FS-rhei-run.3.2
fn spawn_was_interrupted(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::Interrupted
}

/// The status recorded for an invocation the shutdown stopped before it ever
/// started: `SIGTERM`, which is what its group would have been sent a moment
/// later. Never read as a verdict — every surface tests `interrupted` before it
/// looks at a status.
// §FS-rhei-run.3.2
fn never_started_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        std::process::ExitStatus::from_raw(Signal::SIGTERM as i32)
    }
    #[cfg(not(unix))]
    {
        use std::os::windows::process::ExitStatusExt as _;
        std::process::ExitStatus::from_raw(1)
    }
}

/// A subprocess and the process group it leads.
struct Supervised {
    child: std::process::Child,
    /// The group id, which equals the leader's pid. Unix only: Windows keeps
    /// the single-child `kill()` it always had.
    #[cfg(unix)]
    pgid: i32,
    /// The run this invocation belongs to, captured at spawn so its wait ends
    /// when that run tears down — and stays put when another run in the same
    /// process tears down beside it. §FS-rhei-run.3.2
    owner: u64,
    /// Set once the child has been reaped, so `Drop` knows whether it still
    /// owns a live group — and so that it deregisters the group at most once.
    /// The agent path holds a reaped value through output draining, log
    /// footers, and usage capture, and pgids are reused: a second, blind
    /// deregistration there strikes off whatever holds the pgid *now*, leaving
    /// another run's live group invisible to every shutdown path.
    reaped: bool,
}

impl Supervised {
    /// Spawn `cmd` as the leader of its own process group and register the
    /// group with the run's shutdown path under `label` (`<task>@<state>`).
    ///
    /// Refuses outright once the run is shutting down. This is the one place a
    /// subprocess actually starts, and so the only place "an interrupted run
    /// starts nothing further" holds with no window in front of it: the
    /// scheduler checks the token too, but between its check and here a pass
    /// still loads the plan, resolves tooling, composes a prompt, and hands the
    /// item to a worker thread. A signal landing anywhere in that stretch used
    /// to start an agent under `bypassPermissions` that the shutdown had
    /// already ruled out.
    // §FS-rhei-run.3.2
    fn spawn(cmd: &mut std::process::Command, label: &str) -> std::io::Result<Self> {
        // A raw agent/program/redactor command carries no confinement proof.
        // §AR-neural-admission.1 §FS-rhei-budgets.6.4
        Self::spawn_with_budget(cmd, label, None)
    }

    fn spawn_with_budget(cmd: &mut std::process::Command, label: &str, capability: Option<&BudgetProcessCapability>) -> std::io::Result<Self> {
        match capability {
            Some(capability) => capability.authorize()?,
            None => rhei_core::budget::refuse_unqualified_spawn()?,
        }
        // The one place work actually starts, and so the only place the rule
        // holds with no window in front of it: the scheduler's own check is a
        // whole item's work earlier. §FS-rhei-run.3.2
        if interrupt_requested() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "the run was interrupted before this subprocess started",
            ));
        }
        let owner = current_run_owner();
        // The detached-child marker describes *this* process, not its work. It
        // is inherited by the whole subtree, so an agent or program that runs
        // `rhei run` of its own would wait forever at a human gate, ignore
        // `--no-dashboard`, and refuse to detach. One removal here covers
        // agents, programs, and the snapshot redactor alike.

        // §FS-rhei-run-headless.1.2
        cmd.env_remove(HEADLESS_CHILD_ENV);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            // The descendants inherit it, which is the whole point: an agent's
            // MCP servers and shell tools are terminated with it.
            cmd.process_group(0);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt as _;
            // Captured before the fork so the child can tell "my parent is
            // still the process that spawned me" from "it already died and I
            // was reparented" — `getppid() == 1` is not that test, because a
            // subreaper adopts orphans instead of init.
            let supervisor = std::process::id();
            // SAFETY: this runs between fork and exec in the child. It makes
            // two syscalls, allocates nothing, and takes no lock.
            unsafe {
                cmd.pre_exec(move || {
                    // Backstop for the deaths no handler can catch — SIGKILL,
                    // OOM — where the supervisor runs no code. §FS-rhei-run.3.2
                    let _ = nix::sys::prctl::set_pdeathsig(Some(Signal::SIGTERM));
                    // The per-thread semantics are harmless: the thread that
                    // spawns a subprocess is the one that waits on it, so it
                    // outlives the child on every path short of the whole
                    // process dying. Arming happens after the fork, though, so
                    // a supervisor that died in between would never deliver it
                    // — close that window before going any further.
                    if nix::unistd::getppid().as_raw() as u32 != supervisor {
                        return Err(std::io::Error::from_raw_os_error(
                            nix::errno::Errno::ESRCH as i32,
                        ));
                    }
                    Ok(())
                });
            }
        }

        let child = cmd.spawn()?;
        if let Some(capability) = capability { capability.attach(child.id()); }
        #[cfg(unix)]
        let pgid = child.id() as i32;
        #[cfg(unix)]
        register_live_group(pgid, label);
        #[cfg(not(unix))]
        let _ = label;
        Ok(Self {
            child,
            #[cfg(unix)]
            pgid,
            owner,
            reaped: false,
        })
    }

    /// Whether the run that owns this invocation is shutting down: the token
    /// the caller supplied, or this run's own teardown. §FS-rhei-run.3.2
    fn shutdown_requested(&self, stop: &StopToken) -> bool {
        stop.is_set() || run_is_stopping(self.owner)
    }

    /// Wait for the subprocess, its deadline, or the run's interruption —
    /// whichever comes first. The last two run the identical termination
    /// sequence against the group and differ only in the cause reported.
    ///
    /// `notify` is handed the shutdown notice by whichever waiter notices the
    /// interrupt first, and by no other; it is the caller's to supply because
    /// only the caller knows which sink the operator is reading.
    ///
    /// The stop token is read on the way *out* of the termination sequence as
    /// well as on the way in, because that sequence can hold this thread for
    /// the whole grace. An invocation already past its deadline still has ten
    /// seconds to flush, and an operator who hits Ctrl+C inside them has
    /// interrupted it: reporting that as a timeout fires the timeout transition
    /// on a ticket the shutdown promised to leave alone, and leaves the report
    /// calling the run interrupted while the ledger calls the ticket timed
    /// out.
    // §FS-rhei-run.3.2: timeout and shutdown are one routine.
    fn wait(
        &mut self,
        timeout: Option<Duration>,
        stop: &StopToken,
        notify: &dyn Fn(String),
    ) -> std::io::Result<Ended> {
        let start = Instant::now();
        let mut poll = SUPERVISED_POLL_MIN;
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.finish();
                return Ok(Ended { status, cause: EndCause::Exited });
            }
            // Shutdown outranks the deadline: both are true when an agent is
            // seconds from its timeout as the operator hits Ctrl+C, and calling
            // that a timeout fires a transition. §FS-rhei-run.3.2
            let interrupted = self.shutdown_requested(stop);
            let timed_out = !interrupted && timeout.is_some_and(|limit| start.elapsed() > limit);
            if !interrupted && !timed_out {
                std::thread::sleep(poll);
                poll = next_poll_interval(poll);
                continue;
            }
            // The first waiter to notice names every invocation the shutdown
            // is about to end, and says so *before* the grace rather than
            // after it. §FS-rhei-run.3.2
            if interrupted {
                announce_shutdown(stop, notify);
            }
            let status = self.terminate_and_reap(stop)?;
            // Asked again on the way out: the sequence above can hold this
            // thread for the whole grace, and a Ctrl+C inside it interrupted an
            // invocation that was only timing out. §FS-rhei-run.3.2
            let cause = if interrupted || self.shutdown_requested(stop) {
                // First notice of it, when the shutdown landed mid-grace.
                announce_shutdown(stop, notify);
                EndCause::Interrupted
            } else {
                EndCause::TimedOut
            };
            return Ok(Ended { status, cause });
        }
    }

    /// Run the shared termination sequence against this invocation's group and
    /// reap its leader. §FS-rhei-run.3.2
    fn terminate_and_reap(
        &mut self,
        stop: &StopToken,
    ) -> std::io::Result<std::process::ExitStatus> {
        // The sequence ends the group on every path through it, so by the time
        // it returns the group is dead whatever it has to report.
        let sequence = run_termination_sequence(self, stop);
        let reaped = match self.child.try_wait() {
            Ok(Some(status)) => Ok(status),
            // Either the grace ran out and the group was killed, or it ended
            // between the last poll and here; `wait` settles both.
            Ok(None) => self.child.wait(),
            Err(err) => Err(err),
        };
        // Unconditionally: the group is dead either way, and a failure to
        // *reap* is not a failure to *end*. §FS-rhei-run.3.2
        self.finish();
        sequence.and(reaped)
    }

    /// Record that the child has been reaped and drop the group's registration.
    fn finish(&mut self) {
        self.reaped = true;
        #[cfg(unix)]
        unregister_live_group(self.pgid);
    }

    /// Ask the whole group to stop, unless the shutdown guard already has.
    #[cfg(unix)]
    fn terminate_group(&mut self) {
        if claim_group_termination(self.pgid) {
            let _ = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGTERM);
        }
    }

    /// Windows has no process group to signal here, so the direct child is
    /// killed exactly as it was before this change. §FS-rhei-run.3.2
    #[cfg(not(unix))]
    fn terminate_group(&mut self) {
        let _ = self.child.kill();
    }

    /// End the whole group now.
    ///
    /// The leader is killed by name as well as by group. It is normally in the
    /// group and gets the `killpg` like everything else; a leader that called
    /// `setsid` would not be, and the `wait()` that follows this would then
    /// have nothing to wait for but a process nobody signalled.
    #[cfg(unix)]
    fn kill_group(&mut self) {
        let _ = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGKILL);
        let _ = self.child.kill();
    }

    #[cfg(not(unix))]
    fn kill_group(&mut self) {
        let _ = self.child.kill();
    }
}

/// This invocation's side of the sequence: signal its own group, and watch its
/// own child.
impl TerminationTarget for Supervised {
    fn ask_to_stop(&mut self) {
        self.terminate_group();
    }

    fn is_gone(&mut self) -> std::io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    fn kill(&mut self) {
        self.kill_group();
    }
}

impl Drop for Supervised {
    fn drop(&mut self) {
        // A reaped invocation deregistered itself in `finish`, and pgids are
        // reused: striking one off twice removes whatever holds it now.
        // §FS-rhei-run.3.2
        if self.reaped {
            return;
        }
        // Left by an error or a panic before the wait finished: leaving that
        // way earns the group no less time to flush. §FS-rhei-run.3.2
        let _ = self.terminate_and_reap(&INTERRUPT);
        #[cfg(unix)]
        unregister_live_group(self.pgid);
    }
}
