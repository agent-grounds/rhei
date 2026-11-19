// The classifier's shared diagnostic and its interruption/claim precedence.
// No agent is launched: the ordinary spawn recorder establishes exhaustion.
// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1.1

    fn with_spent_visit(check: impl FnOnce(&rhei_core::ast::Rhei, &rhei_validator::MachineSet, &SpawnSiteRoots<'_>)) {
        let rhei = rhei_core::parse("# Rhei: Spent\n\n## Tasks\n\n### Task 1: Deliver\n**State:** win\n").unwrap();
        let machine = rhei_validator::StateMachine::from_yaml_str(r#"name: spent-summary
version: 1
states:
  win:
    initial: true
    target: codex:openai:gpt-5.6-sol
    attempts: 2
    outputs:
      - { name: delivery, path: runtime/delivery.md }
  done: { final: true }
transitions:
  - { from: win, to: done }
"#).unwrap();
        let machines = rhei_validator::MachineSet::single(machine.clone());
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let runtime = root.join("runtime");
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        let spawns = SpawnSiteRoots { settings: Some(&settings), ..SpawnSiteRoots::plan_only(root, &runtime) };
        let agent = resolve_target_agent("codex:openai:gpt-5.6-sol", None, &settings).unwrap();
        let identity = resolved_agent_log_identity(&agent);
        for _ in 0..2 {
            let (plan, _) = plan_named_spawn(&machine, root, &runtime, "1", "win", 1, identity.as_deref(), &spawns.machines.reset).unwrap();
            plan.record_spawn(SpawnEnding {
                task_id: "1", state_name: "win", kind: "agent", worker: "codex",
                started: "2026-11-19T10:00:00Z", ended: "2026-11-19T10:00:01Z",
                duration: "1s", code: Some(1), ending: "exited",
            });
        }
        assert!(spawns.attempt_budget_spent(&rhei, &machine, &rhei.tasks[0], "win"), "fixture must prove exhaustion before classifying it");
        check(&rhei, &machines, &spawns);
    }

    #[test]
    #[ignore = "red until #749: shared halt explanation must name spent visit and retry; implement removes this"]
    fn issue749_shared_halt_explanation_names_spent_visit() {
        with_spent_visit(|rhei, machines, spawns| {
            let (lines, actionable) = halted_task_report(rhei, machines, &None, Path::new("plan.rhei.md"), spawns);
            assert!(actionable);
            let explanation = lines.join("\n");
            for expected in ["2 attempts spent on this visit", "delivery", "rhei retry 1"] {
                assert!(explanation.contains(expected), "shared halt explanation lacks {expected:?}: {explanation}");
            }
            let report = RunSummaryReport::build(rhei, machines, &SummarySink::new(), test_stats(), "plan.rhei.md", spawns);
            let console = report.render_tty(false);
            assert!(console.contains("2 attempts spent on this visit") && console.contains("rhei retry 1"), "console uses the same cause: {console}");
        });
    }

    #[test]
    fn issue749_spent_visit_preserves_interruption_and_claim_precedence() {
        with_spent_visit(|rhei, machines, spawns| {
            let classify = |plan: &rhei_core::ast::Rhei, interrupted| {
                classify_halted_tasks(plan, machines, &None, &|_| true, &|_, _| None, &|_| interrupted, "plan.rhei.md", spawns).remove(0).1
            };
            assert!(matches!(classify(rhei, true), HaltCause::Interrupted));
            let mut claimed = rhei.clone();
            claimed.tasks[0].assignee = Some("operator".to_string());
            for interrupted in [false, true] {
                assert!(matches!(classify(&claimed, interrupted), HaltCause::Claimed { .. }));
            }
        });
    }
