    /// Unit tests for the stop token and the supervised wait routine.
    ///
    /// The token is exercised as a value, never through the process-wide
    /// `INTERRUPT`: these tests run concurrently with tests that drive whole
    /// runs, and raising the global token would stop them too.
    // §FS-rhei-run.3.2 §DA-supervised-process-groups
    #[cfg(unix)]
    mod supervised_tests {
        use super::*;
        use std::os::unix::process::ExitStatusExt as _;

        /// Long enough that nothing under test can outlive the assertion by
        /// finishing on its own.
        const CHILD_SLEEP: &str = "sleep 30";

        /// The tests assert on causes, not on operator-facing text; the
        /// notice's routing is the frontend's test. §FS-rhei-run-tui.1.8
        fn ignore_notice(_: String) {}

        fn sh(script: &str) -> std::process::Command {
            let mut cmd = std::process::Command::new("sh");
            cmd.arg("-c").arg(script);
            cmd.stdin(std::process::Stdio::null());
            cmd
        }

        /// Whether a pid still exists, by the `kill -0` rule.
        fn pid_is_alive(pid: i32) -> bool {
            signal::kill(Pid::from_raw(pid), None).is_ok()
        }

        /// Poll until `check` holds or the deadline passes; no fixed sleeps, so
        /// a slow machine costs patience rather than a failure.
        fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if check() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("timed out waiting for {what}");
        }

        #[test]
        fn a_fresh_token_is_not_set() {
            let token = StopToken::new();
            assert!(!token.is_set());
            assert!(!token.skip_grace());
            assert_eq!(token.signals_received(), 0);
            assert_eq!(token.exit_code(), None);
        }

        #[test]
        fn the_first_signal_wins_and_names_the_exit_code() {
            let token = StopToken::new();
            token.raise(Signal::SIGINT as i32);
            token.raise(Signal::SIGTERM as i32);
            assert_eq!(token.signal_number(), Some(Signal::SIGINT as i32));
            // 128 + SIGINT: what a shell reports for a process SIGINT killed.
            assert_eq!(token.exit_code(), Some(130));
        }

        /// A second interrupt is the operator saying "now", so it and nothing
        /// less flips the skip-grace decision.
        #[test]
        fn a_second_interrupt_skips_the_grace_and_a_first_does_not() {
            let token = StopToken::new();
            token.raise(Signal::SIGINT as i32);
            assert!(token.is_set());
            assert!(!token.skip_grace(), "one Ctrl+C asks; it does not kill");
            token.raise(Signal::SIGINT as i32);
            assert!(token.skip_grace());
        }

        /// The shutdown guard stops in-flight work without naming a signal, so
        /// the exit code stays whatever the run itself decided.
        #[test]
        fn a_requested_stop_sets_no_exit_code() {
            let token = StopToken::new();
            token.request();
            assert!(token.is_set());
            assert_eq!(token.exit_code(), None);
        }

        /// Skipping the grace is something the operator asks for twice, and
        /// nothing an error unwind on another thread can arrange for them: an
        /// agent must not lose its 10 s to flush and commit because a worker
        /// somewhere else returned `Err` after a single Ctrl+C.
        #[test]
        fn a_teardown_never_escalates_an_operators_single_signal() {
            let token = StopToken::new();
            token.raise(Signal::SIGINT as i32);
            for _ in 0..10 {
                token.request();
            }
            assert!(token.is_set());
            assert!(!token.skip_grace(), "only a second signal skips the grace");
            token.raise(Signal::SIGINT as i32);
            assert!(token.skip_grace());
        }

        /// A teardown belongs to the run that raised it. A process drives more
        /// than one run under the in-process tests, and a teardown that
        /// escaped its own run would make every other one break out of its
        /// first pass and report success without doing any work.
        // §FS-rhei-run.3.2
#[test]
        fn a_teardown_stops_its_own_run_and_no_other() {
            let mine = NEXT_RUN_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let beside = NEXT_RUN_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            mark_run_stopping(mine);
            assert!(run_is_stopping(mine));
            assert!(!run_is_stopping(beside), "a run beside it was never asked to stop");
            assert!(!INTERRUPT.is_set(), "one run's failure is not the process stopping");

            // Ids are handed out once, so the mark a finished run leaves behind
            // can never be mistaken for a later run's.
            let later = NEXT_RUN_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert!(!run_is_stopping(later));

            // "No run owns this thread" is not a run and cannot be stopped.
            mark_run_stopping(0);
            assert!(!run_is_stopping(0));
        }

        /// A surface asks the run it belongs to, not the thread it is on: by
        /// the time a TUI shuts down, the guard has already handed back its
        /// thread-local ownership, and a reading taken there answers "no run is
        /// stopping" for the very run that is.
        // §FS-rhei-run-tui.1.5.7
        #[test]
        fn a_raised_run_shutdown_survives_the_owning_thread_giving_up_the_run() {
            let shutdown = RunShutdown::default();
            assert!(!shutdown.is_raised());
            shutdown.raise();
            set_run_owner(0);
            assert!(shutdown.is_raised(), "the surface still knows its run is ending");
        }

        /// A wedged operator leaning on Ctrl+C must not wrap the counter back
        /// round to "not interrupted".
        #[test]
        fn the_signal_count_saturates_instead_of_wrapping() {
            let token = StopToken::new();
            for _ in 0..300 {
                token.raise(Signal::SIGINT as i32);
            }
            assert_eq!(token.signals_received(), u8::MAX);
            assert!(token.is_set());
            assert!(token.skip_grace());
        }

        /// A subprocess that finishes fast must be noticed fast; one that runs
        /// for minutes must not be polled at 10 ms for all of them. The ramp is
        /// what lets one wait routine serve a redactor and an agent alike.
        #[test]
        fn the_poll_interval_ramps_from_the_floor_to_the_cap_and_stops() {
            assert_eq!(next_poll_interval(SUPERVISED_POLL_MIN), Duration::from_millis(20));
            let mut poll = SUPERVISED_POLL_MIN;
            for _ in 0..10 {
                poll = next_poll_interval(poll);
            }
            assert_eq!(poll, SUPERVISED_POLL_MAX, "the ramp settles at the cap");
            assert_eq!(next_poll_interval(SUPERVISED_POLL_MAX), SUPERVISED_POLL_MAX);
        }

        #[test]
        fn a_subprocess_that_exits_on_its_own_reports_exited() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh("exit 3"), "unit@exit").expect("spawn");
            let ended = supervised.wait(Some(Duration::from_secs(30)), &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Exited);
            assert_eq!(ended.status.code(), Some(3));
        }

        #[test]
        fn a_deadline_reports_timed_out_and_ends_the_group() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@timeout").expect("spawn");
            let pgid = supervised.pgid;
            let ended =
                supervised.wait(Some(Duration::from_millis(1)), &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::TimedOut);
            assert!(!ended.status.success());
            drop(supervised);
            wait_until("the timed-out group to be gone", || !pid_is_alive(pgid));
        }

        /// The token, not only the deadline, ends a wait — and a supervised
        /// invocation with no timeout at all is still interruptible, which the
        /// old blocking `wait()` was not.
        #[test]
        fn a_raised_token_reports_interrupted_even_without_a_deadline() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@interrupt").expect("spawn");
            let pgid = supervised.pgid;
            token.raise(Signal::SIGTERM as i32);
            let ended = supervised.wait(None, &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            drop(supervised);
            wait_until("the interrupted group to be gone", || !pid_is_alive(pgid));
        }

        /// An agent seconds from its deadline when the operator hits Ctrl+C is
        /// an interrupted invocation, not a timed-out one. Calling it a timeout
        /// would fire the timeout transition and rewrite the ticket a shutdown
        /// promised to leave alone.
        // §FS-rhei-run.3.2
        #[test]
        fn a_deadline_and_a_shutdown_on_the_same_poll_report_interrupted() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@both").expect("spawn");
            let pgid = supervised.pgid;
            token.raise(Signal::SIGINT as i32);
            // A deadline already in the past, so the first poll sees both.
            let ended = supervised
                .wait(Some(Duration::from_millis(0)), &token, &ignore_notice)
                .expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            drop(supervised);
            wait_until("the interrupted group to be gone", || !pid_is_alive(pgid));
        }

        /// Whether `pid` has died and waits to be reaped. `ps` rather than
        /// `waitid(WNOWAIT)`, which macOS does not offer through `nix`.
        fn is_zombie(pid: i32) -> bool {
            std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim_start().starts_with('Z'))
                .unwrap_or(false)
        }

        /// A child that ends before `wait` first polls it, while the run's
        /// shutdown is already under way: the token is set, then the child
        /// runs `script` to its end and is left unreaped.
        fn wait_on_a_child_that_ended_under_a_shutdown(script: &str, label: &str) -> Ended {
            let token = StopToken::new();
            let mut supervised = Supervised::spawn(&mut sh(script), label).expect("spawn");
            let pgid = supervised.pgid;
            token.raise(Signal::SIGTERM as i32);
            wait_until("the child to die and await its reaping", || is_zombie(pgid));
            supervised.wait(None, &token, &ignore_notice).expect("wait")
        }

        /// An agent dead of its own copy of the signal that stopped the run —
        /// systemd's `KillMode=control-group` delivers it to the whole cgroup at
        /// once — is interrupted, not exited: the attempt must not be charged.
        // §FS-rhei-run.3.2
        #[test]
        fn a_child_killed_by_the_shutdowns_own_signal_reports_interrupted() {
            let ended =
                wait_on_a_child_that_ended_under_a_shutdown("kill -TERM $$", "unit@cgroup");
            assert_eq!(ended.status.signal(), Some(Signal::SIGTERM as i32));
            assert_eq!(ended.cause, EndCause::Interrupted);
        }

        /// An agent that traps the signal and exits non-zero did not finish its
        /// work either, so it is interrupted like one the signal killed.
        // §FS-rhei-run.3.2
        #[test]
        fn a_child_that_exits_non_zero_under_a_shutdown_reports_interrupted() {
            let ended = wait_on_a_child_that_ended_under_a_shutdown("exit 143", "unit@trapped");
            assert_eq!(ended.status.code(), Some(143));
            assert_eq!(ended.cause, EndCause::Interrupted);
        }

        /// A clean exit at that instant finished its work before the shutdown
        /// could stop it, and keeps its real ending.
        // §FS-rhei-run.3.2
        #[test]
        fn a_child_that_exits_zero_under_a_shutdown_keeps_exited() {
            let ended = wait_on_a_child_that_ended_under_a_shutdown("exit 0", "unit@clean");
            assert!(ended.status.success());
            assert_eq!(ended.cause, EndCause::Exited);
        }

        /// The unit `rhei run` owns is the group, not the child: a subprocess
        /// that hands its work to a grandchild cannot outlive its own death
        /// certificate.
        #[test]
        fn terminating_a_supervised_invocation_takes_its_grandchildren() {
            let dir = tempfile::tempdir().expect("tmpdir");
            let pid_file = dir.path().join("grandchild.pid");
            let token = StopToken::new();
            let mut supervised = Supervised::spawn(
                &mut sh(&format!(
                    "sleep 30 & echo $! > {}; {CHILD_SLEEP}",
                    pid_file.display()
                )),
                "unit@grandchild",
            )
            .expect("spawn");
            let pgid = supervised.pgid;

            wait_until("the grandchild to record its pid", || {
                fs::read_to_string(&pid_file).map(|text| !text.trim().is_empty()).unwrap_or(false)
            });
            let grandchild: i32 = fs::read_to_string(&pid_file)
                .expect("read pid file")
                .trim()
                .parse()
                .expect("grandchild pid");
            assert!(pid_is_alive(grandchild), "the grandchild should be running");

            token.raise(Signal::SIGTERM as i32);
            let ended = supervised.wait(None, &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            drop(supervised);

            wait_until("the leader to be gone", || !pid_is_alive(pgid));
            wait_until("the grandchild to be gone", || !pid_is_alive(grandchild));
        }

        /// A `Supervised` dropped without being waited on — an error return, a
        /// panic unwind — must not leave its group running.
        #[test]
        fn dropping_an_unwaited_invocation_kills_its_group() {
            let supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@drop").expect("spawn");
            let pgid = supervised.pgid;
            assert!(pid_is_alive(pgid));
            drop(supervised);
            wait_until("the dropped group to be gone", || !pid_is_alive(pgid));
        }

        /// The registry is what the shutdown guard reads, so a reaped
        /// invocation must leave it — otherwise the guard waits out a grace for
        /// a process that is already gone.
        #[test]
        fn a_reaped_invocation_leaves_the_live_registry() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh("exit 0"), "unit@registry").expect("spawn");
            let pgid = supervised.pgid;
            assert!(live_group_ids_for_test().contains(&pgid));
            supervised.wait(Some(Duration::from_secs(30)), &token, &ignore_notice).expect("wait");
            assert!(!live_group_ids_for_test().contains(&pgid));
        }

        /// Every registered group, whoever owns it.
        fn live_group_ids_for_test() -> Vec<i32> {
            LIVE_GROUPS.lock().map(|live| live.keys().copied().collect()).unwrap_or_default()
        }

        /// A group registered by a thread that no run owns is invisible to any
        /// run's guard: ownership is what a guard terminates, not liveness.
        #[test]
        fn an_unowned_group_is_claimed_by_no_run() {
            let supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@unowned").expect("spawn");
            let pgid = supervised.pgid;
            assert_eq!(current_run_owner(), 0, "a test thread runs under no run");
            assert!(!live_group_ids(Some(1)).contains(&pgid));
            assert!(!live_group_ids(Some(0)).is_empty(), "unowned groups are still tracked");
            // The lost-output exit has no owner to ask: it takes them all.
            assert!(live_group_ids(None).contains(&pgid));
            drop(supervised);
        }

        /// `SIGKILL` is what ends a group that ignores `SIGTERM`; the exit
        /// status reports the signal, which is how the log footer's non-zero
        /// code is explained.
        #[test]
        fn a_group_that_ignores_sigterm_is_killed() {
            let dir = tempfile::tempdir().expect("tmpdir");
            let ready = dir.path().join("trapped");
            let token = StopToken::new();
            let mut supervised = Supervised::spawn(
                &mut sh(&format!(
                    "trap '' TERM; : > {}; {CHILD_SLEEP}",
                    ready.display()
                )),
                "unit@stubborn",
            )
            .expect("spawn");
            // Raising the token before the shell has run `trap` would kill it
            // with the default disposition and prove nothing.
            wait_until("the child to ignore SIGTERM", || ready.exists());
            token.raise(Signal::SIGINT as i32);
            let ended = supervised.wait(None, &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            assert_eq!(
                ended.status.signal(),
                Some(Signal::SIGKILL as i32),
                "the grace expires and the group is killed"
            );
        }

        /// The shutdown can arrive *inside* the termination grace: an agent
        /// already past its deadline, with ten seconds to flush, when the
        /// operator hits Ctrl+C. Reading the token only on the way in called
        /// that a timeout, fired the timeout transition on a ticket the
        /// shutdown had promised to leave alone, and left the report calling
        /// the run interrupted while the ledger called the ticket timed out.
        // §FS-rhei-run.3.2: a shutdown outranks a deadline, whenever it arrives.
        #[test]
        fn a_shutdown_inside_the_grace_outranks_the_deadline_it_interrupted() {
            let dir = tempfile::tempdir().expect("tmpdir");
            let ready = dir.path().join("trapped");
            let token = Arc::new(StopToken::new());
            let mut supervised = Supervised::spawn(
                &mut sh(&format!("trap '' TERM; : > {}; {CHILD_SLEEP}", ready.display())),
                "unit@grace",
            )
            .expect("spawn");
            let pgid = supervised.pgid;
            // A child that ignores `SIGTERM` holds the grace open for its whole
            // length, so the signal below lands inside it rather than after it.
            wait_until("the child to ignore SIGTERM", || ready.exists());

            let raiser = Arc::clone(&token);
            let handle = std::thread::spawn(move || {
                std::thread::sleep(SUPERVISED_TERMINATE_GRACE / 10);
                raiser.raise(Signal::SIGINT as i32);
            });

            // A deadline already in the past, so the wait enters the grace as a
            // timeout and the operator interrupts it there.
            let ended = supervised
                .wait(Some(Duration::from_millis(0)), &token, &ignore_notice)
                .expect("wait");
            handle.join().expect("raiser");
            assert_eq!(
                ended.cause,
                EndCause::Interrupted,
                "the shutdown that arrived mid-grace decides the cause"
            );
            drop(supervised);
            wait_until("the group to be gone", || !pid_is_alive(pgid));
        }

        /// A target that cannot say whether it is gone.
        struct UnpollableTarget {
            asked: bool,
            killed: bool,
        }

        impl TerminationTarget for UnpollableTarget {
            fn ask_to_stop(&mut self) {
                self.asked = true;
            }

            fn is_gone(&mut self) -> std::io::Result<bool> {
                Err(std::io::Error::from_raw_os_error(nix::errno::Errno::ECHILD as i32))
            }

            fn kill(&mut self) {
                self.killed = true;
            }
        }

        /// Failing to *poll* a group is not a reason to leave it alive.
        /// Returning on the error skipped the `SIGKILL` and skipped the reap,
        /// and the caller went on to strike the group off the registry anyway —
        /// an orphan running under nobody's supervision, which is the one thing
        /// this design exists to prevent.
        // §FS-rhei-run.3.2: one termination sequence, and it always ends the group.
        #[test]
        fn a_group_that_cannot_be_polled_is_still_killed() {
            let token = StopToken::new();
            let mut target = UnpollableTarget { asked: false, killed: false };
            let result = run_termination_sequence(&mut target, &token);
            assert!(target.asked, "it is asked to stop first");
            assert!(target.killed, "and killed rather than left running");
            assert!(result.is_err(), "while the failure to poll is still reported");
        }

        /// `Drop` must not strike a group off the registry a second time. The
        /// agent path holds a reaped `Supervised` through output draining, log
        /// footers, and usage capture, and pgids are reused: an unconditional
        /// deregistration there removes whatever holds the pgid *now* — another
        /// run's live group — from every shutdown path that could end it.
        // §FS-rhei-run.3.2
        #[test]
        fn dropping_a_reaped_invocation_leaves_a_reused_pgid_alone() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh("exit 0"), "unit@reused").expect("spawn");
            let pgid = supervised.pgid;
            supervised.wait(Some(Duration::from_secs(30)), &token, &ignore_notice).expect("wait");
            assert!(!live_group_ids_for_test().contains(&pgid), "the wait deregistered it");

            // Stand in for the kernel handing the same pgid to a group spawned
            // after this one was reaped.
            register_live_group(pgid, "unit@newcomer");
            drop(supervised);
            assert!(
                live_group_ids_for_test().contains(&pgid),
                "the newcomer stays visible to every shutdown path"
            );
            unregister_live_group(pgid);
        }

        /// The scheduler checks the token, but between its check and the spawn
        /// a pass still loads the plan, resolves tooling, composes a prompt,
        /// and hands the item to a worker thread. The spawn itself is the only
        /// point with no window in front of it.
        // §FS-rhei-run.3.2: an interrupted run starts nothing further.
        #[test]
        fn an_interrupted_run_starts_no_subprocess_at_all() {
            let owner = NEXT_RUN_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            set_run_owner(owner);
            mark_run_stopping(owner);

            let err = match Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@refused") {
                Err(err) => err,
                Ok(_) => panic!("an interrupted run started a subprocess anyway"),
            };
            assert!(spawn_was_interrupted(&err), "and says why, so it is not read as a failure");
            assert!(
                live_group_ids(Some(owner)).is_empty(),
                "nothing was spawned, so nothing was registered"
            );

            // The refusal is the run's state, not the command's: the same spawn
            // succeeds on a thread whose run was never asked to stop.
            set_run_owner(0);
            let supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@allowed").expect("spawn");
            let pgid = supervised.pgid;
            drop(supervised);
            wait_until("the group to be gone", || !pid_is_alive(pgid));
        }

        /// A run tearing its own groups down after a failure raises the same
        /// token an operator's signal does. Only the operator has an operator
        /// waiting to read the notice — and only they can be told, truthfully,
        /// that pressing Ctrl+C *again* skips the grace. Telling a failing run's
        /// operator they interrupted something points them away from the
        /// failure being reported.
        // §FS-rhei-run.3.2
        #[test]
        fn a_teardown_nobody_signalled_announces_nothing() {
            let token = StopToken::new();
            token.request();
            assert!(token.is_set(), "the run is still shutting down");
            assert_eq!(token.take_announcement(), None, "but nobody interrupted it");

            let signalled = StopToken::new();
            signalled.raise(Signal::SIGINT as i32);
            let notice = signalled.take_announcement().expect("an operator is told");
            assert!(notice.contains("Interrupted"), "notice was: {notice}");
            assert_eq!(signalled.take_announcement(), None, "and told exactly once");
        }
    }

    /// A stdout the process can no longer write to is not a bug in it: a
    /// pipeline's reader closed early, or — under the TUI — the operator's
    /// terminal went away and every later write to the dead pty returns `EIO`.
    /// Left unrecognized, the second one panicked the end-of-run summary and
    /// then panicked again from the report guard while unwinding, which aborts.
    // §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
    #[test]
    fn a_lost_stdout_is_recognized_by_message_and_by_errno() {
        let on_a_terminal = |_| true;
        assert!(lost_output_verdict(
            "failed printing to stdout: Broken pipe (os error 32)",
            on_a_terminal
        ));
        assert!(lost_output_verdict(
            "failed printing to stdout: Input/output error (os error 5)",
            on_a_terminal
        ));
        // The errno alone is enough, so a non-English `strerror` still matches.
        assert!(lost_output_verdict(
            "failed printing to stderr: <translated> (os error 5)",
            on_a_terminal
        ));

        // A different write failure, and a panic that merely mentions one, are
        // both real reports.
        assert!(!lost_output_verdict(
            "failed printing to stdout: No space left on device (os error 28)",
            on_a_terminal
        ));
        assert!(!lost_output_verdict("agent wrote to a Broken pipe (os error 32)", on_a_terminal));
    }

    /// `EIO` on a redirected stdout — a dropped network mount, a failing
    /// device — is a real write failure and must be reported. Swallowing it
    /// killed every in-flight agent and exited `141` with no report and no
    /// message, indistinguishable from `rhei run | head`.
    // §FS-rhei-run.3.2
    #[test]
    fn an_io_error_on_a_redirected_stdout_is_a_real_failure() {
        let redirected = |_| false;
        assert!(!lost_output_verdict(
            "failed printing to stdout: Input/output error (os error 5)",
            redirected
        ));
        // A closed pipe is a closed reader wherever it points, terminal or not.
        assert!(lost_output_verdict(
            "failed printing to stdout: Broken pipe (os error 32)",
            redirected
        ));
    }

    /// The whole `EIO` branch turns on "was this a terminal", and that question
    /// cannot be asked once the terminal has gone: the hangup swaps the pty
    /// slave's file operations out, so the `TCGETS` behind `isatty` fails with
    /// `EIO` like every other ioctl on it and the stream reads as *not* a
    /// terminal from exactly the moment the guard needs it to read as one.
    ///
    /// This pins the kernel behaviour the fix is built on. `stream_is_terminal`
    /// answers from a reading taken at startup for this reason; asking live
    /// would return `false` here and send a lost console down the panic path
    /// the guard exists to replace.
    // §FS-rhei-run.3.2
    #[cfg(target_os = "linux")]
    #[test]
    fn a_hung_up_pty_stops_answering_that_it_is_a_terminal() {
        use std::io::IsTerminal as _;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        // SAFETY: `openpty` fills both ends; each raw fd is owned exactly once.
        let (master, slave) = {
            let pty = nix::pty::openpty(None, None).expect("openpty");
            (pty.master, pty.slave)
        };
        let slave_file = unsafe { std::fs::File::from_raw_fd(slave.as_raw_fd()) };
        std::mem::forget(slave);
        assert!(slave_file.is_terminal(), "a live pty slave is a terminal");

        drop::<OwnedFd>(master);
        // The hangup is what the operator closing a terminal window does.
        assert!(
            !slave_file.is_terminal(),
            "a hung-up pty denies being a terminal, which is why the reading is taken at startup"
        );

        // And the write that produces the panic message really is `EIO`, not
        // `EPIPE`: without the startup reading, nothing would match it.
        let err = (&slave_file).write(b"x").expect_err("writing to a hung-up pty fails");
        assert_eq!(err.raw_os_error(), Some(nix::errno::Errno::EIO as i32));
        assert!(lost_output_verdict(
            &format!("failed printing to stdout: {err}"),
            |_| true
        ));
    }

    /// The stream decides whose `is_terminal` is asked, so a `stderr` panic
    /// must not be answered by looking at `stdout`.
    #[test]
    fn the_panic_message_names_the_stream_that_failed() {
        assert_eq!(
            printing_failure_stream("failed printing to stdout: Broken pipe (os error 32)"),
            Some(LostStream::Stdout)
        );
        assert_eq!(
            printing_failure_stream("failed printing to stderr: Broken pipe (os error 32)"),
            Some(LostStream::Stderr)
        );
        assert_eq!(printing_failure_stream("index out of bounds"), None);
    }
