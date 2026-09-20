    fn run_fake_agent_profile(
        profile: CustomAgentProfile,
        agent_id: &str,
        prompt: &str,
    ) -> (String, Vec<rhei_tui::RunEvent>) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from(agent_id),
            profile,
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(10),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let status = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            prompt,
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-live",
            "pending",
            1,
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder.clone(),
            None,
            &spawn_plan_for_test(&log_path),
            None,
        None,
)
        .expect("fake agent runs");

        assert!(status.status.success());
        let log = fs::read_to_string(&log_path).expect("read log");
        let events = recorder.events.lock().expect("events").clone();
        (log, events)
    }

    /// An agent that speaks once and then outlives its timeout: the flush is
    /// what lets the timeout test assert the line was kept.
    fn write_sleeping_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "sleeping-agent",
            r#"import time

print('stdout:before-timeout', flush=True)
time.sleep(2)
"#,
        )
    }

    /// An agent that exits at once but leaves a detached grandchild holding the
    /// stdout pipe it inherited — the spawn must not wait for that pipe's EOF.
    fn write_inherited_pipe_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "inherited-pipe-agent",
            r#"import subprocess
import sys

print('stdout:before-background', flush=True)
subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(2)'])
"#,
        )
    }

    /// An agent that reports its argv and echoes every stdin line back, so the
    /// stream-json transport can be read off the log.
    fn write_stream_json_fake_agent(dir: &Path) -> Vec<String> {
        python_fixture_command(
            dir,
            "stream-json-agent",
            r#"import sys

print('args:' + ' '.join(sys.argv[1:]), flush=True)
for line in sys.stdin:
    print('stdin:' + line.rstrip('\n'), flush=True)
"#,
        )
    }

    #[test]
    fn fake_claude_profile_streams_prompt_flag_output() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command,
            prompt_flag: Some("-p".to_string()),
            stdin_prompt: false,
            ..CustomAgentProfile::default()
        };

        let (log, events) = run_fake_agent_profile(profile, "claude-code", "hello claude");

        assert!(log.contains("prompt:hello claude"));
        assert!(log.contains("stderr:warn"));
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "prompt:hello claude"
        )));
    }

    #[test]
    fn fake_codex_profile_streams_stdin_prompt_output() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command,
            stdin_prompt: true,
            ..CustomAgentProfile::default()
        };

        let (log, events) = run_fake_agent_profile(profile, "codex", "hello codex");

        assert!(log.contains("stdin:hello codex"));
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "stdin:hello codex"
        )));
    }

    #[test]
    fn claude_code_intervention_profile_writes_initial_stream_json_prompt() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_stream_json_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command,
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            intervene_stdin: true,
            ..CustomAgentProfile::default()
        };

        let (log, _events) = run_fake_agent_profile(profile, "claude-code", "hello claude");

        assert!(log.contains(
            "args:-p --input-format stream-json --output-format stream-json --verbose"
        ));
        let stdin_line = log
            .lines()
            .find_map(|line| line.strip_prefix("stdin:"))
            .expect("stream-json prompt on stdin");
        let json: serde_json::Value = serde_json::from_str(stdin_line).expect("json prompt");
        assert_eq!(json["type"], "user");
        assert_eq!(json["message"]["role"], "user");
        assert_eq!(json["message"]["content"][0]["text"], "hello claude");
    }

    #[test]
    fn stdin_prompt_dashboard_mode_closes_stdin_for_eof_driven_agents() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile {
                command,
                stdin_prompt: true,
                ..CustomAgentProfile::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(1),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let intervene = Arc::new(RunInterveneSink::new(dir.path().join("runtime")));

        let start = Instant::now();
        let status = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "hello codex",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-live",
            "pending",
            1,
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder,
            Some(&intervene),
            &spawn_plan_for_test(&log_path),
            None,
        None,
)
        .expect("fake stdin agent runs");

        assert!(status.status.success());
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdin:hello codex"));
    }

    #[test]
    fn fake_pi_profile_streams_prompt_flag_output() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command,
            prompt_flag: Some("-p".to_string()),
            stdin_prompt: false,
            ..CustomAgentProfile::default()
        };

        let (log, events) = run_fake_agent_profile(profile, "pi", "hello pi");

        assert!(log.contains("prompt:hello pi"));
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "prompt:hello pi"
        )));
    }

    #[test]
    fn fake_agent_timeout_keeps_output_and_writes_footer() {
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
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };

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
            recorder.clone(),
            None,
            &spawn_plan_for_test(&log_path),
            None,
        None,
)
        .expect("timeout returns process status");

        assert!(!status.status.success());
        assert!(status.timed_out, "spawn_and_wait_agent must flag timeouts");
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdout:before-timeout"));
        assert!(log.contains("=== exit ==="));
        assert!(
            log.contains("agent timed out after"),
            "log must contain spec-required timeout marker, got: {log}"
        );
        let events = recorder.events.lock().expect("events");
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "stdout:before-timeout"
        )));
    }

    #[test]
    fn inherited_output_pipe_does_not_block_agent_completion() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let command = write_inherited_pipe_fake_agent(dir.path());
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
            timeout_secs: Some(10),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };

        let start = Instant::now();
        let status = spawn_and_wait_agent(
            &resolved,
            &builtin_price_book(),
            "prompt",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-pipe",
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
        .expect("agent should complete without waiting for inherited pipe EOF");

        assert!(status.status.success());
        assert!(start.elapsed() < Duration::from_secs(1), "spawn waited for inherited pipe EOF");
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdout:before-background"));
        assert!(log.contains("=== exit ==="));
    }

    // ---------------------------------------------------------------------
    // Closing gaps from completeness audit (impl-rhei-agents).
    //
    // These tests pin the new validation, command-building, and runtime
    // behavior added when the audit gaps for agent execution were closed.

    // §FS-rhei-agents: Agent execution behavior.
    // ---------------------------------------------------------------------

    fn machine_with_states(yaml: &str) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(yaml).expect("valid state machine")
    }

    #[test]
    fn validates_agent_command_required_and_mcp_flag_xor() {
        let mut agents = built_in_agents();
        agents.insert(
            "broken".to_string(),
            CustomAgentProfile {
                command: Vec::new(),
                mcp_flag: Some("--mcp".to_string()),
                mcp_config_flag: Some("--mcp-config".to_string()),
                ..Default::default()
            },
        );
        let settings = RheiSettings { agents, ..default_settings() };
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("'broken' has an empty 'command'")),
            "missing 'command' must error: {errs:?}"
        );
        assert!(
            errs.iter().any(|e| e.contains("mutually exclusive")),
            "mcp_flag/mcp_config_flag XOR must error: {errs:?}"
        );
    }

    #[test]
    fn validates_mcp_server_xor_and_url_requires_transport() {
        let mut settings = default_settings();
        settings.mcp_servers.insert(
            "both".to_string(),
            McpServerProfile {
                command: Some(vec!["x".to_string()]),
                url: Some("https://example".to_string()),
                ..Default::default()
            },
        );
        settings.mcp_servers.insert("neither".to_string(), McpServerProfile::default());
        settings.mcp_servers.insert(
            "url-only".to_string(),
            McpServerProfile { url: Some("https://example".to_string()), ..Default::default() },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(errs.iter().any(|e| e.contains("'both' declares both 'command' and 'url'")));
        assert!(errs.iter().any(|e| e.contains("'neither' must declare exactly one")));
        assert!(errs
            .iter()
            .any(|e| e.contains("'url-only' uses 'url' but does not declare 'transport'")));
    }

    #[test]
    fn validates_models_require_provider_and_model() {
        let mut settings = default_settings();
        settings.models.insert("bad".to_string(), ModelProfile::default());
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(errs.iter().any(|e| e.contains("'bad' is missing required field 'provider'")));
        assert!(errs.iter().any(|e| e.contains("'bad' is missing required field 'model'")));
    }

    #[test]
    fn unknown_tooling_id_validation_rejects_defaults_and_state_references() {
        let mut settings = default_settings();
        settings.defaults.mcp_servers = Some(vec![StateMcpEntry::Id("missing-default".to_string())]);
        settings.defaults.skills = Some(vec![StateSkillEntry::Id("missing-skill".to_string())]);
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    mcp_servers: [missing-state]\n    skills: [missing-state-skill]\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );

        let errs = validate_machine_settings_references(&machine, &settings);

        assert!(errs
            .iter()
            .any(|e| e.contains("defaults.mcp_servers references unknown mcp server 'missing-default'")));
        assert!(errs
            .iter()
            .any(|e| e.contains("defaults.skills references unknown skill 'missing-skill'")));
        assert!(errs
            .iter()
            .any(|e| e.contains("state 'pending' mcp_servers references unknown mcp server 'missing-state'")));
        assert!(errs
            .iter()
            .any(|e| e.contains("state 'pending' skills references unknown skill 'missing-state-skill'")));
    }

    #[test]
    fn unknown_tooling_id_validation_keeps_known_optional_unavailable_runtime_status() {
        let mut settings = default_settings();
        settings.skills.insert(
            "known".to_string(),
            SkillProfile { path: "/definitely/not/present".to_string(), description: None },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    skills:\n      - id: known\n        optional: true\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );

        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(errs.is_empty(), "known optional unavailable tooling is a runtime status: {errs:?}");
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.skills.len(), 1);
        assert!(tooling.skills[0].definition.is_none());
        assert!(tooling.skills[0].optional);
    }

    #[test]
    fn validates_snapshot_operations_require_target_and_session_profile() {
        let settings = default_settings();
        let no_target = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&no_target, &settings);
        assert!(
            errs.iter().any(|e| e.contains("snapshot-requires-target")),
            "snapshot operations without an effective target must error: {errs:?}"
        );

        let no_layout = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    target: claude-code:anthropic:model\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&no_layout, &settings);
        assert!(
            errs.iter().any(|e| e.contains("unsupported-snapshot-session")),
            "named emit with no session layout must error: {errs:?}"
        );
        assert!(
            errs.iter().all(|e| !e.contains("remove the `snapshot:` block")),
            "an ordinary state has no supervision advice to give: {errs:?}"
        );
    }

    /// A supervising state is the one place the spec *recommends* `snapshot:`,
    /// so the error an author hits by following that recommendation onto an
    /// agent without session support has to say what to do instead.
    // §FS-rhei-supervision.1.1 §FS-rhei-supervision.6
    #[test]
    fn a_supervisor_told_to_drop_snapshot_is_told_what_it_keeps() {
        let settings = default_settings();
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  supervising:\n    description: x\n    execute_on: descendant-terminal\n    target: claude-code:anthropic:model\n    snapshot:\n      emit:\n        name: supervisor\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: supervising\n    to: supervising\n  - from: supervising\n    to: done\n    condition: openDescendants < 1\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        let hinted = errs
            .iter()
            .find(|e| e.contains("unsupported-snapshot-session"))
            .expect("claude-code has no snapshot session layout");
        assert!(
            hinted.contains("remove the `snapshot:` block")
                && hinted.contains("`## Checkpoints` and the briefs"),
            "the error owes the author the way forward: {hinted}"
        );
    }

    #[test]
    fn validates_required_snapshot_inherit_requires_preload_strategy() {
        let mut settings = default_settings();
        settings.agents.insert(
            "fake".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                session: Some(serde_json::json!({
                    "layout": { "kind": "FlatById", "ext": "jsonl" },
                    "resume": "none"
                })),
                ..Default::default()
            },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  source:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      emit:\n        name: build\n  pending:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      inherit:\n        name: build\n        required: true\n        select:\n          state: source\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: source\n    to: done\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("no supported snapshot preload strategy")),
            "required inherit with ResumeStrategy::None must error: {errs:?}"
        );
    }

    #[test]
    fn validates_snapshot_session_profiles_match_runtime_support() {
        let mut settings = default_settings();
        let emit_machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );

        settings.agents.insert(
            "fake".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                session: Some(serde_json::json!({
                    "session_dir_flag": "--session-dir",
                    "layout": { "kind": "UnknownLayout", "ext": "jsonl" }
                })),
                ..Default::default()
            },
        );
        let errs = validate_machine_settings_references(&emit_machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("unsupported-snapshot-session")),
            "unsupported layout kind must error: {errs:?}"
        );

        settings.agents.insert(
            "fake".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                session: Some(serde_json::json!({
                    "session_dir_flag": "--session-dir",
                    "layout": { "kind": "FlatById" }
                })),
                ..Default::default()
            },
        );
        let errs = validate_machine_settings_references(&emit_machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("unsupported-snapshot-session")),
            "incomplete layout must error: {errs:?}"
        );

        settings.agents.insert(
            "fake".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                session: Some(serde_json::json!({
                    "layout": { "kind": "FlatById", "ext": "jsonl" },
                    "resume": { "native": {} }
                })),
                ..Default::default()
            },
        );
        let inherit_machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  source:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      emit:\n        name: build\n  pending:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      inherit:\n        name: build\n        required: true\n        select:\n          state: source\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: source\n    to: done\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&inherit_machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("unsupported-snapshot-session")),
            "non-empty unsupported resume object must error: {errs:?}"
        );
    }

    // agent-grounds/rhei#125: a fixed-location session (dir_template, no
    // session_dir_flag) supports emit on its own — this is the ticket's exact
    // repro shape. §FS-rhei-snapshots.9.1
    #[test]
    fn validates_fixed_location_dir_template_supports_emit_without_session_dir_flag() {
        let mut settings = default_settings();
        settings.agents.insert(
            "fixed".to_string(),
            CustomAgentProfile {
                command: vec!["fixed".to_string()],
                session: Some(serde_json::json!({
                    "layout": {
                        "kind": "FlatById",
                        "dir_template": "~/.fixed-agent/sessions",
                        "ext": "jsonl"
                    }
                })),
                ..Default::default()
            },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  emit:\n    description: x\n    target: fixed:openai:model\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: emit\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.is_empty(),
            "a supported layout with dir_template and no session_dir_flag must validate: {errs:?}"
        );
    }

    // A layout with neither `session_dir_flag` nor `dir_template` has nowhere
    // for rhei to look, so it stays unsupported for emit. §FS-rhei-snapshots.9.1
    #[test]
    fn validates_layout_without_flag_or_dir_template_stays_unsupported_for_emit() {
        let mut settings = default_settings();
        settings.agents.insert(
            "fixed".to_string(),
            CustomAgentProfile {
                command: vec!["fixed".to_string()],
                session: Some(serde_json::json!({
                    "layout": { "kind": "FlatById", "ext": "jsonl" }
                })),
                ..Default::default()
            },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  emit:\n    description: x\n    target: fixed:openai:model\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: emit\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("unsupported-snapshot-session")),
            "a layout with neither flag nor dir_template must still fail: {errs:?}"
        );
    }

    #[test]
    fn validates_agent_mode_allowed_on_modeless_agent() {
        let mut settings = default_settings();
        settings.agents.insert(
            "noop".to_string(),
            CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    agent: noop\n    agent_mode: yolo\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.is_empty(),
            "agent_mode is permitted when the resolved agent declares no modes: {errs:?}"
        );
    }

    #[test]
    fn validates_target_selector_mode_must_exist_on_agent_profile() {
        let mut settings = default_settings();
        settings.agents.insert(
            "noop".to_string(),
            CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    target: noop[review]:openai:gpt\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| {
                e.contains("unknown target mode 'review'") && e.contains("noop[review]:openai:gpt")
            }),
            "target selector modes must be declared by the agent profile: {errs:?}"
        );
    }

    #[test]
    fn poll_attempt_condition_aliases_are_available_on_first_attempt() {
        let rhei = rhei_core::parse(
            "# Rhei: Poll\n\n## Tasks\n\n### Task 1: Wait\n**State:** wait\n\nWait.\n",
        )
        .expect("parse plan");
        let machine = machine_with_states(
            "name: poll\nversion: 1\nstates:\n  wait:\n    description: wait\n    program: \"true\"\n    poll:\n      interval: 1s\n      max_attempts: 1\n  done:\n    description: done\n    final: true\ntransitions:\n  - from: wait\n    to: wait\n  - from: wait\n    to: done\n    condition: pollAttempts >= pollMaxAttempts\n",
        );

        let to_state =
            find_next_transition(&rhei.tasks[0], &rhei, &machine).expect("transition eval");
        assert_eq!(to_state.as_deref(), Some("done"));
    }

    #[test]
    fn appends_mcp_flag_per_resolved_server() {
        let profile = built_in_agents().remove("codex").expect("codex");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
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
            mcp_servers: vec![
                ResolvedMcpEntry {
                    id: "linear".to_string(),
                    optional: false,
                    definition: Some(McpServerProfile::default()),
                },
                ResolvedMcpEntry {
                    id: "postgres".to_string(),
                    optional: false,
                    definition: Some(McpServerProfile::default()),
                },
            ],
            skills: Vec::new(),
        };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do",
            Path::new("/tmp"),
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
            command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let mcp_idxs: Vec<usize> =
            args.iter().enumerate().filter(|(_, a)| *a == "--mcp").map(|(i, _)| i).collect();
        assert_eq!(mcp_idxs.len(), 2, "one --mcp per resolved server: {args:?}");
        assert_eq!(args[mcp_idxs[0] + 1], "linear");
        assert_eq!(args[mcp_idxs[1] + 1], "postgres");
    }
