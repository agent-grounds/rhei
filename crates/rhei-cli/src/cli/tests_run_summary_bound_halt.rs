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

    /// A halt of `task` on `dimension`, consuming `consumed` of a bound of 4 from
    /// `value_source`, and a window where `renews_at` is set.
    fn emit_distinct_halt(
        sink: &SummarySink,
        (task, dimension, value_source): (&str, &str, &str),
        renews_at: Option<&str>,
        consumed: u64,
    ) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::BudgetHalt {
            task: task.to_string(),
            reason_code: "travel_exhausted".to_string(),
            bound: rhei_tui::BoundReport {
                dimension: dimension.to_string(),
                effective: 4,
                value_source: value_source.to_string(),
                limiting_source: None,
                consumed,
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

    /// The halt lines of the report, in printed order.
    fn halt_lines(report: &RunSummaryReport) -> Vec<String> {
        let markdown = report.render_markdown();
        let halts = markdown.lines().filter(|line| line.contains("was stopped by its"));
        halts.map(str::to_owned).collect()
    }

    const NEXT_RENEWAL: &str = "2026-09-26T00:00:00Z";

    /// A stop is what a line says, so any difference a line shows is a new one, kept
    /// where it first appeared. Green today: this guards the fix against collapsing
    /// too much, as no real run has built such a pair yet.
    #[test]
    fn halts_differing_in_ticket_dimension_provenance_or_renewal_stay_separate_lines() {
        let sink = SummarySink::new();
        let renewing = Some(BOUND_RENEWAL);
        emit_distinct_halt(&sink, ("1", "project spend", "machine"), renewing, 4);
        emit_distinct_halt(&sink, ("1", "project invocations", "machine"), renewing, 4);
        emit_distinct_halt(&sink, ("1", "project invocations", "project"), renewing, 4);
        emit_distinct_halt(&sink, ("1", "project invocations", "project"), Some(NEXT_RENEWAL), 4);
        emit_distinct_halt(&sink, ("2", "project invocations", "project"), Some(NEXT_RENEWAL), 4);

        let report = bound_halted_report("work", &sink);

        let renews = |at: &str| format!("it renews at {at}");
        assert_eq!(
            halt_lines(&report),
            [
                format!(
                    "- `1` was stopped by its project spend bound of 4 (machine); {}",
                    renews(BOUND_RENEWAL)
                ),
                format!(
                    "- `1` was stopped by its project invocations bound of 4 (machine); {}",
                    renews(BOUND_RENEWAL)
                ),
                format!(
                    "- `1` was stopped by its project invocations bound of 4 (project); {}",
                    renews(BOUND_RENEWAL)
                ),
                format!(
                    "- `1` was stopped by its project invocations bound of 4 (project); {}",
                    renews(NEXT_RENEWAL)
                ),
                format!(
                    "- `2` was stopped by its project invocations bound of 4 (project); {}",
                    renews(NEXT_RENEWAL)
                ),
            ]
        );
    }

    /// Two tickets refused on each of two passes alternate, so comparing a line with
    /// the one before it would miss every repeat. `consumed` differs between passes
    /// and no line shows it, so it cannot split a stop.
    #[test]
    fn a_repeated_halt_is_one_line_in_the_place_it_first_appeared() {
        let sink = SummarySink::new();
        let first = ("1", "ticket travel", "machine");
        let second = ("2", "ticket travel", "machine");
        emit_distinct_halt(&sink, first, None, 4);
        emit_distinct_halt(&sink, second, None, 4);
        emit_distinct_halt(&sink, first, None, 5);
        emit_distinct_halt(&sink, second, None, 5);

        let report = bound_halted_report("work", &sink);

        assert_eq!(
            halt_lines(&report),
            [
                format!("- `1` was stopped by its ticket travel bound of 4 (machine); {BOUND_REMEDY}"),
                format!("- `2` was stopped by its ticket travel bound of 4 (machine); {BOUND_REMEDY}"),
            ]
        );
    }

    /// A `budget_halt` as the stream carries it, from the state a refusal named.
    /// Decoded rather than built, so the same record serves one that predates the
    /// field: `None` writes none. §FS-rhei-run-json.2.1
    fn bound_halt_record(state: Option<&str>) -> rhei_tui::RunEvent {
        let mut record = serde_json::json!({
            "seq": 1,
            "ts": "2026-09-24T12:00:00Z",
            "event": "budget_halt",
            "task": "1",
            "reason_code": "travel_exhausted",
            "dimension": "ticket travel",
            "effective": 4,
            "value_source": "machine",
            "limiting_source": null,
            "consumed": 4,
            "outstanding": 0,
            "remaining": 0,
            "mode": "per ticket identity",
            "window": null,
            "renews_at": null,
            "remedy": BOUND_REMEDY,
        });
        if let Some(state) = state {
            record["state"] = serde_json::json!(state);
        }
        rhei_tui::decode_event(&record.to_string()).expect("a budget_halt record").event
    }

    /// Whether task `1` still reads as the bound's, after these refusals, from `state`.
    fn bound_row_stands(refused_in: &[Option<&str>], state: &str) -> bool {
        use rhei_tui::EventSink;
        let sink = SummarySink::new();
        for refused in refused_in {
            sink.emit(bound_halt_record(*refused));
        }
        let report = bound_halted_report(state, &sink);
        assert!(
            report.render_markdown().contains("was stopped by its ticket travel bound"),
            "## Bounds keeps every halt as history"
        );
        report.attention.iter().any(|row| row.id == "1" && row.next == BOUND_REMEDY)
    }

    /// Refused in `work`, then moved by what was already running: no later slot
    /// clears it, and the plan's state is the only thing that shows the move.
    /// §FS-rhei-run-report.3.1
    #[test]
    #[ignore = "red until #603: the overlay does not compare the refused state; implement removes this"]
    fn a_move_after_the_refusal_takes_the_bound_row_off_the_ticket() {
        assert!(!bound_row_stands(&[Some("work")], "plan-approval"));
    }

    /// The first pair fails an implementation that keeps the first refusal, the
    /// second one that keeps any of them. §FS-rhei-run-report.3.1
    #[test]
    #[ignore = "red until #603: the overlay does not compare the refused state; implement removes this"]
    fn the_latest_refusals_state_is_the_one_compared() {
        assert!(bound_row_stands(&[Some("work"), Some("plan-approval")], "plan-approval"));
        assert!(!bound_row_stands(&[Some("plan-approval"), Some("work")], "plan-approval"));
    }

    /// Green today, and has to stay so: `work-2` is `work`. §FS-rhei-run-report.3.1
    #[test]
    fn a_counted_state_is_the_state_the_bound_refused_it_in() {
        assert!(bound_row_stands(&[Some("work")], "work-2"));
    }

    /// Green today, and has to stay so: a record from before the field proves no
    /// move, and replaying it invents no state. §FS-rhei-run-report.3.1
    #[test]
    fn a_record_with_no_state_takes_no_halt_back() {
        assert!(bound_row_stands(&[None], "plan-approval"));
        let at = std::time::SystemTime::now();
        let encoded = rhei_tui::encode_event(Some(1), &bound_halt_record(None), at, None);
        assert!(encoded["state"].is_null(), "{encoded}");
    }

    /// `rhei attach --json` re-emits what it decoded, unchanged. §FS-rhei-run-json.2
    #[test]
    #[ignore = "red until #603: decoding drops `state`; implement removes this"]
    fn a_replayed_budget_halt_keeps_its_state() {
        let at = std::time::SystemTime::now();
        let encoded = rhei_tui::encode_event(Some(1), &bound_halt_record(Some("work")), at, None);
        assert_eq!(encoded["state"], "work", "{encoded}");
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
