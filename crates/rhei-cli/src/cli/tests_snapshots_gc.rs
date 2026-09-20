    /// An agent that succeeds having only recorded that it ran, as one file of
    /// its own in a directory that survives between invocations: counting those
    /// files is how the reschedule tests see how many times it was spawned.
    ///
    /// A file each, not a shared counter and not a shared append: a fan-out
    /// state spawns its invocations concurrently, read-modify-write loses one of
    /// them, and an append on Windows is a seek and a write rather than one
    /// atomic act. The path is JSON-encoded, which is also a Python string
    /// literal, so a backslash in it cannot escape the fixture.
    /// How many invocations the counting agent recorded.
    fn spawn_count(spawns_dir: &Path) -> usize {
        fs::read_dir(spawns_dir).map(|entries| entries.count()).unwrap_or(0)
    }

    fn write_counting_success_agent(dir: &Path, spawns_dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "counting-agent",
            &format!(
                r#"import os, pathlib

spawns = pathlib.Path({path})
spawns.mkdir(parents=True, exist_ok=True)
(spawns / '{{}}.txt'.format(os.getpid())).write_text('spawned')
"#,
                path = serde_json::to_string(spawns_dir.to_string_lossy().as_ref()).expect("json")
            ),
        )
    }

    fn missing_outputs_reschedule_workspace(all_targets: bool) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        let settings_dir = dir.path().join(".agent-grounds/rhei");
        fs::create_dir_all(&settings_dir).expect("mkdir");
        let spawns_dir = dir.path().join("spawns");
        let command = write_counting_success_agent(dir.path(), &spawns_dir);
        fs::write(
            &plan,
            r#"# Rhei: Missing Outputs

## Tasks

### Task 1: Work
**State:** pending
"#,
        )
        .expect("plan");
        let state_invocation = if all_targets {
            "    all_targets:\n      - fake:openai:model-a\n      - fake:openai:model-b\n"
        } else {
            "    agent: fake\n"
        };
        fs::write(
            &states,
            format!(
                "name: missing-outputs\nversion: 1\nstates:\n  pending:\n    description: pending\n{}    agent_timeout: 5s\n    outputs:\n      - name: required-report\n        path: runtime/required-report.md\n  done:\n    description: done\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
                state_invocation
            ),
        )
        .expect("states");
        fs::write(
            settings_dir.join("settings.json"),
            format!(
                r#"{{
                  "agents": {{
                    "fake": {{
                      "command": {},
                      "prompt_flag": "--prompt"
                    }}
                  }}
                }}"#,
                serde_json::to_string(&command).expect("json")
            ),
        )
        .expect("settings");

        let mut opts = default_run_options();
        opts.standalone.no_tui = true;
        let err = run_command(&plan, Some(&states), opts)
            .expect_err("missing outputs leave non-terminal work");
        assert!(
            err.to_string().contains("non-terminal tasks remaining"),
            "unexpected error: {err}"
        );

        (dir, spawns_dir)
    }

    #[test]
    fn missing_outputs_reschedule_single_invocation_spawns_once_and_keeps_state() {
        let (dir, spawns_dir) = missing_outputs_reschedule_workspace(false);
        assert_eq!(spawn_count(&spawns_dir), 1, "one invocation, one spawn");
        let plan = fs::read_to_string(dir.path().join("plan.rhei.md")).expect("plan");
        assert!(plan.contains("**State:** pending"));
    }

    #[test]
    fn missing_outputs_reschedule_fanout_spawns_each_target_once_and_keeps_state() {
        let (dir, spawns_dir) = missing_outputs_reschedule_workspace(true);
        assert_eq!(spawn_count(&spawns_dir), 2, "one spawn per declared target");
        let plan = fs::read_to_string(dir.path().join("plan.rhei.md")).expect("plan");
        assert!(plan.contains("**State:** pending"));
    }

    #[test]
    fn missing_outputs_reschedule_warning_names_missing_artifacts() {
        let recorder = Arc::new(RecordingSink::default());
        let sink: Arc<dyn rhei_tui::EventSink> = recorder.clone();
        emit_missing_required_outputs_warning(
            "agent",
            "1",
            "pending",
            0,
            &["required-report".to_string()],
            RetryOutlook::AttemptsLeft,
            &sink,
        );
        let events = recorder.events.lock().expect("events");
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::Message { text, .. }
                if text.contains("agent exited 0 but required outputs are missing")
                    && text.contains("required-report")
        )));
    }

    /// The same stall reached from a declared route, where the worker's exit was
    /// not `0`: the line names the code the program actually exited with, or it
    /// sends the operator looking for an exit that never happened.
    // §FS-rhei-programs.3.2 §FS-rhei-agents.3.2.1
    #[test]
    fn missing_outputs_warning_names_the_exit_code_the_worker_used() {
        let recorder = Arc::new(RecordingSink::default());
        let sink: Arc<dyn rhei_tui::EventSink> = recorder.clone();
        emit_missing_required_outputs_warning(
            "program",
            "1",
            "build",
            3,
            &["result (runtime/results/plan.1.md)".to_string()],
            RetryOutlook::AttemptsLeft,
            &sink,
        );
        let events = recorder.events.lock().expect("events");
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::Message { text, .. }
                if text.contains("program exited 3 but required outputs are missing")
        )));
    }

    // §FS-rhei-agents.3.2.1: entries carry the resolved path, and a path that
    // still holds a `{...}` template is called out as such — that is the signal
    // the path referenced a variable outside the namespace.
    #[test]
    fn missing_required_output_entry_names_resolved_path() {
        assert_eq!(
            format_missing_required_output("report", "runtime/report/auth.3.md"),
            "report (runtime/report/auth.3.md)"
        );
    }

    #[test]
    fn missing_required_output_entry_flags_unresolved_template() {
        assert_eq!(
            format_missing_required_output("report", "runtime/report/{task.id}.md"),
            "report (runtime/report/{task.id}.md, unresolved template)"
        );
    }

    #[test]
    fn agent_spawn_outcome_carries_resolved_timeout() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_sleeping_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile { command, ..CustomAgentProfile::default() },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(1),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling::default();

        let status = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "prompt",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-timeout",
            "pending",
            1,
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder,
            None,
            &spawn_plan_for_test(&log_path),
            None,
        None,
)
        .expect("timeout returns process status");

        assert!(status.timed_out);
        assert_eq!(status.timeout_secs, Some(1));
    }

    #[test]
    fn parses_snapshot_list_command() {
        let cli = Cli::try_parse_from([
            "rhei",
            "snapshot",
            "list",
            "--plan",
            "plan",
            "--task",
            "1",
            "--name",
            "_state",
            "--state",
            "pending",
            "--produced-by",
            "all",
            "--format",
            "json",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Snapshot {
                command: SnapshotCommand::List { plan, task, name, state, produced_by, format, .. },
                ..
            } => {
                assert_eq!(plan, PathBuf::from("plan"));
                assert_eq!(task.as_deref(), Some("1"));
                assert_eq!(name.as_deref(), Some("_state"));
                assert_eq!(state.as_deref(), Some("pending"));
                assert_eq!(produced_by, SnapshotProducedByFilter::All);
                assert_eq!(format, SnapshotListFormat::Json);
            }
            other => panic!("expected snapshot list command, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_ref_parser_prefers_named_snapshot_over_auto_shorthand() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "pending",
            "review",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "_state",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        refresh_current_links(
            &ctx.cache_root,
            [SnapshotIdentity {
                task_id: "plan.1".to_string(),
                snapshot_name: "pending".to_string(),
                emitting_state: "review".to_string(),
                visit: 1,
                target_slug: "claude-code-anthropic-model".to_string(),
            }]
            .into_iter()
            .collect(),
        )
        .expect("current");

        let resolved =
            resolve_snapshot_ref(&ctx, "plan.1:pending", None, None).expect("resolve ref");
        assert_eq!(resolved.snapshot_name, "pending");
        assert_eq!(resolved.emitting_state, "review");
    }

    #[test]
    fn snapshot_ref_parser_reports_ambiguous_broad_match() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "review",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        refresh_current_links(
            &ctx.cache_root,
            [
                SnapshotIdentity {
                    task_id: "plan.1".to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "pending".to_string(),
                    visit: 1,
                    target_slug: "claude-code-anthropic-model".to_string(),
                },
                SnapshotIdentity {
                    task_id: "plan.1".to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "review".to_string(),
                    visit: 1,
                    target_slug: "claude-code-anthropic-model".to_string(),
                },
            ]
            .into_iter()
            .collect(),
        )
        .expect("current");

        let err = resolve_snapshot_ref(&ctx, "plan.1:impl", None, None).expect_err("ambiguous ref");
        let msg = err.to_string();
        assert!(msg.contains("ambiguous"));
        assert!(msg.contains("plan.1:impl:pending@1:claude-code-anthropic-model/g1"));
        assert!(msg.contains("plan.1:impl:review@1:claude-code-anthropic-model/g1"));
    }

    #[test]
    fn snapshot_ref_parser_requires_current_when_generation_omitted() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "orchestrator",
        );

        let err = resolve_snapshot_ref(&ctx, "plan.1:impl:pending", None, None)
            .expect_err("missing current must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("none is marked current"));
        assert!(msg.contains("retry with /g<N>"));

        let resolved = resolve_snapshot_ref(&ctx, "plan.1:impl:pending/g1", None, None)
            .expect("explicit gen");
        assert_eq!(resolved.generation, 1);
    }

    #[test]
    fn snapshot_gc_keep_generations_ignores_operator_by_default() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "operator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            3,
            "orchestrator",
        );

        snapshot_gc_command(&ctx, None, Some("impl"), None, Some(1), false, false, false, false)
            .expect("gc succeeds");

        let identity = ctx
            .cache_root
            .join("plan.1")
            .join("impl")
            .join("pending")
            .join("1")
            .join("claude-code-anthropic-model");
        assert!(!identity.join("g1").exists(), "old orchestrator generation deleted");
        assert!(identity.join("g2").exists(), "operator generation ignored by default");
        assert!(identity.join("g3").exists(), "newest orchestrator generation retained");
    }

    #[test]
    fn snapshot_orphan_detection_recurses_into_child_tasks() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        write_snapshot_workspace_task(
            dir.path(),
            "### Task 1: Implement\n**State:** pending\n\nDo work.\n\n#### Task 1.1: Child\n**State:** pending\n\nDo child work.\n",
        );
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        let record =
            resolve_snapshot_ref(&ctx, "plan.1.1:impl/g1", None, None).expect("resolve ref");

        assert!(!is_snapshot_orphaned(&record, &ctx), "child task snapshot must not be orphaned");
    }

    #[test]
    fn snapshot_gc_keep_generations_counts_newer_records_before_older_than() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation_with_created_at(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
            "1970-01-01T00:00:00Z",
        );
        write_snapshot_generation_with_created_at(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "orchestrator",
            "1970-01-01T00:00:00Z",
        );
        write_snapshot_generation_with_created_at(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            3,
            "orchestrator",
            "2999-01-01T00:00:00Z",
        );

        snapshot_gc_command(
            &ctx,
            None,
            Some("impl"),
            Some("1s"),
            Some(2),
            false,
            false,
            false,
            false,
        )
        .expect("gc succeeds");

        let identity = ctx
            .cache_root
            .join("plan.1")
            .join("impl")
            .join("pending")
            .join("1")
            .join("claude-code-anthropic-model");
        assert!(!identity.join("g1").exists(), "old generation outside retention deleted");
        assert!(identity.join("g2").exists(), "old generation inside retention retained");
        assert!(identity.join("g3").exists(), "new generation counted for retention");
    }

    #[test]
    fn snapshot_active_inherit_protection_recurses_into_child_tasks() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        write_snapshot_workspace_task(
            dir.path(),
            "### Task 1: Implement\n**State:** completed\n\nDo work.\n\n#### Task 1.1: Child\n**State:** review\n\nDo child work.\n",
        );
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: impl
  review:
    description: review
    target: claude-code:anthropic:model
    snapshot:
      inherit:
        name: impl
        from: ancestor
        select:
          state: pending
          generation: latest
  completed:
    description: completed
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#,
        )
        .expect("write states");
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "plan.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "orchestrator",
        );
        let latest =
            resolve_snapshot_ref(&ctx, "plan.1:impl/g2", None, None).expect("resolve ref");

        assert!(
            snapshot_generation_protected_by_active_inherit(&latest, &ctx),
            "child task active inherit must protect selected generation"
        );
    }

    #[test]
    fn snapshot_active_inherit_protection_respects_source_axis() {
        let _home = TempHome::new();

        let self_dir = snapshot_workspace();
        write_snapshot_workspace_task(
            self_dir.path(),
            "### Task 1: Parent\n**State:** completed\n\nParent.\n\n#### Task 1.1: Child\n**State:** review\n\nChild.\n",
        );
        fs::write(
            self_dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: impl
  review:
    description: review
    target: claude-code:anthropic:model
    snapshot:
      inherit:
        name: impl
        select:
          state: pending
          generation: current
  completed:
    description: completed
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#,
        )
        .expect("write states");
        let self_ctx = load_snapshot_context(self_dir.path(), None).expect("snapshot context");
        for task_id in ["plan.1", "plan.1.1"] {
            write_snapshot_generation(
                &self_ctx.cache_root,
                task_id,
                "impl",
                "pending",
                1,
                "claude-code-anthropic-model",
                1,
                "orchestrator",
            );
        }
        refresh_current_links(
            &self_ctx.cache_root,
            ["plan.1", "plan.1.1"]
                .into_iter()
                .map(|task_id| SnapshotIdentity {
                    task_id: task_id.to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "pending".to_string(),
                    visit: 1,
                    target_slug: "claude-code-anthropic-model".to_string(),
                })
                .collect(),
        )
        .expect("current");
        let parent =
            resolve_snapshot_ref(&self_ctx, "plan.1:impl/g1", None, None).expect("parent snapshot");
        let child = resolve_snapshot_ref(&self_ctx, "plan.1.1:impl/g1", None, None)
            .expect("child snapshot");
        assert!(
            !snapshot_generation_protected_by_active_inherit(&parent, &self_ctx),
            "from: self must not protect ancestor snapshots"
        );
        assert!(
            snapshot_generation_protected_by_active_inherit(&child, &self_ctx),
            "from: self protects the active task's own selected snapshot"
        );

        let ancestor_dir = snapshot_workspace();
        write_snapshot_workspace_task(
            ancestor_dir.path(),
            "### Task 1: Parent\n**State:** completed\n\nParent.\n\n#### Task 1.1: Child\n**State:** review\n\nChild.\n",
        );
        fs::write(
            ancestor_dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: impl
  review:
    description: review
    target: claude-code:anthropic:model
    snapshot:
      inherit:
        name: impl
        from: ancestor
        select:
          state: pending
          generation: current
  completed:
    description: completed
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#,
        )
        .expect("write states");
        let ancestor_ctx =
            load_snapshot_context(ancestor_dir.path(), None).expect("snapshot context");
        for task_id in ["plan.1", "plan.1.1"] {
            write_snapshot_generation(
                &ancestor_ctx.cache_root,
                task_id,
                "impl",
                "pending",
                1,
                "claude-code-anthropic-model",
                1,
                "orchestrator",
            );
        }
        refresh_current_links(
            &ancestor_ctx.cache_root,
            ["plan.1", "plan.1.1"]
                .into_iter()
                .map(|task_id| SnapshotIdentity {
                    task_id: task_id.to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "pending".to_string(),
                    visit: 1,
                    target_slug: "claude-code-anthropic-model".to_string(),
                })
                .collect(),
        )
        .expect("current");
        let ancestor_parent = resolve_snapshot_ref(&ancestor_ctx, "plan.1:impl/g1", None, None)
            .expect("ancestor parent snapshot");
        let ancestor_child = resolve_snapshot_ref(&ancestor_ctx, "plan.1.1:impl/g1", None, None)
            .expect("ancestor child snapshot");
        assert!(
            snapshot_generation_protected_by_active_inherit(&ancestor_parent, &ancestor_ctx),
            "from: ancestor protects ancestor snapshots"
        );
        assert!(
            !snapshot_generation_protected_by_active_inherit(&ancestor_child, &ancestor_ctx),
            "from: ancestor must not protect the active task's own snapshot"
        );
    }

    #[test]
    fn snapshot_emit_writes_auto_and_named_generations() {
        let dir = snapshot_workspace();
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: pi:openai:model
    snapshot:
      emit:
        name: impl
        on: always
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("write states");
        let loaded = load_plan(dir.path()).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve")
                .remove(0);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "transcript\n").expect("log");
        let snapshot_preload =
            snapshot_preload_with_native_session(dir.path(), "native-session", b"transcript\n");

        emit_snapshots_after_agent_exit(
            dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("done"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Failure,
            &snapshot_preload,
        )
        .expect("emit snapshots");

        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.iter().any(|record| record.snapshot_name == "_state"));
        assert!(records.iter().any(|record| record.snapshot_name == "impl"));
        assert!(records.iter().all(|record| record.generation == 1 && record.is_current));
        assert!(records.iter().all(|record| {
            record.manifest.get("session_id").and_then(serde_json::Value::as_str)
                == Some("native-session")
        }));
        assert!(records.iter().all(|record| {
            record.manifest.get("transcript_path").and_then(serde_json::Value::as_str)
                == Some("transcript.jsonl")
        }));
    }
