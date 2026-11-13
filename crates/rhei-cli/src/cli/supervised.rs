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

// The one way rhei signals a process tree on Unix, shared with bounded
// callbacks. §FS-rhei-transitions.4.10
#[cfg(unix)]
use rhei_core::platform::process_tree::{signal_group, GroupSignal};

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

/// How long a child dead of an interrupting signal waits for the run's own copy
/// of that signal to raise the stop token before it is classed `exited`.
/// §FS-rhei-run.3.2
const SHUTDOWN_DELIVERY_SETTLE: Duration = Duration::from_millis(250);

/// Whether `status` is a death by one of the signals that interrupt a run —
/// `SIGINT`, `SIGTERM`, `SIGHUP` — either raw or as a shell's `128 + signal`.
/// Only such a death can be the run's own shutdown arriving a moment early.
/// §FS-rhei-run.3.2
fn died_of_an_interrupting_signal(status: std::process::ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        let interrupting = [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP].map(|s| s as i32);
        status.signal().is_some_and(|signum| interrupting.contains(&signum))
            || status.code().is_some_and(|code| interrupting.contains(&(code - 128)))
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        false
    }
}

/// Poll interval while waiting out the termination grace.
const SUPERVISED_GRACE_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Longest a scheduler sleep may run before it re-reads the stop token.
const SUPERVISED_SLEEP_SLICE: Duration = Duration::from_millis(500);

// §AR-source-file-size.3: separate shutdown state and shared group termination from the invocation wait.
include!("supervised_shutdown.rs");
include!("supervised_termination.rs");

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
    /// Consumed at real grace entry, isolated to this invocation and absent
    /// from normal builds. §FS-rhei-run.3.2.1
    #[cfg(test)]
    on_grace_entry: Option<fn(&StopToken)>,
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
        #[cfg(unix)]
        let pgid = child.id() as i32;
        #[cfg(unix)]
        register_live_group(pgid, label);
        #[cfg(not(unix))]
        let _ = label;
        Ok(Self {
            child,
            #[cfg(test)]
            on_grace_entry: None,
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
                // A death the shutdown delivered is an interruption. Announced
                // while the group is still registered, so the notice names it.
                // §FS-rhei-run.3.2
                let cause = if !status.success() && self.ended_under_shutdown(status, stop) {
                    announce_shutdown(stop, notify);
                    EndCause::Interrupted
                } else {
                    EndCause::Exited
                };
                self.finish();
                return Ok(Ended { status, cause });
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

    /// Whether a child that ended on its own, unsuccessfully, ended under this
    /// run's shutdown, which makes it interrupted rather than exited
    /// (§FS-rhei-run.3.2).
    ///
    /// One delivery that reaches the run and its agent at once — systemd
    /// stopping a cgroup — does not promise the run's handler runs first: the
    /// agent can be dead and reaped before the run's own copy of the signal has
    /// raised the token. A child dead of an interrupting signal therefore gets
    /// a short, bounded look at the token before it is classed `exited`; any
    /// other failure is read against the token as it stands.
    fn ended_under_shutdown(&self, status: std::process::ExitStatus, stop: &StopToken) -> bool {
        if self.shutdown_requested(stop) {
            return true;
        }
        if !died_of_an_interrupting_signal(status) {
            return false;
        }
        let deadline = Instant::now() + SHUTDOWN_DELIVERY_SETTLE;
        while Instant::now() < deadline {
            std::thread::sleep(SUPERVISED_POLL_MIN);
            if self.shutdown_requested(stop) {
                return true;
            }
        }
        false
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
            let _ = signal_group(self.pgid, GroupSignal::Terminate);
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
        let _ = signal_group(self.pgid, GroupSignal::Kill);
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

    /// Take before calling so panic cleanup cannot deliver twice. §FS-rhei-run.3.2.1
    #[cfg(test)]
    fn entered_grace(&mut self, stop: &StopToken) {
        if let Some(deliver) = self.on_grace_entry.take() {
            deliver(stop);
        }
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
