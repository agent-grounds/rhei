    #[test]
    fn appends_mcp_config_flag_with_temp_file() {
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "linear".to_string(),
                optional: false,
                definition: Some(McpServerProfile {
                    command: Some(vec!["mcp-linear".to_string()]),
                    ..Default::default()
                }),
            }],
            skills: Vec::new(),
        };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do",
            Path::new("/tmp"),
            None,
            None,
            "task-7",
            "pending",
            1,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let idx = args.iter().position(|a| a == "--mcp-config").expect("--mcp-config emitted");
        let path = PathBuf::from(&args[idx + 1]);
        assert!(path.exists(), "mcp config file '{}' written", path.display());
        let body = fs::read_to_string(&path).expect("read config");
        assert!(body.contains("\"mcpServers\""));
        assert!(body.contains("\"linear\""));
    }

    #[test]
    fn appends_autonomous_args_after_mode_flags() {
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: Some("yolo".to_string()),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: vec!["--auto-flag".to_string(), "value".to_string()],
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do",
            Path::new("/tmp"),
            None,
            None,
            "t",
            "pending",
            1,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let mode_idx = args.iter().position(|a| a == "bypassPermissions").expect("mode flag");
        let auto_idx = args.iter().position(|a| a == "--auto-flag").expect("autonomous arg");
        assert!(auto_idx > mode_idx, "autonomous_args must follow mode flags: {args:?}");
    }

    #[test]
    fn settings_parses_nested_defaults_and_snapshots_block() {
        let json = r#"{
          "defaults": {
            "model": "impl-fast",
            "agent": "claude-code",
            "agent_mode": "yolo",
            "agent_timeout": "30m",
            "program_timeout": "10m"
          },
          "snapshots": {
            "cache_dir": ".rhei/cache/snapshots"
          }
        }"#;
        let parsed: RheiSettings = serde_json::from_str(json).expect("parse settings");
        assert_eq!(parsed.defaults.model.as_deref(), Some("impl-fast"));
        assert_eq!(parsed.defaults.agent.as_ref().map(|a| a.id()), Some("claude-code"));
        assert_eq!(parsed.defaults.agent_mode.as_deref(), Some("yolo"));
        assert_eq!(parsed.defaults.agent_timeout.as_deref(), Some("30m"));
        assert_eq!(parsed.defaults.program_timeout.as_deref(), Some("10m"));
        assert!(parsed.snapshots.is_some());
    }

    #[test]
    fn expand_env_vars_substitutes_present_ignores_missing() {
        std::env::set_var("RHEI_TEST_VAR", "hello");
        assert_eq!(expand_env_vars("/path/${RHEI_TEST_VAR}/y"), "/path/hello/y");
        // Unknown vars expand to empty string per spec.
        std::env::remove_var("RHEI_TEST_UNKNOWN");
        assert_eq!(expand_env_vars("a${RHEI_TEST_UNKNOWN}b"), "ab");
        // No `${` patterns are passed through.
        assert_eq!(expand_env_vars("plain"), "plain");
    }

    #[test]
    fn resolved_agent_log_suffix_includes_visit_count() {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile: CustomAgentProfile {
                command: vec!["claude".to_string()],
                ..Default::default()
            },
            mode: None,
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: None,
            model_name: Some("impl-fast".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        // Visit 1 stays unsuffixed (no counted-loop noise on first visit).
        assert_eq!(resolved_agent_log_suffix(&resolved, Some(1)).as_deref(), Some("impl-fast"));
        // Visits > 1 are appended after the model slug.
        assert_eq!(resolved_agent_log_suffix(&resolved, Some(3)).as_deref(), Some("impl-fast-3"));
        // Without a model, the suffix is just the visit count.
        let resolved_no_model = ResolvedAgent { model: None, model_name: None, ..resolved.clone() };
        assert_eq!(resolved_agent_log_suffix(&resolved_no_model, Some(2)).as_deref(), Some("2"));
    }

    #[test]
    fn collect_unsupported_tooling_warnings_reports_dropped_entries() {
        let profile = CustomAgentProfile { command: vec!["x".to_string()], ..Default::default() };
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("noop"),
            profile,
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "linear".to_string(),
                optional: false,
                definition: Some(McpServerProfile::default()),
            }],
            skills: vec![ResolvedSkillEntry {
                id: "test-authoring".to_string(),
                optional: false,
                definition: Some(SkillProfile {
                    path: "/skills/test-authoring".to_string(),
                    description: None,
                }),
            }],
        };
        let warnings = collect_unsupported_tooling_warnings(&resolved, &tooling);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("no mcp_flag/mcp_config_flag")));
        assert!(warnings.iter().any(|w| w.contains("no skill_flag")));
    }

    #[test]
    fn merge_deep_merges_models_agents_by_binding_id() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan_root = dir.path().join("plan");
        let project_dir = plan_root.join(".agent-grounds/rhei");
        fs::create_dir_all(&project_dir).expect("mkdir");
        // Global settings define args; project settings define autonomous_args
        // for the same model-agent binding. Deep-merge must keep both.
        let _global_home = TempHome::new();
        let global_dir = home_dir().expect("home").join(".config/rhei");
        fs::create_dir_all(&global_dir).expect("global dir");
        fs::write(
            global_dir.join("settings.json"),
            r#"{
              "models": {
                "impl-fast": {
                  "provider": "anthropic",
                  "model": "claude-sonnet-4-6",
                  "agents": {
                    "claude-code": { "args": ["--global-arg"] }
                  }
                }
              }
            }"#,
        )
        .expect("write global");
        fs::write(
            project_dir.join("settings.json"),
            r#"{
              "models": {
                "impl-fast": {
                  "agents": {
                    "claude-code": { "autonomous_args": ["--project-arg"] }
                  }
                }
              }
            }"#,
        )
        .expect("write project");
        let merged = load_merged_settings(&plan_root).expect("merge settings");
        let model = merged.models.get("impl-fast").expect("impl-fast survives");
        // Project did not respecify provider/model, so they inherit.
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert_eq!(model.model.as_deref(), Some("claude-sonnet-4-6"));
        let binding = model.agents.get("claude-code").expect("claude-code binding");
        // The project entry replaces the agent binding by id (wholesale per
        // bindings), but the merge is at the model level — both registries
        // observe the deep-merge of the inner `agents` map.
        assert_eq!(binding.autonomous_args, vec!["--project-arg".to_string()]);
    }

    #[test]
    fn settings_missing_file_defaults_and_malformed_project_errors() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let missing = dir.path().join("missing-settings.json");
        let settings = load_settings(&missing).expect("missing settings default");
        assert!(settings.agent.is_none());

        let _home = TempHome::new();
        let plan_root = dir.path().join("plan");
        // Rejoined component by component from the loader's own relative path,
        // so the expectation carries the platform's separator the way the
        // diagnostic does, rather than the constant's spelling.
        let project_settings = Path::new(PROJECT_SETTINGS_RELATIVE_PATH)
            .components()
            .fold(plan_root.clone(), |path, part| path.join(part));
        fs::create_dir_all(project_settings.parent().expect("settings parent")).expect("mkdir");
        fs::write(&project_settings, "{ not json").expect("write malformed");

        let err = load_merged_settings(&plan_root).expect_err("malformed project settings fails");
        let msg = err.to_string();
        assert!(msg.contains(project_settings.to_string_lossy().as_ref()), "{msg}");
    }

    #[test]
    fn settings_merge_project_null_clears_inherited_optional_defaults() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan_root = dir.path().join("plan");
        let project_dir = plan_root.join(".agent-grounds/rhei");
        fs::create_dir_all(&project_dir).expect("mkdir");
        let _home = TempHome::new();
        let global_dir = home_dir().expect("home").join(".config/rhei");
        fs::create_dir_all(&global_dir).expect("global dir");
        fs::write(
            global_dir.join("settings.json"),
            r#"{
              "agent": "codex",
              "agent_mode": "yolo",
              "model": "impl-fast",
              "defaults": {
                "agent": "codex",
                "agent_mode": "yolo",
                "model": "impl-fast"
              },
              "models": {
                "impl-fast": {
                  "provider": "anthropic",
                  "model": "claude-sonnet-4-6",
                  "default_agent": "codex",
                  "agents": {
                    "codex": { "timeout": "30m" }
                  }
                }
              }
            }"#,
        )
        .expect("write global");
        fs::write(
            project_dir.join("settings.json"),
            r#"{
              "agent": null,
              "agent_mode": null,
              "model": null,
              "defaults": {
                "agent": null,
                "agent_mode": null,
                "model": null
              },
              "models": {
                "impl-fast": {
                  "default_agent": null,
                  "agents": {
                    "codex": { "timeout": null }
                  }
                }
              }
            }"#,
        )
        .expect("write project");

        let merged = load_merged_settings(&plan_root).expect("merge settings");
        assert!(merged.agent.is_none());
        assert!(merged.agent_mode.is_none());
        assert!(merged.model.is_none());
        assert!(merged.defaults.agent.is_none());
        assert!(merged.defaults.agent_mode.is_none());
        assert!(merged.defaults.model.is_none());
        let model = merged.models.get("impl-fast").expect("model inherited");
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert!(model.default_agent.is_none());
        assert!(model.agents.get("codex").expect("binding").timeout.is_none());
    }

    #[test]
    fn model_registry_resolve_legacy_agent_rejects_unknown_configured_model_id() {
        let mut settings = default_settings();
        settings.defaults.model = Some("missing-model".to_string());
        settings.defaults.agent = Some(AgentConfig::from("codex"));

        let err =
            match resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None) {
                Ok(_) => panic!("unknown model id must fail"),
                Err(err) => err,
            };
        assert!(err.to_string().contains("model 'missing-model' is not defined"));

        settings.defaults.model = None;
        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent resolves without model registry context")
                .expect("agent exists");
        assert!(resolved.model.is_none());
    }

    #[test]
    fn run_readiness_excludes_assigned_tasks_without_changing_ready_semantics() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Assigned

## Tasks

### Task 1: Claimed
**State:** pending
**Assignee:** codex

### Task 2: Open
**State:** pending
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let dir = tempfile::tempdir().expect("tmpdir");
        let ready = find_ready_tasks(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &ReadySetRoots::plan_only(dir.path()), &HashSet::new());
        assert_eq!(ready.len(), 2);
        let runnable = find_runnable_tasks(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &ReadySetRoots::plan_only(dir.path()), &HashSet::new());
        assert_eq!(
            runnable.iter().map(|task| task.id.to_string()).collect::<Vec<_>>(),
            vec!["2".to_string()]
        );
    }

    /// One eligibility rule for parents, shared by `rhei next` and `rhei run`:
    /// a non-leaf task joins the ready set only once its whole subtree is
    /// terminal, and then it joins it like any other ticket.
    // §FS-rhei-plan-language.3 §FS-rhei-next.3 §FS-rhei-run.3
    #[test]
    fn readiness_admits_a_parent_only_once_its_subtree_is_terminal() {
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let machines = rhei_validator::MachineSet::single(machine);
        let dir = tempfile::tempdir().expect("tmpdir");
        let ready_ids = |plan: &str| {
            let rhei = rhei_core::parse(plan).expect("parse plan");
            find_ready_tasks(&rhei, &machines, &ReadySetRoots::plan_only(dir.path()), &HashSet::new())
                .iter()
                .map(|task| task.id.to_string())
                .collect::<Vec<_>>()
        };

        // A grandchild left open holds back both of its ancestors.
        assert_eq!(
            ready_ids(
                r#"# Rhei: Open subtree
---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** pending

#### Task 1.1: Child
**State:** pending

##### Task 1.1.1: Grandchild
**State:** pending
"#
            ),
            vec!["1.1.1".to_string()]
        );

        // The child terminal but the grandchild still open: still only the
        // grandchild, since the rule reads the whole subtree, not one level.
        assert_eq!(
            ready_ids(
                r#"# Rhei: Deep open subtree
---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** pending

#### Task 1.1: Child
**State:** done

##### Task 1.1.1: Grandchild
**State:** pending
"#
            ),
            vec!["1.1.1".to_string()]
        );

        // Whole subtree terminal: the parent is ordinary ready work.
        assert_eq!(
            ready_ids(
                r#"# Rhei: Closed subtree
---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** pending

#### Task 1.1: Child
**State:** done

##### Task 1.1.1: Grandchild
**State:** done
"#
            ),
            vec!["1".to_string()]
        );
    }

    /// A cancelled descendant is terminal, so it releases its parent — the
    /// guard is about open work, not about success.
    // §FS-rhei-transition-cmd.3.1
    #[test]
    fn readiness_treats_a_cancelled_descendant_as_closed() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Cancelled child

## Tasks

### Task 1: Parent
**State:** pending

#### Task 1.1: Abandoned
**State:** cancelled
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  cancelled:\n    description: abandoned\n    final: true\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let dir = tempfile::tempdir().expect("tmpdir");
        let ready = find_ready_tasks(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            &ReadySetRoots::plan_only(dir.path()),
            &HashSet::new(),
        );
        assert_eq!(
            ready.iter().map(|task| task.id.to_string()).collect::<Vec<_>>(),
            vec!["1".to_string()]
        );
    }

    #[test]
    fn run_readiness_excludes_gating_tasks() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Gate

## Tasks

### Task 1: Review
**State:** human-review
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  human-review:\n    description: review\n    gating: true\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: human-review\n    to: done\n",
        );
        let dir = tempfile::tempdir().expect("tmpdir");

        // §FS-rhei-run-tui.1.5.7: gate-only runs reach the empty-ready wait path.
        assert!(find_runnable_tasks(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &ReadySetRoots::plan_only(dir.path()), &HashSet::new()).is_empty());
    }

    #[test]
    fn human_gate_wait_requires_gates_to_be_the_remaining_blocker() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Mixed Stuck

## Tasks

### Task 1: Human review
**State:** human-review

### Task 2: Missing input
**State:** work
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  human-review:
    description: review
    gating: true
  work:
    description: needs input
    inputs:
      - name: brief
        path: runtime/brief.md
  done:
    description: terminal
    final: true
transitions:
  - from: human-review
    to: done
  - from: work
    to: done
"#,
        );
        let dir = tempfile::tempdir().expect("tmpdir");

        assert!(find_runnable_tasks(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &ReadySetRoots::plan_only(dir.path()), &HashSet::new()).is_empty());
        assert!(has_pending_human_gate(&rhei, &rhei_validator::MachineSet::single(machine.clone())));
        assert!(!should_wait_for_human_gate(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &None));
    }

    #[test]
    fn human_gate_wait_allows_gate_blocked_remaining_work() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Gate Blocked

## Tasks

### Task 1: Human review
**State:** human-review

### Task 2: After approval
**State:** work
**Prior:** Task 1
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  human-review:
    description: review
    gating: true
  work:
    description: work
  done:
    description: terminal
    final: true
transitions:
  - from: human-review
    to: done
  - from: work
    to: done
"#,
        );
        let dir = tempfile::tempdir().expect("tmpdir");

        assert!(find_runnable_tasks(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &ReadySetRoots::plan_only(dir.path()), &HashSet::new()).is_empty());
        assert!(should_wait_for_human_gate(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &None));
    }

    /// A parent left non-terminal because its subtree is gated is blocked by
    /// the gate, not stuck: it must not stop an interactive run from waiting
    /// for the human decision that unblocks the whole branch.
    // §FS-rhei-plan-language.3 §FS-rhei-run.3
    #[test]
    fn human_gate_wait_allows_a_parent_held_by_a_gated_descendant() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Gated Subtree

## Tasks

### Task 1: Parent
**State:** work

#### Task 1.1: Human review
**State:** human-review
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  human-review:
    description: review
    gating: true
  work:
    description: work
  done:
    description: terminal
    final: true
transitions:
  - from: human-review
    to: done
  - from: work
    to: done
"#,
        );
        let machines = rhei_validator::MachineSet::single(machine);
        let dir = tempfile::tempdir().expect("tmpdir");

        // The parent is not schedulable while its subtree is open, and the
        // child is gated, so nothing is runnable — but the gate is the reason.
        assert!(find_runnable_tasks(&rhei, &machines, &ReadySetRoots::plan_only(dir.path()), &HashSet::new()).is_empty());
        assert!(should_wait_for_human_gate(&rhei, &machines, &None));
    }

    /// The dependent case: a ticket whose `**Prior:**` is a parent held open by
    /// a gated child inherits the gate. Reading the prior's own state alone saw
    /// a non-gating parent with no priors and called the run stalled, so an
    /// interactive run refused to hold the gate open and a batch run exited
    /// non-zero with "no further advancement possible".
    // §FS-rhei-plan-language.3 §FS-rhei-run.3
    #[test]
    fn human_gate_wait_allows_a_dependent_of_a_parent_held_by_a_gated_descendant() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Gated Subtree

## Tasks

### Task 1: Parent
**State:** work

#### Task 1.1: Human review
**State:** human-review

### Task 2: Dependent
**State:** work
**Prior:** Task 1
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  human-review:
    description: review
    gating: true
  work:
    description: work
  done:
    description: terminal
    final: true
transitions:
  - from: human-review
    to: done
  - from: work
    to: done
"#,
        );
        let machines = rhei_validator::MachineSet::single(machine);
        let dir = tempfile::tempdir().expect("tmpdir");

        // Nothing is runnable: the parent waits on its subtree, the child is
        // gated, and the dependent waits on the parent — one gate, three
        // tickets, and the gate is still the whole reason.
        assert!(find_runnable_tasks(&rhei, &machines, &ReadySetRoots::plan_only(dir.path()), &HashSet::new()).is_empty());
        assert!(remaining_work_is_only_gating_or_poll_blocked(&rhei, &machines, &None));
        assert!(should_wait_for_human_gate(&rhei, &machines, &None));
    }

    /// Two children of one parent, one gated and one waiting on it. Both
    /// orderings must reach the same verdict: the gate holds the sibling, the
    /// sibling holds nothing else, and the parent is waiting on a human.
    ///
    /// A single *visited* set could not answer this. A revisit is neutral-true
    /// inside the descendant `.all()` and neutral-false inside the prior
    /// `.any()`, so whichever child the preorder walk consumed first decided the
    /// parent's verdict: gate-then-dependent read as stuck and exited one,
    /// dependent-then-gate read as waiting and exited zero.
    // §FS-rhei-plan-language.3 §FS-rhei-run-tui.1.5.5
    #[test]
    fn human_gate_wait_allows_a_sibling_waiting_on_a_gated_sibling_in_either_order() {
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  human-review:
    description: review
    gating: true
  work:
    description: work
  done:
    description: terminal
    final: true
transitions:
  - from: human-review
    to: done
  - from: work
    to: done
"#,
        );
        let machines = rhei_validator::MachineSet::single(machine);

        // The gate is reached first by the preorder descendant walk.
        let gate_first = rhei_core::parse(
            r#"# Rhei: Gated Sibling

## Tasks

### Task 1: Parent
**State:** work

#### Task 1.1: Human review
**State:** human-review

#### Task 1.2: Waits on the review
**State:** work
**Prior:** Task 1.1
"#,
        )
        .expect("parse plan");

        // The dependent is reached first; only the source order differs.
        let dependent_first = rhei_core::parse(
            r#"# Rhei: Gated Sibling

## Tasks

### Task 1: Parent
**State:** work

#### Task 1.1: Waits on the review
**State:** work
**Prior:** Task 1.2

#### Task 1.2: Human review
**State:** human-review
"#,
        )
        .expect("parse plan");

        for (label, rhei) in [("gate first", &gate_first), ("dependent first", &dependent_first)] {
            assert!(
                remaining_work_is_only_gating_or_poll_blocked(rhei, &machines, &None),
                "{label}: one gate holds the whole subtree, so nothing is stuck"
            );
            assert!(
                should_wait_for_human_gate(rhei, &machines, &None),
                "{label}: an interactive run must hold the gate open"
            );
        }
    }

    /// The other half of the same walk: a dependent of a parent whose subtree
    /// is genuinely stuck is stuck too, so the run must not sit waiting on a
    /// gate elsewhere in the plan.
    // §FS-rhei-plan-language.3
    #[test]
    fn human_gate_wait_refuses_a_dependent_of_a_parent_with_a_stuck_descendant() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Stuck Subtree

## Tasks

### Task 1: Human review
**State:** human-review

### Task 2: Parent
**State:** work

#### Task 2.1: Missing input
**State:** needs-input

### Task 3: Dependent
**State:** work
**Prior:** Task 2
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  human-review:
    description: review
    gating: true
  work:
    description: work
  needs-input:
    description: needs input
    inputs:
      - name: brief
        path: runtime/brief.md
  done:
    description: terminal
    final: true
transitions:
  - from: human-review
    to: done
  - from: work
    to: done
  - from: needs-input
    to: done
"#,
        );
        let machines = rhei_validator::MachineSet::single(machine);

        assert!(has_pending_human_gate(&rhei, &machines));
        assert!(!remaining_work_is_only_gating_or_poll_blocked(&rhei, &machines, &None));
        assert!(!should_wait_for_human_gate(&rhei, &machines, &None));
    }

    #[test]
    fn human_gate_wait_ignores_terminal_gating_states() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Terminal Gate

## Tasks

### Task 1: Already approved
**State:** approved
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            r#"name: t
version: 1
states:
  approved:
    description: terminal gate
    gating: true
    final: true
transitions: []
"#,
        );
        let dir = tempfile::tempdir().expect("tmpdir");

        assert!(find_runnable_tasks(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &ReadySetRoots::plan_only(dir.path()), &HashSet::new()).is_empty());
        assert!(!has_pending_human_gate(&rhei, &rhei_validator::MachineSet::single(machine.clone())));
        assert!(!should_wait_for_human_gate(&rhei, &rhei_validator::MachineSet::single(machine.clone()), &None));
    }

    #[test]
    fn defaults_only_agent_mode_selects_agent_mode_for_effective_agents() {
        let rhei = rhei_core::parse(
            "# Rhei: Test\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n\nDo work.\n",
        )
        .expect("parse plan");
        let dir = tempfile::tempdir().expect("tmpdir");
        let bare_machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let model_machine = machine_with_states(
            "name: t\nversion: 1\nmodels:\n  - impl-fast\nstates:\n  pending:\n    description: x\n    model: impl-fast\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );

        let mut cli_opts = default_run_options();
        cli_opts.agent.agent = Some("codex".to_string());
        assert!(should_use_agent_mode(&rhei, &rhei_validator::MachineSet::single(bare_machine.clone()), &default_settings(), &cli_opts, &ReadySetRoots::plan_only(dir.path()))
            .expect("cli agent mode selection"));

        let mut defaults_agent = default_settings();
        defaults_agent.defaults.agent = Some(AgentConfig::from("codex"));
        assert!(should_use_agent_mode(&rhei, &rhei_validator::MachineSet::single(bare_machine.clone()), &defaults_agent, &default_run_options(), &ReadySetRoots::plan_only(dir.path()))
            .expect("defaults.agent mode selection"));

        let mut defaults_model = default_settings();
        defaults_model.defaults.model = Some("impl-fast".to_string());
        defaults_model.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet".to_string()),
                default_agent: Some("codex".to_string()),
                agents: BTreeMap::new(),
                prices: None,
            },
        );
        assert!(should_use_agent_mode(&rhei, &rhei_validator::MachineSet::single(bare_machine.clone()), &defaults_model, &default_run_options(), &ReadySetRoots::plan_only(dir.path()))
            .expect("defaults.model mode selection"));

        let mut model_default_agent = default_settings();
        model_default_agent.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet".to_string()),
                default_agent: Some("codex".to_string()),
                agents: BTreeMap::new(),
                prices: None,
            },
        );
        assert!(should_use_agent_mode(&rhei, &rhei_validator::MachineSet::single(model_machine.clone()), &model_default_agent, &default_run_options(), &ReadySetRoots::plan_only(dir.path()))
            .expect("models.<id>.default_agent mode selection"));
    }

    #[test]
    fn tooling_gate_classifies_required_optional_and_transition_triggers() {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("noop"),
            profile: CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "optional-mcp".to_string(),
                optional: true,
                definition: None,
            }],
            skills: vec![ResolvedSkillEntry {
                id: "required-skill".to_string(),
                optional: false,
                definition: Some(SkillProfile {
                    path: "/tmp/skill".to_string(),
                    description: None,
                }),
            }],
        };

        let gate = gate_tooling_for_agent(&resolved, &tooling);
        assert_eq!(gate.tooling.mcp_servers.len(), 1);
        assert_eq!(gate.tooling.mcp_servers[0].id, "optional-mcp");
        assert!(gate.tooling.mcp_servers[0].definition.is_none());
        assert!(gate.tooling.skills.is_empty());
        assert_eq!(gate.warnings.len(), 1);
        assert_eq!(gate.required.len(), 1);
        assert_eq!(gate.required[0].kind, ToolingKind::Skill);
        assert_eq!(gate.required[0].id, "required-skill");

        assert!(tooling_trigger_matches(
            &serde_yaml::Value::Bool(true),
            &["required-skill".to_string()]
        ));
        assert!(tooling_trigger_matches(
            &serde_yaml::from_str("[other, required-skill]").expect("yaml"),
            &["required-skill".to_string()]
        ));
        assert!(!tooling_trigger_matches(
            &serde_yaml::from_str("[other]").expect("yaml"),
            &["required-skill".to_string()]
        ));
    }

    #[test]
    fn optional_tooling_availability_preserves_prompt_env_and_log_visibility() {
        let machine = machine_with_states(
            r#"name: optional-tooling
version: 1
states:
  pending:
    description: x
    instructions: "mcp={mcp.optional-mcp.available} skill={skill.optional-skill.available}"
  done:
    description: terminal
    final: true
transitions:
  - from: pending
    to: done
"#,
        );
        let rhei = rhei_core::parse(
            r#"# Rhei: Optional Tooling

## Tasks

### Task 1: Work
**State:** pending
"#,
        )
        .expect("parse plan");
        let task = rhei.tasks.first().expect("task");
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "optional-mcp".to_string(),
                optional: true,
                definition: None,
            }],
            skills: vec![ResolvedSkillEntry {
                id: "optional-skill".to_string(),
                optional: true,
                definition: None,
            }],
        };
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: built_in_agents().remove("codex").expect("codex"),
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let gate = gate_tooling_for_agent(&resolved, &tooling);
        assert_eq!(gate.tooling.mcp_servers.len(), 1);
        assert_eq!(gate.tooling.skills.len(), 1);

        let render_context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: Path::new("/tmp/workspace"),
            checkout_root: Path::new("/tmp/workspace"),
            plan_path: Path::new("/tmp/workspace/plan.rhei.md"),
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name: "pending",
            current_state_raw: task.state.as_str(),
            machine: &machine,
            metadata: rhei.metadata.as_ref(),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: Some(&gate.tooling),
            memory: None,
        };
        let prompt = compose_agent_prompt(&render_context).expect("prompt");
        assert!(prompt.contains("mcp=false skill=false"), "{prompt}");

        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "prompt",
            runtime_dir.path(),
            None,
            None,
            "1",
            "pending",
            1,
            1,
            &gate.tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        assert!(!args.windows(2).any(|pair| pair == ["--mcp", "optional-mcp"]));
        let envs: BTreeMap<String, String> = command
            .get_envs()
            .filter_map(|(k, v)| {
                let key = k.to_string_lossy().into_owned();
                v.map(|val| (key, val.to_string_lossy().into_owned()))
            })
            .collect();
        assert_eq!(
            envs.get("RHEI_MCP_OPTIONAL_MCP_AVAILABLE").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            envs.get("RHEI_SKILL_OPTIONAL_SKILL_AVAILABLE").map(String::as_str),
            Some("false")
        );

        let command = write_quiet_fake_agent(runtime_dir.path());
        let log_path = runtime_dir.path().join("agent.log");
        let log_resolved = ResolvedAgent {
            profile: CustomAgentProfile { command, ..Default::default() },
            ..resolved
        };
        spawn_and_wait_agent(
            &log_resolved,
            &builtin_price_book(),
            "prompt",
            runtime_dir.path(),
            runtime_dir.path(),
            None,
            runtime_dir.path(),
            None,
            "1",
            "pending",
            1,
            &gate.tooling,
            &log_path,
            runtime_dir.path(),
            None,
            0,
            Arc::new(RecordingSink::default()),
            None,
            &spawn_plan_for_test(&log_path),
            None,
        None,
)
        .expect("agent runs");
        let log = fs::read_to_string(log_path).expect("read log");
        assert!(log.contains("\nmcp_servers: optional-mcp?\n"), "{log}");
        assert!(log.contains("\nskills: optional-skill?\n"), "{log}");
    }

    #[test]
    fn tooling_required_missing_skill_blocks_fake_agent_spawn() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        let settings_dir = dir.path().join(".agent-grounds/rhei");
        fs::create_dir_all(&settings_dir).expect("mkdir");
        let spawned = dir.path().join("spawned");
        // The path is JSON-encoded, which is also a Python string literal, so a
        // backslash in a Windows path cannot escape into the fixture.
        let command = python_fixture_command(
            dir.path(),
            "fake-agent",
            &format!(
                "import pathlib\n\npathlib.Path({}).touch()\n",
                serde_json::to_string(spawned.to_string_lossy().as_ref()).expect("json")
            ),
        );
        fs::write(
            &plan,
            r#"# Rhei: Missing Skill

## Tasks

### Task 1: Work
**State:** pending
"#,
        )
        .expect("plan");
        fs::write(
            &states,
            r#"name: missing-skill
version: 1
states:
  pending:
    description: pending
    agent: fake
    agent_timeout: 1s
    skills:
      - missing
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
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
                  }},
                  "skills": {{
                    "missing": {{ "path": {} }}
                  }}
                }}"#,
                serde_json::to_string(&command).expect("json"),
                serde_json::to_string(
                    dir.path().join("does-not-exist").to_string_lossy().as_ref()
                )
                .expect("json")
            ),
        )
        .expect("settings");

        let mut opts = default_run_options();
        opts.standalone.continue_on_error = true;
        opts.standalone.no_tui = true;
        let err = run_command(&plan, Some(&states), opts)
            .expect_err("required unavailable tooling leaves non-terminal work");
        assert!(
            err.to_string().contains("non-terminal tasks remaining"),
            "unexpected run error: {err}"
        );

        assert!(!spawned.exists(), "required missing skill must block agent spawn");
    }
