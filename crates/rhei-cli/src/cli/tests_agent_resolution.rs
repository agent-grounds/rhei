    #[test]
    fn resolve_legacy_agent_uses_defaults_agent_timeout() {
        let settings = RheiSettings {
            project_settings_file: ProjectSettingsFile::default(),
            agent: Some(AgentConfig::from("codex")),
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults {
                model: None,
                agent: None,
                agent_mode: None,
                agent_timeout: Some("45m".to_string()),
                program_timeout: None,
                attempts: None,
                mcp_servers: None,
                skills: None,
            },
            agents: built_in_agents(),
            models: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            skills: BTreeMap::new(),
            snapshots: None,
        };

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent should resolve")
                .expect("agent should exist");

        assert_eq!(resolved.timeout_secs, Some(45 * 60));
    }

    fn default_settings() -> RheiSettings {
        RheiSettings {
            project_settings_file: ProjectSettingsFile::default(),
            agent: None,
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults::default(),
            agents: built_in_agents(),
            models: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            skills: BTreeMap::new(),
            snapshots: None,
        }
    }

    #[test]
    fn resolve_legacy_agent_pulls_default_agent_from_model_registry() {
        // When no agent is configured at any level, the resolution order
        // §FS-rhei-agents.1.4: Fallback to model default_agent.
        let mut settings = default_settings();
        settings.model = Some("impl-fast".to_string());
        settings.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                default_agent: Some("claude-code".to_string()),
                agents: BTreeMap::new(),
                prices: None,
            },
        );

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent should resolve")
                .expect("agent should be selected via models.<id>.default_agent");

        assert_eq!(resolved.agent.id(), "claude-code");
        assert_eq!(resolved.model.as_deref(), Some("impl-fast"));
        assert_eq!(resolved.model_provider.as_deref(), Some("anthropic"));
        assert_eq!(resolved.model_name.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn resolve_legacy_agent_prefers_model_agent_binding_timeout() {
        // `models.<id>.agents.<agent>.timeout` sits between state-level and
        // agent-profile timeouts in the resolution chain.
        let mut settings = default_settings();
        settings.agent = Some(AgentConfig::from("claude-code"));
        settings.model = Some("impl-fast".to_string());
        let mut agents = BTreeMap::new();
        agents.insert(
            "claude-code".to_string(),
            ModelAgentBinding {
                args: Vec::new(),
                autonomous_args: Vec::new(),
                timeout: Some("90m".to_string()),
            },
        );
        settings.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                default_agent: None,
                agents,
                prices: None,
            },
        );
        settings.defaults.agent_timeout = Some("30m".to_string());

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent should resolve")
                .expect("agent should exist");

        assert_eq!(resolved.timeout_secs, Some(90 * 60));
    }

    #[test]
    fn target_selector_literal_model_uses_selector_values_when_registry_collides() {
        let mut settings = default_settings();
        let mut agents = BTreeMap::new();
        agents.insert(
            "codex".to_string(),
            ModelAgentBinding {
                timeout: Some("2m".to_string()),
                ..Default::default()
            },
        );
        settings.models.insert(
            "literal-model".to_string(),
            ModelProfile {
                provider: Some("registry-provider".to_string()),
                model: Some("registry-concrete-model".to_string()),
                default_agent: None,
                agents,
                prices: None,
            },
        );

        let resolved =
            resolve_target_agent("codex:selector-provider:literal-model", None, &settings)
                .expect("target resolves");

        assert_eq!(resolved.model.as_deref(), Some("literal-model"));
        assert_eq!(resolved.model_provider.as_deref(), Some("selector-provider"));
        assert_eq!(resolved.model_name.as_deref(), Some("literal-model"));
        assert_eq!(resolved.timeout_secs, Some(120));
    }

    /// Spawn time refuses an undeclared mode with the clause `validate` gives
    /// the same selector, so the category has one wording rather than one per
    /// surface. `validate` shadows this at `rhei run`, but not at
    /// `rhei snapshot continue`, where a record's mode outlives the settings
    /// that declared it. §FS-rhei-errors.1.4 §FS-rhei-errors.6
    #[test]
    fn a_target_selectors_mode_is_refused_with_where_modes_are_declared() {
        let selector = "codex[bogus]:openai:gpt";
        let Err(err) = resolve_target_agent(selector, None, &default_settings()) else {
            panic!("an undeclared mode must be refused");
        };
        assert!(err.to_string().contains("has no mode 'bogus'"), "got: {err}");
        let help = err.help().expect("the refusal carries help").to_string();
        assert!(help.contains("`agents.codex.modes`"), "got: {help}");
        assert!(help.contains(".agent-grounds/rhei/settings.json"), "got: {help}");
        assert!(help.contains("~/.config/rhei/settings.json"), "got: {help}");
    }

    #[test]
    fn mode_default_order_uses_declaration_order() {
        let settings: RheiSettings = serde_json::from_str(
            r#"{
              "defaults": { "agent": "custom" },
              "agents": {
                "custom": {
                  "command": ["custom-agent"],
                  "modes": {
                    "yolo": ["--yolo"],
                    "safe": ["--safe"]
                  }
                }
              }
            }"#,
        )
        .expect("settings parse");

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent resolves")
                .expect("agent exists");

        assert_eq!(resolved.mode.as_deref(), Some("yolo"));
    }

    #[test]
    fn build_agent_command_uses_concrete_model_name_for_flag() {
        // The `--model` flag should receive the registry-resolved concrete
        // model name (`claude-sonnet-4-6`), not the rhei profile id
        // (`impl-fast`).
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/tmp/workspace"),
            None,
            None,
            "task-1",
            "pending",
            7,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();

        let model_idx =
            args.iter().position(|arg| arg == "--model").expect("--model flag should be present");
        assert_eq!(args.get(model_idx + 1).map(String::as_str), Some("claude-sonnet-4-6"));
        assert!(!args.iter().any(|arg| arg == "impl-fast"));

        let envs: BTreeMap<String, String> = command
            .get_envs()
            .filter_map(|(k, v)| {
                let key = k.to_string_lossy().into_owned();
                v.map(|val| (key, val.to_string_lossy().into_owned()))
            })
            .collect();
        assert_eq!(envs.get("RHEI_MODEL").map(String::as_str), Some("impl-fast"));
        assert_eq!(envs.get("RHEI_MODEL_PROVIDER").map(String::as_str), Some("anthropic"));
        assert_eq!(envs.get("RHEI_MODEL_NAME").map(String::as_str), Some("claude-sonnet-4-6"));
        assert_eq!(envs.get("RHEI_VISIT_COUNT").map(String::as_str), Some("7"));
    }

    /// §FS-rhei-agents.4: every autonomous profile gets the same sanitized
    /// spawn boundary while retaining non-identity invocation details.
    #[test]
    fn autonomous_agent_command_removes_outer_identity_and_keeps_invocation_details() {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("custom-wrapper"),
            profile: CustomAgentProfile {
                command: vec!["custom-wrapper".to_string()],
                ..Default::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/checkout"),
            None,
            Some(Path::new("/outer/states.yaml")),
            "outer.1",
            "work",
            3,
            2,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let envs: BTreeMap<String, Option<String>> = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();

        for name in [
            "RHEI_ROOT",
            "RHEI_PLAN_PATH",
            "RHEI_RESULT_PATH",
            "RHEI_TASK_ID",
            "RHEI_TASK_ID_LOCAL",
        ] {
            assert_eq!(envs.get(name), Some(&None), "{name} must be removed at spawn");
        }
        assert_eq!(envs.get("RHEI_CHECKOUT_ROOT"), Some(&Some("/checkout".to_string())));
        assert_eq!(envs.get("RHEI_STATE"), Some(&Some("work".to_string())));
        assert_eq!(envs.get("RHEI_VISIT_COUNT"), Some(&Some("3".to_string())));
        assert_eq!(envs.get("RHEI_ATTEMPT"), Some(&Some("2".to_string())));
        assert_eq!(envs.get("RHEI_AGENT"), Some(&Some("custom-wrapper".to_string())));
    }

    #[test]
    fn ordinary_builtin_claude_requests_typed_json_without_intervention_flags() {
        // §FS-rhei-cost-accounting.4: ordinary Claude launches must emit typed usage JSON.
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: Some("yolo".to_string()),
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/tmp/workspace"),
            None,
            None,
            "task-1",
            "pending",
            1,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();

        assert_eq!(
            args.windows(2).filter(|pair| *pair == ["--output-format", "json"]).count(),
            1,
            "ordinary Claude command must request JSON exactly once: {args:?}"
        );
        assert!(
            args.windows(2).any(|pair| pair == ["-p", "--model"]),
            "the print flag is bare and the prompt is on stdin: {args:?}"
        );
        assert!(!args.iter().any(|arg| arg == "do work"), "prompt leaked into argv: {args:?}");
        assert!(!args.iter().any(|arg| arg == "--input-format"), "stream input leaked: {args:?}");
        assert!(!args.iter().any(|arg| arg == "--verbose"), "stream verbose leaked: {args:?}");
    }

    #[test]
    fn claude_code_intervention_uses_stream_json_stdin_command() {
        let mut profile = built_in_agents().remove("claude-code").expect("claude-code");
        profile.intervene_stdin = true;
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/tmp/workspace"),
            None,
            None,
            "task-1",
            "pending",
            1,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();

        assert!(args.iter().any(|arg| arg == "-p"), "claude print mode required: {args:?}");
        assert!(!args.iter().any(|arg| arg == "do work"), "prompt must move to stdin: {args:?}");
        let input_idx =
            args.iter().position(|arg| arg == "--input-format").expect("--input-format");
        assert_eq!(args.get(input_idx + 1).map(String::as_str), Some("stream-json"));
        let output_idx =
            args.iter().position(|arg| arg == "--output-format").expect("--output-format");
        assert_eq!(args.get(output_idx + 1).map(String::as_str), Some("stream-json"));
        assert!(args.iter().any(|arg| arg == "--verbose"), "stream-json output requires verbose");
        let model_idx = args.iter().position(|arg| arg == "--model").expect("--model");
        assert_eq!(args.get(model_idx + 1).map(String::as_str), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn build_agent_command_falls_back_to_model_id_when_registry_missing() {
        // Backward-compatible behavior: an unregistered model id is passed
        // through as the concrete model name.
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: Some("gpt-5".to_string()),
            model_provider: None,
            model_name: Some("gpt-5".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/tmp/workspace"),
            None,
            None,
            "task-1",
            "pending",
            1,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        let model_idx = args.iter().position(|arg| arg == "--model").expect("--model");
        assert_eq!(args.get(model_idx + 1).map(String::as_str), Some("gpt-5"));
    }

    #[test]
    fn settings_parse_models_registry() {
        let json = r#"{
          "models": {
            "impl-fast": {
              "provider": "anthropic",
              "model": "claude-sonnet-4-6",
              "default_agent": "claude-code",
              "agents": {
                "claude-code": {
                  "args": ["--permission-mode", "default"],
                  "autonomous_args": ["--permission-mode", "bypassPermissions"],
                  "timeout": "1h"
                }
              }
            }
          }
        }"#;
        let parsed: RheiSettings = serde_json::from_str(json).expect("parse settings");
        let model = parsed.models.get("impl-fast").expect("impl-fast model");
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert_eq!(model.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(model.default_agent.as_deref(), Some("claude-code"));
        let binding = model.agents.get("claude-code").expect("claude-code binding");
        assert_eq!(binding.timeout.as_deref(), Some("1h"));
    }

    #[test]
    fn format_iso8601_utc_renders_epoch_origin() {
        let epoch = std::time::UNIX_EPOCH;
        assert_eq!(format_iso8601_utc(epoch), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_iso8601_utc_renders_known_instant() {
        // 2026-04-20T10:30:00Z = 1_776_681_000 seconds since epoch.
        let when = std::time::UNIX_EPOCH + Duration::from_secs(1_776_681_000);
        assert_eq!(format_iso8601_utc(when), "2026-04-20T10:30:00Z");
    }

    #[test]
    fn format_duration_human_matches_spec_examples() {
        assert_eq!(format_duration_human(0), "0s");
        assert_eq!(format_duration_human(30), "30s");
        assert_eq!(format_duration_human(5 * 60), "5m");
        assert_eq!(format_duration_human(60 * 60), "1h");
        assert_eq!(format_duration_human(2 * 3600 + 30 * 60), "2h30m");
        assert_eq!(format_duration_human(4 * 60 + 23), "4m23s");
    }

    #[test]
    fn agent_log_header_uses_v1_format_and_spec_fields() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_quiet_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile: CustomAgentProfile { command, ..CustomAgentProfile::default() },
            mode: Some("yolo".to_string()),
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(1800),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };

        spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "prompt",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-log",
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
        )
        .expect("agent runs");

        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.starts_with("=== rhei agent log v1 ==="), "header missing v1: {log}");
        assert!(log.contains("\nprovider: anthropic\n"));
        assert!(log.contains("\nmodel_name: claude-sonnet-4-6\n"));
        assert!(log.contains("\ntimeout: 30m\n"));
        // started/ended ISO timestamps and human-readable duration.
        assert!(log.contains("\nstarted: "));
        assert!(log.contains("\nended: "));
        assert!(log.contains("\nduration: "));
        // No legacy "1800s"-style numeric timeout anywhere.
        assert!(!log.contains("\ntimeout: 1800s\n"));
    }

    /// An agent that says nothing and succeeds: the log header is the subject.
    fn write_quiet_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(dir, "quiet-agent", "import sys\n\nsys.exit(0)\n")
    }

    #[test]
    fn built_in_codex_yolo_includes_approval_never() {
        // The known-agent profile pins codex yolo to a non-interactive approval mode.
        // §FS-rhei-agents.2: Built-in codex yolo is non-interactive.
        let profile = built_in_agents().remove("codex").expect("built-in codex");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile,
            mode: Some("yolo".to_string()),
            target: Some(parse_execution_target("codex[yolo]:openai:gpt-5-codex").expect("target")),
            model: Some("gpt-5-codex".to_string()),
            model_provider: Some("openai".to_string()),
            model_name: Some("gpt-5-codex".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "analyze this",
            Path::new("/tmp/workspace"),
            None,
            None,
            "analysis",
            "analyze",
            1,
            1,
            &tooling,
            runtime_dir.path(),
            &[],
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();

        assert!(args.windows(2).any(|pair| pair == ["--sandbox", "danger-full-access"]));
        assert!(args.iter().any(|arg| arg == "--skip-git-repo-check"));
        assert!(args.windows(2).any(|pair| pair == ["-c", "approval_policy=\"never\""]));
    }

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<rhei_tui::RunEvent>>,
    }

    impl rhei_tui::EventSink for RecordingSink {
        fn emit(&self, event: rhei_tui::RunEvent) {
            self.events.lock().expect("recording sink lock").push(event);
        }
    }

    #[test]
    fn output_reader_logs_and_emits_complete_and_partial_lines() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let log_path = dir.path().join("agent.log");
        let log_file = Arc::new(Mutex::new(fs::File::create(&log_path).expect("log file")));
        let recorder = Arc::new(RecordingSink::default());
        let sink: Arc<dyn rhei_tui::EventSink> = recorder.clone();

        let handle = spawn_agent_output_reader(
            std::io::Cursor::new(b"first\npartial".to_vec()),
            rhei_tui::AgentStream::Stdout,
            log_file,
            sink,
            3,
            "task-live".to_string(),
            None,
            Arc::new(Mutex::new(Vec::new())),
        );

        drain_agent_output_reader(handle, rhei_tui::AgentStream::Stdout).expect("reader drains");

        let log = fs::read_to_string(&log_path).expect("read log");
        assert_eq!(log, "first\npartial");

        let events = recorder.events.lock().expect("events");
        assert_eq!(events.len(), 2);
        match &events[0] {
            rhei_tui::RunEvent::AgentOutput { slot, task, stream, line, .. } => {
                assert_eq!(*slot, 3);
                assert_eq!(task, "task-live");
                assert_eq!(*stream, rhei_tui::AgentStream::Stdout);
                assert_eq!(line, "first");
            }
            other => panic!("expected AgentOutput, got {other:?}"),
        }
        match &events[1] {
            rhei_tui::RunEvent::AgentOutput { line, .. } => assert_eq!(line, "partial"),
            other => panic!("expected AgentOutput, got {other:?}"),
        }
    }

    #[test]
    fn output_reader_decodes_multiline_claude_result_into_live_logical_lines() {
        // §FS-rhei-cost-accounting.4 §FS-rhei-run-tui.1.2: the JSON envelope is
        // internal; its result text remains measured, durable, line-oriented output.
        let dir = tempfile::tempdir().expect("tmpdir");
        let log_path = dir.path().join("agent.log");
        let capture_path = dir.path().join("usage.jsonl");
        let log_file = Arc::new(Mutex::new(fs::File::create(&log_path).expect("log file")));
        let recorder = Arc::new(RecordingSink::default());
        let sink: Arc<dyn rhei_tui::EventSink> = recorder.clone();
        let capture = AgentUsageCapture {
            extractor: AgentUsageExtractor::Claude,
            replace_usage_capture: false,
            path: capture_path,
            invocation_id: "task-live::pending::claude-code::visit-1".to_string(),
            task_id: "task-live".to_string(),
            state: "pending".to_string(),
            agent: "claude-code".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            price_book: builtin_price_book(),
            slot: 3,
            cli_session: Arc::new(Mutex::new(None)),
        };
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"first\n\nsecond","usage":{"input_tokens":12,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens":7}}"#;

        let handle = spawn_agent_output_reader(
            std::io::Cursor::new(format!("{result}\n").into_bytes()),
            rhei_tui::AgentStream::Stdout,
            log_file,
            sink,
            3,
            "task-live".to_string(),
            Some(capture),
            Arc::new(Mutex::new(Vec::new())),
        );
        handle.join().expect("reader thread joins").expect("reader drains");

        let log = fs::read_to_string(&log_path).expect("read log");
        assert_eq!(log, "first\n\nsecond\n");
        assert!(!log.contains("\"type\":\"result\""), "raw envelope leaked: {log}");
        let events = recorder.events.lock().expect("events");
        let output = events
            .iter()
            .filter_map(|event| match event {
                rhei_tui::RunEvent::AgentOutput {
                    stream: rhei_tui::AgentStream::Stdout,
                    line,
                    ..
                } => Some(line.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(output, ["first", "", "second"]);
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::UsageReported { usage, .. }
                if usage.input_total.value == Some(12) && usage.output_total.value == Some(7)
        )));
    }

    #[test]
    fn supported_agents_keep_expected_prompt_transports() {
        let agents = built_in_agents();
        // `-p` stays declared and stops carrying the prompt: it is what puts
        // Claude Code in print mode. agent-grounds/rhei#179 §FS-rhei-agents.2
        let claude = agents.get("claude-code").expect("claude-code profile");
        assert_eq!(claude.prompt_flag.as_deref(), Some("-p"));
        assert!(claude.stdin_prompt);

        let codex = agents.get("codex").expect("codex profile");
        assert_eq!(codex.prompt_flag.as_deref(), None);
        assert!(codex.stdin_prompt);

        let pi = agents.get("pi").expect("pi profile");
        assert_eq!(pi.prompt_flag.as_deref(), Some("-p"));
        assert!(!pi.stdin_prompt);
    }

    /// The agent the transport tests run: it echoes back whichever transport
    /// carried the prompt — `-p` on the command line, or stdin after a `--`.
    ///
    /// Each line is flushed as it is written because the tests read the log
    /// while the process is still alive, and the trailing `partial` is written
    /// without a newline so the readers' partial-line path is exercised too.
    fn write_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "fake-agent",
            r#"import sys

print('stdout:start', flush=True)
print('stderr:warn', file=sys.stderr, flush=True)

previous = ''
read_stdin = False
for arg in sys.argv[1:]:
    if previous == '-p':
        print('prompt:' + arg, flush=True)
    if arg == '--':
        read_stdin = True
    previous = arg

if read_stdin:
    for line in sys.stdin:
        print('stdin:' + line.rstrip('\n'), flush=True)

sys.stdout.write('partial')
sys.stdout.flush()
"#,
        )
    }
