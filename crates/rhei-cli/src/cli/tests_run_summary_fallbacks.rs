// What counts as a Codex configuration fallback, and how each surface spells
// one. Included into `mod run_summary_tests` beside the report's own tests, so
// the indentation is the module's and its helpers are shared.
//
// The end-to-end proof is `tests/e2e/codex_config_fallback_tests.rs`; these are
// the cases a run cannot show cheaply — a key no fixture uses, a repeat, a
// message whose `|` would split its table cell.

// §AR-source-file-size.3 §FS-rhei-configuration-fallbacks

    const FALLBACK: &str = "Configured value for `permission_profile` is disallowed by requirements; falling back to required value ReadOnly.";

    fn stdout(lines: &[String]) -> Vec<(rhei_tui::AgentStream, String)> {
        lines.iter().map(|line| (rhei_tui::AgentStream::Stdout, line.clone())).collect()
    }

    fn item_error(message: &str) -> String {
        let item = serde_json::json!({"id": "i", "type": "error", "message": message});
        serde_json::json!({"type": "item.completed", "item": item}).to_string()
    }

    fn top_error(message: &str) -> String {
        serde_json::json!({"type": "error", "message": message}).to_string()
    }

    /// Both event shapes count, for any backtick-quoted key, and the message is
    /// kept whole and trimmed. §FS-rhei-configuration-fallbacks
    #[test]
    fn top_level_and_item_errors_carry_a_fallback_for_any_field() {
        let web = "Configured value for `web_search_mode` is disallowed by requirements; falling back to Disabled";
        let lines = stdout(&[item_error(FALLBACK), top_error(&format!("  {web}\n"))]);
        assert_eq!(
            codex_configuration_fallbacks("codex", &lines),
            vec![FALLBACK.to_string(), web.to_string()]
        );
    }

    /// Distinct messages in the order first seen; a repeat is kept once.
    /// §FS-rhei-configuration-fallbacks
    #[test]
    fn fallbacks_are_distinct_in_the_order_first_seen() {
        let approval = "Configured value for `approval_policy` is disallowed by requirements; falling back to OnRequest.";
        let lines = stdout(&[
            item_error(approval),
            top_error(FALLBACK),
            item_error(approval),
            top_error(FALLBACK),
        ]);
        assert_eq!(
            codex_configuration_fallbacks("codex", &lines),
            vec![approval.to_string(), FALLBACK.to_string()]
        );
    }

    /// Nothing else is one: an assistant message quoting the text, another
    /// error, a line that is not one JSON object, an empty key, stderr, and
    /// every other family. §FS-rhei-configuration-fallbacks
    #[test]
    fn output_that_only_quotes_a_fallback_carries_none() {
        let event = |kind: &str, item: serde_json::Value| {
            serde_json::json!({"type": kind, "item": item}).to_string()
        };
        let quoted = event(
            "item.completed",
            serde_json::json!({"type": "agent_message", "text": FALLBACK, "message": FALLBACK}),
        );
        let reasoning =
            event("item.completed", serde_json::json!({"type": "reasoning", "message": FALLBACK}));
        let started =
            event("item.started", serde_json::json!({"type": "error", "message": FALLBACK}));
        let unrelated = top_error("stream disconnected before completion");
        let empty_key =
            top_error("Configured value for `` is disallowed by requirements; falling back");
        let mid_sentence = top_error(&format!("note: {FALLBACK}"));
        let not_text =
            serde_json::json!({"type": "error", "message": ["Configured value for `x`"]})
                .to_string();
        let truncated = item_error(FALLBACK)[..40].to_string();
        let two_objects = format!("{}{}", top_error(FALLBACK), top_error(FALLBACK));
        for line in [
            quoted,
            reasoning,
            started,
            unrelated,
            empty_key,
            mid_sentence,
            not_text,
            truncated,
            two_objects,
            FALLBACK.to_string(),
        ] {
            assert_eq!(
                codex_configuration_fallbacks("codex", &stdout(&[line.clone()])),
                Vec::<String>::new(),
                "{line}"
            );
        }
        let stderr = vec![(rhei_tui::AgentStream::Stderr, top_error(FALLBACK))];
        assert!(codex_configuration_fallbacks("codex", &stderr).is_empty());
        for family in ["claude-code", "gemini", "cursor", "kilocode", "pi"] {
            assert!(
                codex_configuration_fallbacks(family, &stdout(&[top_error(FALLBACK)])).is_empty(),
                "{family}"
            );
        }
    }

    /// One summary of a single halted attempt: a worker that exited 0 owing
    /// `entries` and reported `fallbacks`, in an agent state, so nothing
    /// outranks the missing outputs as the reason it halted.
    fn halted_with(fallbacks: &[&str]) -> RunSummaryReport {
        use rhei_tui::EventSink;
        let sink = SummarySink::new();
        let log = std::path::PathBuf::from("runtime/logs/x.log");
        sink.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: "1".to_string(),
            from: "working".to_string(),
            to: "working".to_string(),
            agent: Some("cdx".to_string()),
            template_context: None,
            log_path: log.clone(),
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
        sink.emit(rhei_tui::RunEvent::TaskOutputsMissing {
            task: "1".to_string(),
            state: "working".to_string(),
            entries: vec!["finding (runtime/finding.md)".to_string()],
        });
        sink.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: "1".to_string(),
            from: "working".to_string(),
            to: "working".to_string(),
            log_path: log,
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_000,
            configuration_fallbacks: fallbacks.iter().map(|m| (*m).to_string()).collect(),
            reverted: None,
        });
        let md = "# Rhei: Test Plan\n\n## Tasks\n\n### Task 1: Task 1\n**State:** working\n";
        let rhei = rhei_core::parse(md).expect("plan parses");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            "name: fallbacks\nversion: 1\nstates:\n  working:\n    initial: true\n    \
             agent: cdx\n    agent_timeout: 60s\n  done:\n    final: true\ntransitions:\n  \
             - { from: working, to: done }\n",
        )
        .expect("valid state machine");
        let machines = rhei_validator::MachineSet::single(machine);
        RunSummaryReport::build(&rhei, &machines, &sink, test_stats(), "plan", &no_task_roots())
    }

    /// The halt row names each fallback after the missing outputs, one clause
    /// per message in order, and the cell escapes a `|` and folds a newline;
    /// Invocations carries the same clauses. §FS-rhei-run-report.3.1 §FS-rhei-run-report.2
    #[test]
    fn a_halt_names_its_fallbacks_in_escaped_cells() {
        let piped = "Configured value for `sandbox_mode` is disallowed by requirements; falling back to a|b\nnext";
        let md = halted_with(&[FALLBACK, piped]).render_markdown();
        let attention = md.split("## Attention").nth(1).unwrap_or_default();
        let row = attention.lines().find(|line| line.starts_with("| 1 |")).unwrap_or_default();
        let escaped = "falling back to a\\|b next";
        let expected = format!(
            "worker exited 0 without finding (runtime/finding.md); Codex configuration fallback: \
             {FALLBACK}; Codex configuration fallback: Configured value for `sandbox_mode` is \
             disallowed by requirements; {escaped}"
        );
        assert!(row.contains(&expected), "{row}\n{md}");
        let invocations = md.split("## Invocations").nth(1).unwrap_or_default();
        assert!(
            invocations.contains(&format!("exit 0; Codex configuration fallback: {FALLBACK}; ")),
            "{invocations}"
        );
        assert!(invocations.contains(escaped), "{invocations}");
    }

    /// Without a fallback the row and the exit cell read as before.
    /// §FS-rhei-run-report.3.1
    #[test]
    fn a_halt_without_a_fallback_reads_as_before() {
        let md = halted_with(&[]).render_markdown();
        assert!(md.contains("worker exited 0 without finding (runtime/finding.md) |"), "{md}");
        assert!(!md.contains("Codex configuration fallback"), "{md}");
    }

    /// The console line, verbatim. §FS-rhei-run-report.3.4
    #[test]
    fn the_console_warning_names_the_task_state_and_message() {
        assert_eq!(
            configuration_fallback_warning("plan.1", "review", FALLBACK),
            format!("  warning: Codex configuration fallback for task plan.1 in state 'review': {FALLBACK}")
        );
        assert_eq!(configuration_fallback_clauses(&[]), "");
    }
