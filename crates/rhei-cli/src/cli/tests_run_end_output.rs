    // What a run says, or can no longer say, as it ends.

    /// A run tearing its own groups down after a failure raises the same
    /// token an operator's signal does. Only the operator has an operator
    /// waiting to read the notice — and only they can be told, truthfully,
    /// that pressing Ctrl+C *again* skips the grace. Telling a failing run's
    /// operator they interrupted something points them away from the
    /// failure being reported.
    // §FS-rhei-run.3.2
    #[cfg(unix)]
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
        use std::os::fd::FromRawFd;

        use nix::fcntl::OFlag;
        use nix::sys::stat::Mode;

        // Close-on-exec from the call that creates each end, or a process another test
        // spawns here keeps the master and the drop below hangs nothing up.
        // §REQ-cross-platform.7.2
        let flags = OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC;
        let master = nix::pty::posix_openpt(flags).expect("posix_openpt");
        nix::pty::grantpt(&master).expect("grantpt");
        nix::pty::unlockpt(&master).expect("unlockpt");
        let slave_name = nix::pty::ptsname_r(&master).expect("ptsname_r");
        let slave =
            nix::fcntl::open(slave_name.as_str(), flags, Mode::empty()).expect("open pty slave");
        // SAFETY: `open` just returned this descriptor and nothing else owns it.
        let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };
        assert!(slave_file.is_terminal(), "a live pty slave is a terminal");

        drop(master);
        // The hangup is what the operator closing a terminal window does. A process another
        // test forked after the open and not yet through its `exec` holds a copy of the master,
        // so it comes within a moment of the drop rather than at it. §REQ-cross-platform.7.2
        let deadline = Instant::now() + FIXTURE_MARGIN;
        while slave_file.is_terminal() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
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
