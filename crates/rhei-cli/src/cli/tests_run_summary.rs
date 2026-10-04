// The run report's own unit coverage: the classification it gives one ticket,
// the groups a ticket lands in, and what each renderer writes for it. Kept
// beside `run_summary.rs` rather than in it, so that file is the renderer and
// its model. Included into `mod run_summary_tests` there, so the indentation
// is the module's, not this file's.

// §AR-source-file-size.3 §FS-rhei-run-report

    use super::*;

    fn machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::builtin_default()
    }

    /// A single-file plan gives its tickets no execution roots of their own.
    fn no_task_roots() -> std::collections::HashMap<String, std::path::PathBuf> {
        std::collections::HashMap::new()
    }

    /// Parse a tiny plan whose tasks carry the given `(id, state)` pairs.
    fn report(tasks: &[(&str, &str)]) -> RunSummaryReport {
        let mut md = String::from("# Rhei: Test Plan\n\n## Tasks\n\n");
        for (id, state) in tasks {
            md.push_str(&format!("### Task {id}: Task {id}\n**State:** {state}\n\n"));
        }
        let rhei = rhei_core::parse(&md).expect("plan parses");
        RunSummaryReport::build(&rhei, &rhei_validator::MachineSet::single(machine()), &SummarySink::new(), test_stats(), "plan.rhei.md", &no_task_roots())
    }

    /// `RunStats` with non-zero spawn counts and empty run metadata, for the
    /// renderer tests that do not exercise the durable header.
    fn test_stats() -> RunStats {
        RunStats {
            agents_spawned: 2,
            programs_spawned: 3,
            callback_only: 0,
            duration: Some(std::time::Duration::from_secs(5)),
            dashboard: None,
            run_id: "abc123".to_string(),
            started_at: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_749_115_351)),
            workspace_root: std::path::PathBuf::from("examples/test"),
            command: "rhei run .".to_string(),
            parallel: 4,
            mode: "agent",
            initial_states: HashMap::new(),
            dry_run: false,
            interrupted: false,
            stop: None,
        }
    }

    #[test]
    fn markers_classify_by_state_class() {
        let m = machine();
        assert_eq!(classify_marker("completed", &m), Marker::Done);
        assert_eq!(classify_marker("blocked", &m), Marker::Attention);
        assert_eq!(classify_marker("cancelled", &m), Marker::Cancelled);
    }

    /// A parent halted only because its own subtree is open is the eligibility
    /// rule working, so it reads as a calm pause. Classifying by state alone
    /// turned every ancestor of one gated leaf into its own red Attention row.
    // §FS-rhei-run-report.3.2
    #[test]
    fn a_parent_waiting_on_its_subtree_reads_as_a_calm_pause() {
        let m = machine();
        let mut causes: HashMap<String, HaltCause> = HashMap::new();
        causes.insert(
            "plan.1".to_string(),
            HaltCause::WaitingOnDescendants { open: "Task plan.1.1 (human-gate)".to_string() },
        );
        causes.insert("plan.2".to_string(), HaltCause::Stalled);

        // Same state, same machine: only the halt cause separates the two.
        assert_eq!(classify_marker("pending", &m), Marker::Attention);
        assert_eq!(marker_for_task("plan.1", "pending", &m, &causes), Marker::Gate);
        assert_eq!(marker_for_task("plan.2", "pending", &m, &causes), Marker::Attention);
        assert_eq!(marker_for_task("plan.3", "pending", &m, &causes), Marker::Attention);

        // The reason still names the descendants, and the row still counts as
        // a gate rather than as something broken.
        let (reason, _) = attention_reason(Marker::Gate, "plan.1", "pending", &causes);
        assert!(
            reason.contains("waiting on open descendant Task plan.1.1 (human-gate)"),
            "{reason}"
        );
    }

    /// The ticket's own machine: a poll that waits on the author beside one
    /// that waits on CI, so a person wait and a machine backoff can block each
    /// other in one plan. §FS-rhei-states.2.5
    fn approval_report(tasks: &str) -> RunSummaryReport {
        let rhei = rhei_core::parse(&format!("# Rhei: Approvals\n\n## Tasks\n\n{tasks}"))
            .expect("plan parses");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: approvals
version: 1
states:
  plan-approval:
    description: Wait for the author
    initial: true
    program: "./check-reply.sh"
    poll: { interval: 10m, max_attempts: 60, waiting_on: author }
  ci-watch:
    description: Wait for CI
    program: "./check-ci.sh"
    poll: { interval: 2m, max_attempts: 30 }
  done: { description: terminal, final: true }
transitions:
  - { from: plan-approval, to: plan-approval }
  - { from: plan-approval, to: done }
  - { from: ci-watch, to: ci-watch }
  - { from: ci-watch, to: done }
"#,
        )
        .expect("valid state machine");
        RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            &SummarySink::new(),
            test_stats(),
            "plan.rhei.md",
            &no_task_roots(),
        )
    }

    /// A poll waiting on a person is nobody's action item: it goes under
    /// Waiting beside held tickets, keeps a calm marker, stays out of the
    /// `N gated · M blocked` header and `could not advance`, and gives the
    /// ledger the label rather than a stall. Before this it was simply an
    /// active state, indistinguishable from an agent that was running.
    // §FS-rhei-states.2.5 §FS-rhei-run-report.3.1 §FS-rhei-run-report.4
    #[test]
    fn a_poll_waiting_on_a_person_is_parked_not_halted() {
        let report =
            approval_report("### Task 1: Get the plan approved\n**State:** plan-approval\n");

        assert!(report.attention.is_empty(), "a person wait is not an action item");
        assert_eq!(
            report.waiting.iter().map(|w| w.reason.as_str()).collect::<Vec<_>>(),
            vec!["waiting on author"]
        );
        assert_eq!(
            report.ledger.iter().find(|e| e.driver == "blocked").map(|e| e.reason.as_str()),
            Some("waiting on author"),
            "the ledger explains it the way the summary did"
        );

        let tty = report.render_tty(false);
        assert!(tty.contains("Waiting    1 waiting on a person"), "{tty}");
        assert!(!tty.contains("Attention"), "{tty}");

        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 0 |"), "{markdown}");
        assert!(markdown.contains("waiting on author"), "{markdown}");
    }

    /// An unsatisfied prior stops the poll from ever reaching its next
    /// attempt, so the prior — not the person — is why the ticket is not
    /// moving. Classified the other way round, a ticket a prior really blocks
    /// read as calmly parked, with a promise that the author's answer would
    /// release it. It stays in Attention and keeps counting.
    // §FS-rhei-run-report.3.1 §FS-rhei-states.2.5
    #[test]
    fn a_prior_outranks_the_person_a_poll_waits_on() {
        let report = approval_report(
            "### Task 1: Watch CI\n**State:** ci-watch\n\n\
             ### Task 2: Get the plan approved\n**State:** plan-approval\n**Prior:** 1\n",
        );

        let row = report
            .attention
            .iter()
            .find(|row| row.state == "plan-approval")
            .expect("a blocked ticket keeps its Attention row");
        assert_eq!(row.reason, "waiting on Task 1 (ci-watch)");
        assert_eq!(row.next, "finish the prior first");
        assert!(!row.is_gate, "an unsatisfied prior is not a deliberate pause");
        assert!(report.waiting.is_empty(), "nothing here is parked");

        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 2 |"), "{markdown}");
        assert!(!markdown.contains("the poll resumes itself"), "{markdown}");
    }

    /// A live claim is the same story with a different remedy: `rhei release`
    /// is what unblocks the ticket, and the person the poll names cannot
    /// deliver it. Hiding the claim behind the person wait left the operator a
    /// single row telling them to do nothing.
    // §FS-rhei-run-report.3.1 §FS-rhei-states.2.5
    #[test]
    fn a_live_claim_outranks_the_person_a_poll_waits_on() {
        let report = approval_report(
            "### Task 1: Get the plan approved\n**State:** plan-approval\n**Assignee:** bot\n",
        );

        let row = report.attention.first().expect("a claimed ticket is an action item");
        assert_eq!(row.reason, "claimed by bot");
        assert!(row.next.contains("rhei release 1"), "{}", row.next);
        assert!(report.waiting.is_empty(), "a claimed ticket is not parked");
    }

    /// The Waiting group can hold both kinds at once, and its count line names
    /// each rather than calling every row "held". §FS-rhei-run-report.3.1
    #[test]
    fn the_waiting_tally_names_each_kind_it_holds() {
        let row = |waits_on_person: bool, provider_limited: bool| AttentionRow {
            id: "1".to_string(),
            state: "s".to_string(),
            reason: "r".to_string(),
            next: "n".to_string(),
            is_gate: true,
            waits_on_person,
            provider_limited,
        };
        assert_eq!(waiting_tally(&[row(false, false), row(false, false)]), "2 held");
        assert_eq!(waiting_tally(&[row(true, false)]), "1 waiting on a person");
        assert_eq!(waiting_tally(&[row(false, true)]), "1 provider-limited");
        assert_eq!(
            waiting_tally(&[row(false, false), row(true, false), row(false, true)]),
            "1 held \u{b7} 1 waiting on a person \u{b7} 1 provider-limited"
        );
    }

    /// One gated leaf under three ancestors is one thing needing a human, so
    /// the report counts it once. Treating each ancestor as halted work of its
    /// own gave four Attention rows, `4 gated`, `could not advance | 4`, and
    /// four blocked ledger rows for a single decision — and the topmost
    /// parent's reason text repeated the whole transitive subtree.
    // §FS-rhei-run-report.3.1 §FS-rhei-run-report.4 §FS-rhei-plan-language.3
    #[test]
    fn one_gate_under_three_ancestors_is_counted_once() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Deep Subtree
---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Top
**State:** work

#### Task 1.1: Middle
**State:** work

##### Task 1.1.1: Inner
**State:** work

###### Task 1.1.1.1: Gated leaf
**State:** human-gate
"#,
        )
        .expect("plan parses");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: t
version: 1
states:
  work:
    initial: true
    description: work
  human-gate:
    description: awaiting a human
    gating: true
  done:
    description: terminal
    final: true
transitions:
  - from: work
    to: done
  - from: human-gate
    to: done
"#,
        )
        .expect("valid state machine");
        let report = RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            &SummarySink::new(),
            test_stats(),
            "plan.rhei.md",
            &no_task_roots(),
        );

        assert_eq!(
            report.attention.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            vec!["1.1.1.1"],
            "only the gate itself is halted work"
        );

        let tty = report.render_tty(false);
        assert!(tty.contains("Attention  1 gated · 0 blocked"), "{tty}");

        let markdown = report.render_markdown();
        assert!(markdown.contains("| could not advance | 1 |"), "{markdown}");
        assert_eq!(
            report.ledger.iter().filter(|e| e.driver == "blocked").count(),
            1,
            "one blocked ledger row, not one per ancestor"
        );

        // The ancestors stay visible in the tree, calm and specific about what
        // holds them. §FS-rhei-run-report.3.2
        for id in ["1", "1.1", "1.1.1"] {
            let row = report.rows.iter().find(|r| r.id == id).expect("row present");
            assert_eq!(row.marker, Marker::Gate, "{id}");
            assert!(
                row.detail.as_deref().is_some_and(|d| d.contains("waiting on open descendant")),
                "{id}: {:?}",
                row.detail
            );
        }
    }

    /// A parent that is itself blocked keeps its own attention marker: that is
    /// wrong independently of whatever its children are doing.
    // §FS-rhei-run-report.3.2
    #[test]
    fn a_failed_parent_keeps_its_attention_marker() {
        let m = machine();
        let mut causes: HashMap<String, HaltCause> = HashMap::new();
        causes.insert(
            "plan.1".to_string(),
            HaltCause::WaitingOnDescendants { open: "Task plan.1.1 (pending)".to_string() },
        );
        assert_eq!(marker_for_task("plan.1", "blocked", &m, &causes), Marker::Attention);
    }

    #[test]
    fn plain_render_lists_every_task_with_state() {
        let r = report(&[("1", "completed"), ("2", "blocked")]);
        let out = r.render_tty(false);
        assert!(out.contains("Run Report"), "{out}");
        assert!(out.contains("Test Plan"), "{out}");
        assert!(out.contains("completed"), "{out}");
        assert!(out.contains("blocked"), "{out}");
        // No ANSI escapes when color is disabled.
        assert!(!out.contains('\x1b'), "{out}");
    }

    #[test]
    fn attention_block_surfaces_blocked_tasks() {
        let r = report(&[("1", "completed"), ("2", "blocked")]);
        let out = r.render_tty(false);
        assert!(out.contains("Attention"), "{out}");
        assert!(out.contains("1 blocked"), "{out}");
        assert!(out.contains("stopped for human attention"), "{out}");
    }

    #[test]
    fn all_completed_reads_as_completed() {
        let r = report(&[("1", "completed"), ("2", "completed")]);
        let out = r.render_tty(false);
        assert!(out.contains("completed"), "{out}");
        assert!(!out.contains("Attention"), "{out}");
    }

    #[test]
    fn color_render_emits_ansi() {
        let r = report(&[("1", "blocked")]);
        let out = r.render_tty(true);
        assert!(out.contains('\x1b'), "expected ANSI escapes");
    }

    #[test]
    fn duration_formats_short_and_long() {
        assert_eq!(format_duration_short(200), "0.2s");
        assert_eq!(format_duration_short(8_100), "8.1s");
        assert_eq!(format_duration_short(65_000), "1m05s");
        assert_eq!(format_duration_long(std::time::Duration::from_secs(724)), "12m04s");
    }

    /// Build a report from `(id, state)` pairs and a custom `RunStats`, used by
    /// the durable-report tests that vary spawn counts and initial states.
    fn report_with(tasks: &[(&str, &str)], stats: RunStats) -> RunSummaryReport {
        let mut md = String::from("# Rhei: Test Plan\n\n## Tasks\n\n");
        for (id, state) in tasks {
            md.push_str(&format!("### Task {id}: Task {id}\n**State:** {state}\n\n"));
        }
        let rhei = rhei_core::parse(&md).expect("plan parses");
        RunSummaryReport::build(&rhei, &rhei_validator::MachineSet::single(machine()), &SummarySink::new(), stats, "plan.rhei.md", &no_task_roots())
    }

    #[test]
    fn markdown_report_has_all_sections() {
        let r = report(&[("1", "completed"), ("2", "blocked")]);
        let md = r.render_markdown();
        assert!(md.starts_with("# Run Report: Test Plan"), "{md}");
        assert!(md.contains("Run: 2025-"), "header carries the ISO start: {md}");
        assert!(md.contains("| Final states | Count |"), "{md}");
        assert!(md.contains("| Activity | Count |"), "{md}");
        assert!(md.contains("## Attention"), "{md}");
        assert!(md.contains("## Transition Ledger"), "{md}");
        assert!(md.contains("## Task Final States"), "{md}");
    }

    #[test]
    fn run_id_is_stable_for_a_given_start() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(1_749_115_351_123_456);
        assert_eq!(short_run_id(t), short_run_id(t));
        assert_eq!(short_run_id(t).len(), 6);
    }

    #[test]
    fn no_work_run_that_advanced_reads_differently() {
        // Every task ended completed, nothing spawned, and a task moved off its
        // non-terminal start — the report must not look like fast agent work.
        // §FS-rhei-run-report.3.3
        let mut initial = HashMap::new();
        initial.insert("1".to_string(), "queued".to_string());
        let stats = RunStats {
            agents_spawned: 0,
            programs_spawned: 0,
            callback_only: 1,
            initial_states: initial,
            ..test_stats()
        };
        let r = report_with(&[("1", "completed")], stats);
        assert_eq!(r.result, "completed — no work spawned");
        let md = r.render_markdown();
        assert!(md.contains("No agent or program ran"), "{md}");
        // The advance with no invocation is a callback-only ledger row.
        assert!(md.contains("| 1 | queued | completed | callback-only |"), "{md}");
    }

    #[test]
    fn terminal_at_start_task_is_marked_calm() {
        let mut initial = HashMap::new();
        initial.insert("done".to_string(), "completed".to_string());
        let stats = RunStats { initial_states: initial, ..test_stats() };
        let r = report_with(&[("done", "completed")], stats);
        assert_eq!(r.terminal_at_start, 1);
        let md = r.render_markdown();
        assert!(md.contains("terminal at start"), "{md}");
        // It is a terminal-at-start ledger row, not an invocation.
        assert!(md.contains("| done | completed | - | terminal-at-start |"), "{md}");
    }

    #[test]
    fn write_to_runtime_emits_latest_and_history() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        let stats =
            RunStats { workspace_root: dir.path().to_path_buf(), ..test_stats() };
        let mut r = report_with(&[("1", "completed")], stats);
        r.write_to_runtime(&runtime).expect("write report");
        assert!(runtime.join("run-report.md").exists());
        assert_eq!(r.report_path.as_deref(), Some("runtime/run-report.md"));
        let history = std::fs::read_dir(runtime.join("run-reports"))
            .expect("history dir")
            .filter_map(Result::ok)
            .count();
        assert_eq!(history, 1, "one timestamped history entry written");
    }

    /// The result follows the reading the run took when its loop ended, not
    /// the process-wide token at report time: a signal that arrives after the
    /// run finished — while the TUI is parked on its finished screen — leaves
    /// the run its own result.
    // §FS-rhei-run.3.2 §FS-rhei-run-report.3.1
    #[test]
    fn a_signal_after_the_loop_finished_does_not_relabel_the_result() {
        let finished = report_with(&[("1", "completed")], test_stats());
        assert_eq!(finished.result, "completed");
        let cut_short =
            report_with(&[("1", "completed")], RunStats { interrupted: true, ..test_stats() });
        assert_eq!(cut_short.result, "interrupted — re-run to continue");
    }

    #[test]
    fn dry_run_result_reads_as_preview() {
        let stats = RunStats { dry_run: true, ..test_stats() };
        let r = report_with(&[("1", "completed")], stats);
        assert_eq!(r.result, "dry run — no changes applied");
        assert!(r.render_markdown().contains("Result: dry run — no changes applied"));
    }

    #[test]
    fn dashboard_pointer_gated_on_enabled_this_run() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::write(runtime.join("dashboard.html"), "<html>").unwrap();
        // A stale dashboard from an earlier run must not be linked when the
        // dashboard was off this run.
        assert_eq!(frozen_dashboard_relative_path(false, &runtime, dir.path()), None);
        assert_eq!(
            frozen_dashboard_relative_path(true, &runtime, dir.path()).as_deref(),
            Some("runtime/dashboard.html"),
        );
    }

    #[test]
    fn md_cell_escapes_pipes_and_newlines() {
        assert_eq!(md_cell("a|b"), "a\\|b");
        assert_eq!(md_cell("line1\nline2"), "line1 line2");
    }

    /// A `SummarySink` carrying one spawned transition `from`→`to`.
    fn summary_with_spawn(task: &str, from: &str, to: &str, agent: bool) -> SummarySink {
        use rhei_tui::EventSink;
        let s = SummarySink::new();
        let log = std::path::PathBuf::from("runtime/logs/x.log");
        s.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            agent: agent.then(|| "mock".to_string()),
            template_context: None,
            log_path: log.clone(),
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
        s.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            log_path: log,
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_200,
            reverted: None,
        });
        s
    }

    #[test]
    fn ledger_records_trailing_callback_advance_after_spawn() {
        // An agent ran build->review, then a callback carried review->completed
        // with no further spawn. The ledger must reach the final state.
        let summary = summary_with_spawn("1", "build", "review", true);
        let stats = RunStats { initial_states: HashMap::new(), ..test_stats() };
        let mut md = String::from("# Rhei: Test Plan\n\n## Tasks\n\n");
        md.push_str("### Task 1: Task 1\n**State:** completed\n\n");
        let rhei = rhei_core::parse(&md).expect("plan parses");
        let report = RunSummaryReport::build(&rhei, &rhei_validator::MachineSet::single(machine()), &summary, stats, "plan.rhei.md", &no_task_roots());
        let md = report.render_markdown();
        // The spawned agent row and the synthesized callback advance both appear.
        assert!(md.contains("| 1 | build | review | agent |"), "{md}");
        assert!(md.contains("| 1 | review | completed | callback-only |"), "{md}");
    }

    fn idle_stop(blocker: rhei_tui::IdleBlocker, next: Option<&str>) -> rhei_tui::RunStop {
        rhei_tui::RunStop {
            reason: rhei_tui::StopReason::Idle,
            idle_blocker: Some(blocker),
            next_attempt_at: next.map(str::to_string),
            exit_code: 3,
        }
    }

    fn gate_row() -> TaskRow {
        TaskRow {
            depth: 0,
            id: "1".to_string(),
            state: "review".to_string(),
            marker: Marker::Gate,
            detail: None,
        }
    }

    fn attention_row() -> AttentionRow {
        AttentionRow {
            id: "2".to_string(),
            state: "build".to_string(),
            reason: "stalled".to_string(),
            next: "inspect the logs".to_string(),
            is_gate: false,
            waits_on_person: false,
            provider_limited: false,
        }
    }

    /// The idle branch of the run's one-line outcome, and where it ranks.
    ///
    /// Each wait gets its own phrase, because "idle" alone tells an operator
    /// nothing about whether to come back or to go and do something. The rank
    /// is the half that matters more: one ticket whose blocker is actionable
    /// defeats idle however many tickets wait beside it, and an interrupt
    /// defeats both — so a real problem can never hide under a waiting result.
    /// §FS-rhei-run-report.3.1 §FS-rhei-run.3
    #[test]
    fn the_idle_result_phrase_names_its_wait_and_ranks_below_attention() {
        let rows = vec![gate_row()];
        let phrase = |stop: &rhei_tui::RunStop| {
            result_phrase(&[], &rows, true, false, false, Some(stop))
        };
        assert_eq!(
            phrase(&idle_stop(rhei_tui::IdleBlocker::Gate, None)),
            "idle \u{2014} waiting on a person"
        );
        assert_eq!(
            phrase(&idle_stop(rhei_tui::IdleBlocker::Poll, Some("2030-01-01T00:00:00Z"))),
            "idle \u{2014} retry at 2030-01-01T00:00:00Z"
        );
        assert_eq!(
            phrase(&idle_stop(rhei_tui::IdleBlocker::ProviderLimit, None)),
            "idle \u{2014} waiting on a provider limit"
        );
        assert_eq!(
            phrase(&idle_stop(rhei_tui::IdleBlocker::Mixed, None)),
            "idle \u{2014} mixed waits"
        );

        let idle = idle_stop(rhei_tui::IdleBlocker::Gate, None);
        assert_eq!(
            result_phrase(&[attention_row()], &rows, true, false, false, Some(&idle)),
            "stopped for human attention",
            "one actionable ticket defeats idle however many wait beside it"
        );
        assert_eq!(
            result_phrase(&[], &rows, true, false, true, Some(&idle)),
            "interrupted \u{2014} re-run to continue",
            "and an interrupt defeats both"
        );
        assert_eq!(
            result_phrase(&[], &rows, true, false, false, None),
            "finished",
            "an unselected run reaches none of this"
        );
    }

    // ------------------------------------------------------------------------
    // The subtree fold. A terminal parent speaks for its subtree on the
    // console tree, with the clause the prompt side already writes.

    // §FS-rhei-run-report.3.2 §FS-rhei-shape.3.2

    /// A machine with a terminal success, a cancellation, a gate, a second
    /// terminal of its own name, and one open work state — the five classes
    /// the fold's predicate and its breakdown have to tell apart.
    fn fold_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"name: fold
version: 1
states:
  work:
    initial: true
    description: open work
  human-gate:
    description: awaiting a human
    gating: true
  completed:
    description: finished
    final: true
  shipped:
    description: finished and released
    final: true
  cancelled:
    description: not done
    final: true
transitions:
  - from: work
    to: completed
  - from: work
    to: shipped
  - from: work
    to: cancelled
  - from: work
    to: human-gate
  - from: human-gate
    to: completed
"#,
        )
        .expect("valid state machine")
    }

    /// Build a report from a plan body, under [`fold_machine`] and a depth that
    /// permits grandchildren.
    fn fold_report(tasks: &str) -> RunSummaryReport {
        let rhei = rhei_core::parse(&format!(
            "# Rhei: Fold\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n{tasks}"
        ))
        .expect("plan parses");
        RunSummaryReport::build(
            &rhei,
            &rhei_validator::MachineSet::single(fold_machine()),
            &SummarySink::new(),
            test_stats(),
            "plan.rhei.md",
            &no_task_roots(),
        )
    }

    /// Every row id the console tree actually prints, in order.
    fn tree_ids(report: &RunSummaryReport) -> Vec<String> {
        let tty = report.render_tty(false);
        let ids: Vec<String> = report
            .rows
            .iter()
            .map(|row| row.id.clone())
            .filter(|id| {
                tty.lines().any(|line| {
                    line.split_whitespace().any(|token| token == id.as_str())
                })
            })
            .collect();
        ids
    }

    /// A terminal parent renders one row for its whole finished subtree, and
    /// the count is every descendant at any depth rather than its direct
    /// children. Four nested rows that say the same thing are four rows a
    /// person scrolls past to reach the one that does not.
    // §FS-rhei-run-report.3.2
    #[test]
    fn a_terminal_parent_speaks_for_its_finished_subtree() {
        let report = fold_report(
            "### Task 1: Harden the parser\n**State:** completed\n\n\
             #### Task 1.1: Review parser\n**State:** completed\n\n\
             ##### Task 1.1.1: Read the spec\n**State:** completed\n\n\
             #### Task 1.2: Fix findings\n**State:** completed\n\n\
             ### Task 2: Next\n**State:** work\n",
        );

        let tty = report.render_tty(false);
        assert!(
            tty.contains("— 3 subtasks: 3 completed"),
            "the parent carries the clause for every descendant at any depth; got:\n{tty}"
        );
        assert_eq!(
            tree_ids(&report),
            vec!["1", "2"],
            "the finished subtree renders no rows of its own; got:\n{tty}"
        );
    }

    /// `{breakdown}` buckets every descendant by its normalized state name, in
    /// the order the machine declares those states, so the numbers sum to
    /// `{n}` and a custom terminal appears under its own name. Two fixed
    /// buckets would have reported `shipped` as something it is not.
    // §FS-rhei-run-report.3.2 §FS-rhei-memory.3.2
    #[test]
    fn the_breakdown_names_every_bucket_in_the_machines_own_order() {
        let report = fold_report(
            "### Task 1: Release the parser\n**State:** completed\n\n\
             #### Task 1.1: First\n**State:** completed\n\n\
             #### Task 1.2: Second\n**State:** shipped\n\n\
             #### Task 1.3: Third\n**State:** cancelled\n\n\
             #### Task 1.4: Fourth\n**State:** completed\n",
        );

        let tty = report.render_tty(false);
        assert!(
            tty.contains("— 4 subtasks: 2 completed, 1 shipped, 1 cancelled"),
            "declaration order, every descendant bucketed, nothing omitted; got:\n{tty}"
        );
    }

    /// A bucket with no members is omitted, so an all-completed subtree reads
    /// the way it always did and no empty count is invented for a state the
    /// machine merely declares.
    // §FS-rhei-run-report.3.2
    #[test]
    fn an_empty_bucket_is_omitted_from_the_breakdown() {
        let report = fold_report(
            "### Task 1: Parent\n**State:** completed\n\n\
             #### Task 1.1: First\n**State:** completed\n\n\
             #### Task 1.2: Second\n**State:** completed\n",
        );

        let tty = report.render_tty(false);
        assert!(tty.contains("— 2 subtasks: 2 completed"), "got:\n{tty}");
        assert!(
            !tty.contains("0 cancelled") && !tty.contains("0 shipped"),
            "a bucket with no members is not rendered; got:\n{tty}"
        );
    }

    /// A descendant that needs attention or waits on a person keeps its own
    /// row, and so does every ancestor above it. The fold may make a run look
    /// quiet; it may never make a run look quiet that is not.
    // §FS-rhei-run-report.3.2
    #[test]
    fn a_descendant_needing_attention_prevents_the_fold() {
        for (label, state) in [("blocked", "work"), ("gated", "human-gate")] {
            let report = fold_report(&format!(
                "### Task 1: Parent\n**State:** completed\n\n\
                 #### Task 1.1: Finished\n**State:** completed\n\n\
                 #### Task 1.2: Not finished\n**State:** {state}\n\n\
                 ##### Task 1.2.1: Under it\n**State:** completed\n"
            ));

            let tty = report.render_tty(false);
            assert!(
                !tty.contains("subtasks:"),
                "{label}: a subtree with a {label} descendant does not fold; got:\n{tty}"
            );
            assert_eq!(
                tree_ids(&report),
                vec!["1", "1.1", "1.2", "1.2.1"],
                "{label}: the row and every ancestor stay expanded; got:\n{tty}"
            );
        }
    }

    /// A parent still open renders its finished children exactly as before, so
    /// a person watching a long run keeps seeing evidence of progress. The
    /// fold is about a subtree that is over, not about one in flight.
    // §FS-rhei-run-report.3.2
    #[test]
    fn an_open_parent_still_shows_the_children_that_finished() {
        let report = fold_report(
            "### Task 1: Parent\n**State:** work\n\n\
             #### Task 1.1: Done\n**State:** completed\n\n\
             #### Task 1.2: Also done\n**State:** completed\n",
        );

        let tty = report.render_tty(false);
        assert!(
            !tty.contains("subtasks:"),
            "an open parent does not speak for its subtree; got:\n{tty}"
        );
        assert_eq!(
            tree_ids(&report),
            vec!["1", "1.1", "1.2"],
            "its finished children keep their rows; got:\n{tty}"
        );
    }

    /// The two collapse mechanisms compose: the subtree fold runs first, the
    /// forty-row budget runs on whatever is left, and neither hides the one
    /// row a person has to act on.
    // §FS-rhei-run-report.3.2
    #[test]
    fn the_subtree_fold_and_the_forty_row_budget_compose() {
        let mut tasks = String::new();
        // The parents come first, so their folded rows sit inside the first
        // forty, where the budget leaves them alone.
        for parent in 1..=6 {
            tasks.push_str(&format!("### Task p{parent}: Parent {parent}\n**State:** completed\n\n"));
            for child in 1..=4 {
                tasks.push_str(&format!(
                    "#### Task p{parent}.{child}: Child {parent}.{child}\n**State:** completed\n\n"
                ));
            }
        }
        // Enough flat finished roots that the budget still has work to do once
        // every subtree has folded.
        for root in 1..=45 {
            tasks.push_str(&format!("### Task f{root}: Flat {root}\n**State:** completed\n\n"));
        }
        tasks.push_str("### Task last: Needs a person\n**State:** human-gate\n");

        let report = fold_report(&tasks);
        assert_eq!(report.rows.len(), 76, "45 flat, 6 parents with 4 children each, one gate");

        let tty = report.render_tty(false);
        assert_eq!(
            tty.matches("\u{2014} 4 subtasks: 4 completed").count(),
            6,
            "the fold runs first, once per finished parent; got:\n{tty}"
        );
        assert!(
            tty.contains("completed tasks collapsed"),
            "51 rows survive the fold, so the forty-row budget still fires; got:\n{tty}"
        );
        assert!(
            tree_ids(&report).contains(&"last".to_string()),
            "and the gate is shown whatever either mechanism would prefer; got:\n{tty}"
        );
    }

    /// The budget bounds a tree of folded parents the way it bounds a flat one:
    /// past the fortieth row a folded parent collapses with its subtree, and
    /// the collapsed count carries the parent and every task it spoke for.
    /// One row per finished parent, with no bound, is the tree the fold was
    /// meant to shorten.
    // §FS-rhei-run-report.3.2
    #[test]
    fn the_forty_row_budget_collapses_a_folded_parent_with_its_subtree() {
        let mut tasks = String::new();
        for parent in 1..=45 {
            tasks.push_str(&format!(
                "### Task p{parent}: Parent {parent}\n**State:** completed\n\n\
                 #### Task p{parent}.1: Child {parent}\n**State:** completed\n\n"
            ));
        }

        let report = fold_report(&tasks);
        let tty = report.render_tty(false);
        assert_eq!(tree_ids(&report).len(), 40, "forty rows survive the budget; got:\n{tty}");
        assert!(
            tty.contains("\u{2026} 10 completed tasks collapsed"),
            "five folded parents collapse, each with its one child; got:\n{tty}"
        );
    }

    /// A folded parent whose breakdown names a cancelled descendant is the only
    /// place that descendant appears on the tree, and the collapsed line counts
    /// completed tasks, so the budget keeps that parent shown past forty rows.
    // §FS-rhei-run-report.3.2
    #[test]
    fn the_budget_keeps_a_folded_parent_that_speaks_for_a_cancelled_task() {
        let mut tasks = String::new();
        for parent in 1..=45 {
            tasks.push_str(&format!(
                "### Task p{parent}: Parent {parent}\n**State:** completed\n\n\
                 #### Task p{parent}.1: Child {parent}\n**State:** completed\n\n"
            ));
        }
        tasks.push_str(
            "### Task q: Parent with a cancelled child\n**State:** completed\n\n\
             #### Task q.1: Dropped\n**State:** cancelled\n",
        );

        let report = fold_report(&tasks);
        let tty = report.render_tty(false);
        assert!(
            tty.contains("\u{2014} 1 subtasks: 1 cancelled"),
            "the parent still folds its subtree; got:\n{tty}"
        );
        assert!(
            tree_ids(&report).contains(&"q".to_string()),
            "and stays shown past the fortieth row; got:\n{tty}"
        );
        assert!(
            tty.contains("\u{2026} 10 completed tasks collapsed"),
            "while the calm parents before it still collapse; got:\n{tty}"
        );
    }

    /// `## Task Final States` is the un-collapsed source of truth, so the
    /// console line that says *N subtasks* is honest only because every folded
    /// task still has a line of its own two sections below it.
    // §FS-rhei-run-report.3.2
    #[test]
    fn the_task_final_states_section_stays_un_collapsed() {
        let report = fold_report(
            "### Task 1: Parent\n**State:** completed\n\n\
             #### Task 1.1: First\n**State:** completed\n\n\
             #### Task 1.2: Second\n**State:** completed\n",
        );

        assert!(report.render_tty(false).contains("— 2 subtasks: 2 completed"));
        let markdown = report.render_markdown();
        let section = markdown
            .split("## Task Final States")
            .nth(1)
            .expect("the report carries the section");
        for id in ["`1`", "`1.1`", "`1.2`"] {
            assert!(section.contains(id), "{id} keeps its own line; got:\n{section}");
        }
    }
