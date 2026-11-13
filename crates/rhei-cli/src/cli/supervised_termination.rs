// Owned process groups, their registry, and the shared termination sequence.
// §FS-rhei-run.3.2 §AR-source-file-size.3

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
    /// Deliver invocation-local test shutdown before any grace exit, even if
    /// the waiter misses every timed poll. §FS-rhei-run.3.2.1
    #[cfg(test)]
    fn entered_grace(&mut self, _stop: &StopToken) {}
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
        // §FS-rhei-run.3.2.1: synchronous delivery cannot be skipped by a stalled poller.
        #[cfg(test)]
        target.entered_grace(stop);
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
                let _ = signal_group(pgid, GroupSignal::Terminate);
            }
        }
    }

    fn is_gone(&mut self) -> std::io::Result<bool> {
        Ok(live_group_ids(self.owner).is_empty())
    }

    fn kill(&mut self) {
        for pgid in live_group_ids(self.owner) {
            let _ = signal_group(pgid, GroupSignal::Kill);
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
