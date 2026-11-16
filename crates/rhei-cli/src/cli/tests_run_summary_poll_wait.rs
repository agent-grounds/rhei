// A plain poll held only by its own future deadline (#736): the Waiting group,
// the count line, the two edges no plain run produces on demand — an elapsed
// deadline, and a bound that refused the ticket in the state it still polls in —
// and the missing input that outranks the deadline. Included into `mod
// run_summary_tests` in `run_summary.rs`, so the indentation is the module's,
// `approval_plan_report` comes from `tests_run_summary.rs` and the event helpers
// from `tests_run_summary_bound_halt.rs`. The console, `--dry-run` and report
// rows are in `tests/e2e/until_idle_poll_wait_row_tests.rs`.

// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1

    const FUTURE_DEADLINE: u64 = 1_893_456_000; // 2030-01-01T00:00:00Z
    const ELAPSED_DEADLINE: u64 = 946_684_800; // 2000-01-01T00:00:00Z

    /// Task `1` in `ci-watch` — a plain poll — with its next attempt seeded at
    /// `deadline`. The frontmatter sits under the `# Rhei:` header, the one place
    /// the parser reads it.
    fn plain_poll_report_with(deadline: u64, sink: &SummarySink) -> RunSummaryReport {
        approval_plan_report(
            &format!(
                "# Rhei: Approvals\n\n---\nmetadata:\n  tasks:\n    1:\n      pollNextAttemptAt:\n        \
                 ci-watch: {deadline}\n---\n\n## Tasks\n\n### Task 1: Watch CI\n**State:** ci-watch\n"
            ),
            sink,
        )
    }

    fn plain_poll_report(deadline: u64) -> RunSummaryReport {
        plain_poll_report_with(deadline, &SummarySink::new())
    }

    /// A plain poll held only by its own future deadline is a wait rhei ends by
    /// itself: Waiting, calm, out of `could not advance`, and named by its
    /// instant rather than by never having been scheduled.
    // §FS-rhei-run-report.3.1 §FS-rhei-run.5.1
    #[test]
    fn a_plain_poll_held_by_its_deadline_is_waiting_not_halted() {
        let report = plain_poll_report(FUTURE_DEADLINE);

        assert!(report.attention.is_empty(), "a poll wait is not an action item");
        let row = report.waiting.first().expect("the poll wait is under Waiting");
        assert_eq!(row.id, "1");
        assert_eq!(row.reason, "waiting on its poll until 2030-01-01T00:00:00Z");
        assert_eq!(row.next, "nothing to do on Task 1; rhei resumes it itself");

        let tty = report.render_tty(false);
        assert!(tty.contains("Waiting    1 waiting on a poll"), "{tty}");
        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 0 |"), "{markdown}");
        assert!(
            markdown.contains("- ⏸ `1` (ci-watch) — waiting on its poll until 2030-01-01T00:00:00Z"),
            "the Final States line carries the calm marker:\n{markdown}"
        );
        assert!(!markdown.contains("not scheduled before the run halted"), "{markdown}");
    }

    /// A deadline already passed holds nothing, so it earns no poll row: the
    /// ticket keeps the reading it had. The same fixture with a future deadline
    /// does earn it, so the guard is the rung's `pollNextAttemptAt > read_at`
    /// condition and not a deadline the fixture never seeded. §FS-rhei-run-report.3.1
    #[test]
    fn an_elapsed_poll_deadline_earns_no_poll_row() {
        let future = plain_poll_report(FUTURE_DEADLINE);
        assert!(
            future.waiting.iter().any(|row| row.reason.starts_with("waiting on its poll until")),
            "the fixture's deadline is read"
        );

        let report = plain_poll_report(ELAPSED_DEADLINE);

        assert!(report.waiting.is_empty(), "an elapsed deadline is not a wait");
        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert_eq!(row.reason, "not scheduled before the run halted, still in 'ci-watch'");
        let markdown = report.render_markdown();
        assert!(!markdown.contains("waiting on its poll"), "{markdown}");
    }

    /// A bound refused the ticket in `ci-watch` and it still polls there with a
    /// future deadline — a fan-out's order: one arm assigned, the other refused,
    /// the first released. At that deadline the bound refuses it again, so the
    /// bound's row stands and the poll row would promise a resumption that never
    /// comes. §FS-rhei-run-report.3.1
    #[test]
    fn a_bound_halt_in_the_poll_state_outranks_its_future_deadline() {
        use rhei_tui::EventSink;
        let sink = SummarySink::new();
        emit_assign_into(&sink, "1", "ci-watch", "ci-watch");
        sink.emit(bound_halt_record(Some("ci-watch")));
        emit_release_into(&sink, "1", "ci-watch", "ci-watch");

        let report = plain_poll_report_with(FUTURE_DEADLINE, &sink);

        assert!(report.waiting.is_empty(), "a bound-halted ticket is not in Waiting");
        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert!(row.reason.contains("ticket travel"), "{:?}", row.reason);
        assert_eq!(row.next, BOUND_REMEDY);
        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 1 |"), "{markdown}");
        assert!(!markdown.contains("waiting on its poll"), "{markdown}");
    }

    /// Task `1` in `checked` — a plain program poll whose state requires
    /// `runtime/brief.md` — with its next attempt seeded at `deadline`, its
    /// inputs read under `root`.
    fn poll_needing_input_report(deadline: u64, root: &std::path::Path) -> RunSummaryReport {
        let rhei = rhei_core::parse(&format!(
            "# Rhei: Inputs\n\n---\nmetadata:\n  tasks:\n    1:\n      pollNextAttemptAt:\n        \
             checked: {deadline}\n---\n\n## Tasks\n\n### Task 1: Check the brief\n**State:** checked\n"
        ))
        .expect("plan parses");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: poll-needs-input
version: 1
states:
  checked:
    description: Poll once the brief is written
    initial: true
    program: "./check.sh"
    poll: { interval: 2m, max_attempts: 30 }
    inputs:
      - name: brief
        path: runtime/brief.md
  done: { description: terminal, final: true }
transitions:
  - { from: checked, to: checked }
  - { from: checked, to: done }
"#,
        )
        .expect("valid state machine");
        let runtime = root.join("runtime");
        RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            &SummarySink::new(),
            test_stats(),
            "plan.rhei.md",
            &SpawnSiteRoots::plan_only(root, &runtime),
        )
    }

    /// A plain poll missing its required input is held by the input, not by its
    /// future deadline: no clock releases a missing file, so the row names the
    /// input and the poll row would promise a resumption that never comes. With
    /// the input written the deadline is the wait again, so the guard is the
    /// input and not a deadline the fixture never seeded.
    // §FS-rhei-run.5.1.1 §FS-rhei-run-report.3.1
    #[test]
    fn a_plain_poll_missing_its_input_is_reported_for_the_input() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let report = poll_needing_input_report(FUTURE_DEADLINE, dir.path());

        assert!(report.waiting.is_empty(), "a missing input is not a wait");
        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert!(row.reason.starts_with("required input(s) not found: "), "{:?}", row.reason);
        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 1 |"), "{markdown}");
        assert!(!markdown.contains("waiting on its poll"), "{markdown}");
        assert!(!markdown.contains("rhei resumes it itself"), "{markdown}");
        assert!(!markdown.contains("- ⏸ `1`"), "no calm marker for Task 1:\n{markdown}");

        std::fs::create_dir_all(dir.path().join("runtime")).expect("runtime directory");
        std::fs::write(dir.path().join("runtime/brief.md"), "# Brief\n").expect("write the brief");
        let written = poll_needing_input_report(FUTURE_DEADLINE, dir.path());
        assert!(written.attention.is_empty(), "with the input written the poll alone holds it");
        let row = written.waiting.first().expect("the poll wait is under Waiting");
        assert_eq!(row.reason, "waiting on its poll until 2030-01-01T00:00:00Z");
    }
