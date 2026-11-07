// A ticket halted by a spent bound, at the level the end-to-end scenarios cannot
// reach: the console group, which needs a terminal, and the two edges no plain run
// produces on demand. Included into `mod run_summary_tests` in `run_summary.rs`, so
// the indentation is the module's and `use super::*` is already in scope from
// `tests_run_summary.rs`. Each is red until agent-grounds/rhei#587 is fixed; the
// scenarios it shares a defect with are in `tests/e2e/budget_halt_attention_tests.rs`.

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
  done: { description: Done, final: true }
transitions:
  - { from: work, to: done }
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
            remedy: BOUND_REMEDY.to_string(),
        });
    }

    /// The console group reads off the row the durable report does, on a ticket
    /// that ran and on one the run never spawned. The second is the one whose
    /// reason also reaches the ledger and the task tree.
    #[test]
    #[ignore = "red until #587: the console group gives the stall advice; implement removes this"]
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
    #[ignore = "red until #587: the row and the ledger give no remedy; implement removes this"]
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
    #[ignore = "red until #587: a bound-halted poll reads as Waiting; implement removes this"]
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
    #[ignore = "red until #587: no row names the bound at all yet; implement removes this"]
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

    fn emit_spawn_into(sink: &SummarySink, task: &str, state: &str) {
        use rhei_tui::EventSink;
        let log = std::path::PathBuf::from("runtime/logs/x.log");
        sink.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: task.to_string(),
            from: state.to_string(),
            to: state.to_string(),
            agent: Some("mock".to_string()),
            template_context: None,
            log_path: log.clone(),
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
        sink.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: task.to_string(),
            from: state.to_string(),
            to: state.to_string(),
            log_path: log,
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_200,
            configuration_fallbacks: Vec::new(),
            reverted: None,
        });
    }
