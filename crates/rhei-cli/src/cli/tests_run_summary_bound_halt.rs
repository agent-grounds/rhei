// A ticket halted by a spent bound, at the level the end-to-end scenarios cannot
// reach: the console group, which needs a terminal, and the two edges no plain run
// produces on demand. Included into `mod run_summary_tests` in `run_summary.rs`, so
// the indentation is the module's and `use super::*` is already in scope from
// `tests_run_summary.rs`. The scenarios of agent-grounds/rhei#587 are in
// `tests/e2e/budget_halt_attention_tests.rs`.

// §FS-rhei-run-report.3.1 §FS-rhei-budgets.8 §FS-rhei-states.2.5

    const BOUND_REMEDY: &str = "set `defaults.transition_limit` in /m/settings.json";
    const BOUND_RENEWAL: &str = "2026-09-25T00:00:00Z";

    fn bound_halted_report(state: &str, sink: &SummarySink) -> RunSummaryReport {
        let rhei = rhei_core::parse(&format!(
            "# Rhei: Bounded\n\n## Tasks\n\n### Task 1: Work\n**State:** {state}\n"
        ))
        .expect("plan parses");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: bounded
version: 1
states:
  work: { description: Work, initial: true, program: "./work.sh" }
  plan-approval:
    description: Wait for the author
    program: "./check-reply.sh"
    poll: { interval: 10m, max_attempts: 60, waiting_on: author }
  review: { description: Wait for a reviewer, gating: true }
  done: { description: Done, final: true }
transitions:
  - { from: work, to: done }
  - { from: review, to: done }
  - { from: plan-approval, to: plan-approval }
  - { from: plan-approval, to: done }
"#,
        )
        .expect("valid state machine");
        RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            sink,
            test_stats(),
            "plan.rhei.md",
            &no_task_roots(),
        )
    }

    /// Task `1` refused by a bound of 4: a count, or a window where `renews_at` is set.
    fn emit_bound_halt(sink: &SummarySink, renews_at: Option<&str>) {
        emit_bound_halt_with(sink, renews_at, BOUND_REMEDY);
    }

    fn emit_bound_halt_with(sink: &SummarySink, renews_at: Option<&str>, remedy: &str) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::BudgetHalt {
            task: "1".to_string(),
            reason_code: "travel_exhausted".to_string(),
            bound: rhei_tui::BoundReport {
                dimension: "ticket travel".to_string(),
                effective: 4,
                value_source: "machine".to_string(),
                limiting_source: None,
                consumed: 4,
                outstanding: 0,
                remaining: 0,
                mode: "per ticket identity".to_string(),
                window: None,
                currency: None,
                marks: None,
                ceiling: None,
            },
            renews_at: renews_at.map(str::to_string),
            remedy: remedy.to_string(),
        });
    }

    /// The console group reads off the row the durable report does, on a ticket
    /// that ran and on one the run never spawned. The second is the one whose
    /// reason also reaches the ledger and the task tree.
    #[test]
    fn the_console_group_names_the_bound_of_a_halted_ticket() {
        for sink in [summary_with_spawn("1", "work", "work", true), SummarySink::new()] {
            emit_bound_halt(&sink, Some(BOUND_RENEWAL));

            let tty = bound_halted_report("work", &sink).render_tty(false);

            assert!(tty.contains("ticket travel"), "the blocker names the dimension:\n{tty}");
            assert!(tty.contains(BOUND_RENEWAL), "the blocker names when it renews:\n{tty}");
            assert!(tty.contains("→ renews automatically"), "{tty}");
            for wrong in ["stalled in non-terminal", "inspect logs", "not scheduled", "rerun"] {
                assert!(!tty.contains(wrong), "the console still gives {wrong:?}:\n{tty}");
            }
        }
    }

    /// Without a renewal the next action is the event's own remedy verbatim, and a
    /// ticket the run never spawned explains itself the same way in the ledger.
    #[test]
    fn a_count_halt_carries_the_events_remedy_into_the_row_and_the_ledger() {
        let sink = SummarySink::new();
        emit_bound_halt(&sink, None);

        let report = bound_halted_report("work", &sink);

        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert!(row.reason.contains("ticket travel") && row.reason.contains("4 (machine)"), "{:?}", row.reason);
        assert_eq!(row.next, BOUND_REMEDY);
        let blocked = report.ledger.iter().find(|entry| entry.driver == "blocked");
        assert!(blocked.is_some_and(|entry| entry.reason == row.reason), "the ledger reads as the row");
    }

    /// A refused start is why the ticket is not moving, so it outranks the person
    /// the poll waits on: the author's answer would not release it.
    #[test]
    fn a_bound_halted_poll_waiting_on_a_person_is_in_attention() {
        let sink = SummarySink::new();
        emit_bound_halt(&sink, None);

        let report = bound_halted_report("plan-approval", &sink);

        assert!(report.waiting.is_empty(), "a bound-halted ticket is not in Waiting");
        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert!(row.reason.contains("ticket travel"), "{:?}", row.reason);
        assert_eq!(row.next, BOUND_REMEDY);
        assert!(!row.is_gate, "it is halted work");
        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 1 |"), "{markdown}");
    }

    /// The event carries no state, so a halt counts only while it is still true. A
    /// spawn before the halt leaves it standing, which is every real halt's shape
    /// and what makes this test red today. A spawn after it means the ticket was
    /// admitted again, and whatever stopped it next is its own story. `## Bounds`
    /// keeps the history either way. An old stall never explains a new one.
    #[test]
    fn a_ticket_admitted_again_after_a_bound_halt_does_not_keep_the_bound_row() {
        let halted_last = summary_with_spawn("1", "work", "work", true);
        emit_bound_halt(&halted_last, None);
        let report = bound_halted_report("work", &halted_last);
        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert!(row.reason.contains("ticket travel"), "the halt is still true: {:?}", row.reason);

        let spawned_last = SummarySink::new();
        emit_bound_halt(&spawned_last, None);
        emit_spawn_into(&spawned_last, "1", "work");
        let report = bound_halted_report("work", &spawned_last);
        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert!(!row.reason.contains("ticket travel"), "the halt is stale: {:?}", row.reason);
        assert_ne!(row.next, BOUND_REMEDY, "the halt is stale: {:?}", row.reason);
        assert!(report.render_markdown().contains("was stopped by its ticket travel bound"));
    }

    /// §FS-rhei-budgets.8's long form: one line with backticks and ordered steps. A
    /// plan path may hold a pipe, so the table cell escapes it, and the console takes
    /// the text as it is on a single `→` line.
    #[test]
    fn a_long_lifetime_remedy_is_one_row_and_one_console_line() {
        let remedy = "set `defaults.lifetime_max` in /m/settings.json above 2, then run \
                      `rhei budget adjust 'a|b.rhei.md' --invocations <N> --reason <TEXT>`";
        let sink = summary_with_spawn("1", "work", "work", true);
        emit_bound_halt_with(&sink, None, remedy);

        let report = bound_halted_report("work", &sink);

        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert_eq!(row.next, remedy, "the row holds the record's remedy verbatim");
        let markdown = report.render_markdown();
        let cell = remedy.replace('|', "\\|");
        let rows: Vec<&str> = markdown.lines().filter(|line| line.starts_with("| 1 | work |")).collect();
        assert!(
            rows.iter().any(|line| line.ends_with(&format!("| {cell} |"))),
            "one intact row:\n{markdown}"
        );
        assert!(
            report.render_tty(false).lines().any(|line| line.trim() == format!("→ {remedy}")),
            "one console line"
        );
    }

    /// A move with no slot: only the plan shows it, and a refused start does not leave
    /// a ticket final, at a gate, or claimed, so the classifier's reading stands.
    #[test]
    fn a_ticket_that_moved_on_with_no_slot_does_not_keep_the_bound_row() {
        let sink = SummarySink::new();
        emit_bound_halt(&sink, None);

        let report = bound_halted_report("done", &sink);
        assert!(report.attention.iter().all(|row| row.id != "1"), "a final ticket has no row");

        let report = bound_halted_report("review", &sink);
        let row = report.attention.iter().find(|row| row.id == "1").expect("a gate row");
        assert_eq!(row.reason, "gating state awaiting review");
        assert!(row.is_gate, "the gate, not halted work");

        let report = bound_halted_report("work\n**Assignee:** someone", &sink);
        let row = report.attention.iter().find(|row| row.id == "1").expect("a claim row");
        assert_eq!(row.reason, "claimed by someone");

        assert!(report.render_markdown().contains("was stopped by its ticket travel bound"));
    }

    /// A release says nothing about where a ticket went: `from` is the raw plan
    /// state and `to` the normalized name of the same state. So a counted state's
    /// worker, in flight across the refusal, leaves the halt standing when it ends.
    /// This is the order a fan-out over `visits:` logs.
    #[test]
    fn a_counted_states_release_across_a_refusal_leaves_the_bound_row() {
        let sink = SummarySink::new();
        emit_assign_into(&sink, "1", "work-2", "work");
        emit_bound_halt(&sink, None);
        emit_release_into(&sink, "1", "work-2", "work");

        let report = bound_halted_report("work", &sink);

        let row = report.attention.iter().find(|row| row.id == "1").expect("an Attention row");
        assert_eq!(row.next, BOUND_REMEDY, "the ticket has not moved, so the halt is still true");
    }

    fn emit_spawn_into(sink: &SummarySink, task: &str, state: &str) {
        emit_assign_into(sink, task, state, state);
        emit_release_into(sink, task, state, state);
    }

    fn emit_assign_into(sink: &SummarySink, task: &str, from: &str, to: &str) {
        use rhei_tui::EventSink;
        let log = std::path::PathBuf::from("runtime/logs/x.log");
        sink.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            agent: Some("mock".to_string()),
            template_context: None,
            log_path: log,
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
    }

    fn emit_release_into(sink: &SummarySink, task: &str, from: &str, to: &str) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            log_path: std::path::PathBuf::from("runtime/logs/x.log"),
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_200,
            configuration_fallbacks: Vec::new(),
            reverted: None,
        });
    }
