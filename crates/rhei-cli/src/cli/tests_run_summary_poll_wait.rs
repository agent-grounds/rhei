// A plain poll held only by its own future deadline (#736): the Waiting group,
// the count line, and the elapsed-deadline edge no plain run produces on
// demand. Included into `mod run_summary_tests` in `run_summary.rs`, so the
// indentation is the module's and `approval_report` comes from
// `tests_run_summary.rs`. The console, `--dry-run` and report rows are in
// `tests/e2e/until_idle_poll_wait_row_tests.rs`.

// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1

    /// `ci-watch` — a plain poll — with its next attempt seeded at `deadline`.
    fn plain_poll_report(deadline: u64) -> RunSummaryReport {
        approval_report(&format!(
            "---\nmetadata:\n  tasks:\n    1:\n      pollNextAttemptAt:\n        \
             ci-watch: {deadline}\n---\n\n### Task 1: Watch CI\n**State:** ci-watch\n"
        ))
    }

    /// A plain poll held only by its own future deadline is a wait rhei ends by
    /// itself: Waiting, calm, out of `could not advance`, and named by its
    /// instant rather than by never having been scheduled.
    // §FS-rhei-run-report.3.1 §FS-rhei-run.5.1
    #[test]
    #[ignore = "red until #736: a plain poll wait is classified NotScheduled and put in Attention; \
                implement removes this"]
    fn a_plain_poll_held_by_its_deadline_is_waiting_not_halted() {
        let report = plain_poll_report(1_893_456_000); // 2030-01-01T00:00:00Z

        assert!(report.attention.is_empty(), "a poll wait is not an action item");
        let row = report.waiting.first().expect("the poll wait is under Waiting");
        assert_eq!(row.reason, "waiting on its poll until 2030-01-01T00:00:00Z");
        assert_eq!(row.next, "nothing to do on Task plan.1; rhei resumes it itself");

        let tty = report.render_tty(false);
        assert!(tty.contains("Waiting    1 waiting on a poll"), "{tty}");
        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 0 |"), "{markdown}");
        assert!(!markdown.contains("not scheduled before the run halted"), "{markdown}");
    }

    /// A deadline already passed holds nothing, so it earns no poll row: the
    /// ticket keeps the reading it had. Holds today; it guards the rung's
    /// `pollNextAttemptAt > read_at` condition. §FS-rhei-run-report.3.1
    #[test]
    fn an_elapsed_poll_deadline_earns_no_poll_row() {
        let report = plain_poll_report(946_684_800); // 2000-01-01T00:00:00Z

        assert!(report.waiting.is_empty(), "an elapsed deadline is not a wait");
        let markdown = report.render_markdown();
        assert!(!markdown.contains("waiting on its poll"), "{markdown}");
    }

