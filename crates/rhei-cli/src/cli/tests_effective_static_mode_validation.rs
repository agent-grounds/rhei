    fn static_mode_machine(state: &str) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(&format!(
            "name: static-mode\nversion: 1\nstates:\n{state}  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n"
        ))
        .expect("valid state machine")
    }

    fn assert_unknown_effective_mode(errors: &[String]) {
        assert!(
            errors.iter().any(|error| {
                error.contains("codex")
                    && error.contains("bogus")
                    && error.contains("agents.codex.modes")
            }),
            "the effective mode refusal must identify its value, agent, and registry key: {errors:?}"
        );
    }

    #[test]
    fn validate_effective_static_agent_mode_from_nested_default_is_declared() {
        let mut settings = default_settings();
        settings.defaults.agent_mode = Some("bogus".to_string());
        let machine = static_mode_machine(
            "  pending:\n    initial: true\n    description: x\n    agent: codex\n",
        );

        let errors = validate_machine_settings_references(&machine, &settings);

        assert_unknown_effective_mode(&errors);
    }

    #[test]
    fn validate_effective_static_agent_mode_from_legacy_default_is_declared() {
        let mut settings = default_settings();
        settings.agent = Some(AgentConfig::from("codex"));
        settings.agent_mode = Some("bogus".to_string());
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert_unknown_effective_mode(&errors);
    }

    #[test]
    fn validate_effective_static_agent_mode_against_model_default_agent() {
        let mut settings = default_settings();
        settings.defaults.model = Some("impl".to_string());
        settings.defaults.agent_mode = Some("bogus".to_string());
        settings.models.insert(
            "impl".to_string(),
            ModelProfile {
                provider: Some("openai".to_string()),
                model: Some("gpt".to_string()),
                default_agent: Some("codex".to_string()),
                agents: BTreeMap::new(),
            },
        );
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert_unknown_effective_mode(&errors);
    }

    #[test]
    fn validate_effective_static_agent_mode_accepts_declared_mode() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        settings.defaults.agent_mode = Some("yolo".to_string());
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert!(errors.is_empty(), "a declared effective mode must validate: {errors:?}");
    }

    #[test]
    fn validate_effective_static_agent_mode_prefers_nested_over_legacy_default() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        settings.defaults.agent_mode = Some("yolo".to_string());
        settings.agent_mode = Some("bogus".to_string());
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert!(errors.is_empty(), "the nested default must shadow the legacy value: {errors:?}");
    }

    #[test]
    fn validate_effective_static_agent_mode_does_not_speculate_without_agent() {
        let mut settings = default_settings();
        settings.defaults.agent_mode = Some("bogus".to_string());
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert!(errors.is_empty(), "a mode without an effective agent is not refused: {errors:?}");
    }

    #[test]
    fn validate_effective_static_agent_mode_preserves_state_and_selector_shadowing() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        settings.defaults.agent_mode = Some("bogus".to_string());
        let machine = rhei_validator::StateMachine::from_yaml_str(
            "name: static-mode\nversion: 1\nstates:\n  pending:\n    initial: true\n    description: x\n    agent: codex\n    agent_mode: yolo\n  targeted:\n    description: x\n    target: codex[yolo]:openai:gpt\n  parallel:\n    description: x\n    all_targets:\n      - codex[yolo]:openai:gpt\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: targeted\n  - from: targeted\n    to: parallel\n  - from: parallel\n    to: done\n",
        )
        .expect("valid state machine");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert!(errors.is_empty(), "explicit static selections must shadow defaults: {errors:?}");
    }

    #[test]
    fn validate_effective_static_agent_mode_allows_profiles_without_modes() {
        let mut settings = default_settings();
        settings.agents.insert(
            "noop".to_string(),
            CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
        );
        settings.defaults.agent = Some(AgentConfig::from("noop"));
        settings.defaults.agent_mode = Some("anything".to_string());
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = validate_machine_settings_references(&machine, &settings);

        assert!(errors.is_empty(), "a profile with no modes keeps no-mode behavior: {errors:?}");
    }

    /// A task target follows settings-derived autonomous work and replaces the
    /// invalid fallback tuple exactly as execution will use it.
    /// §FS-rhei-plan-language.3.11 §FS-rhei-agents.1.4.1
    #[test]
    fn task_target_resolves_over_settings_derived_agent_mode() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        settings.defaults.agent_mode = Some("bogus".to_string());
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");
        let rhei = rhei_core::parse(
            "# Rhei: Target override\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Target:** codex[yolo]:openai:gpt-5.6-luna\n",
        )
        .expect("valid plan");

        let resolved = resolve_agent_invocations_for_task(
            &machine,
            "pending",
            &settings,
            &default_run_options(),
            rhei.tasks.first(),
        )
        .expect("the task target must bypass the invalid settings mode");

        assert_eq!(resolved.len(), 1);
        let resolved = &resolved[0];
        assert_eq!(resolved.agent.id(), "codex");
        assert_eq!(resolved.mode.as_deref(), Some("yolo"));
        assert_eq!(resolved.model_provider.as_deref(), Some("openai"));
        assert_eq!(resolved.model_name.as_deref(), Some("gpt-5.6-luna"));
        assert_eq!(
            resolved.target.as_ref().map(ExecutionTarget::selector).as_deref(),
            Some("codex[yolo]:openai:gpt-5.6-luna")
        );
    }

    fn settings_with_distinct_model_identities() -> RheiSettings {
        let mut settings = default_settings();
        settings.defaults.model = Some("baseline".to_string());
        settings.defaults.agent_mode = Some("yolo".to_string());
        settings.models.insert(
            "baseline".to_string(),
            ModelProfile {
                provider: Some("openai".to_string()),
                model: Some("baseline-model".to_string()),
                default_agent: Some("codex".to_string()),
                agents: BTreeMap::new(),
            },
        );
        let mut alternate_agents = BTreeMap::new();
        alternate_agents.insert(
            "codex".to_string(),
            ModelAgentBinding {
                autonomous_args: vec!["--alternate".to_string()],
                timeout: Some("17m".to_string()),
                ..Default::default()
            },
        );
        settings.models.insert(
            "alternate".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("alternate-model".to_string()),
                default_agent: Some("claude-code".to_string()),
                agents: alternate_agents,
            },
        );
        settings
    }

    fn settings_model_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            "name: settings-model\nversion: 1\nmodels: [baseline, alternate]\nstates:\n  pending:\n    initial: true\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        )
        .expect("valid state machine")
    }

    /// A task model changes only the model dimension of settings-derived work,
    /// even when that profile prefers a different agent and provider.
    /// §FS-rhei-plan-language.3.11
    #[test]
    fn task_model_preserves_settings_derived_agent_mode_and_provider() {
        let settings = settings_with_distinct_model_identities();
        let machine = settings_model_machine();
        let rhei = rhei_core::parse(
            "# Rhei: Model override\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Model:** alternate\n",
        )
        .expect("valid plan");
        let opts = default_run_options();

        let control = resolve_agent_invocations_for_task(
            &machine, "pending", &settings, &opts, None,
        )
        .expect("settings-derived control identity resolves");
        let overridden = resolve_agent_invocations_for_task(
            &machine, "pending", &settings, &opts, rhei.tasks.first(),
        )
        .expect("task model resolves");

        assert_eq!(control.len(), 1);
        assert_eq!(overridden.len(), 1);
        let control = &control[0];
        let overridden = &overridden[0];
        assert_eq!(control.agent.id(), "codex");
        assert_eq!(control.mode.as_deref(), Some("yolo"));
        assert_eq!(control.model_provider.as_deref(), Some("openai"));
        assert_eq!(control.model.as_deref(), Some("baseline"));
        assert_eq!(control.model_name.as_deref(), Some("baseline-model"));
        assert_eq!(overridden.agent.id(), control.agent.id());
        assert_eq!(overridden.mode, control.mode);
        assert_eq!(overridden.model_provider, control.model_provider);
        assert_eq!(overridden.model.as_deref(), Some("alternate"));
        assert_eq!(overridden.model_name.as_deref(), Some("alternate-model"));
        assert_eq!(overridden.timeout_secs, Some(17 * 60));
        assert_eq!(overridden.autonomous_args, ["--alternate"]);
    }

    /// A task model cannot use its profile's default agent to hide an invalid
    /// mode on the normally resolved settings-derived identity.
    /// §FS-rhei-plan-language.3.11 §FS-rhei-agents.1.4.1
    #[test]
    fn task_model_does_not_shadow_invalid_settings_agent_mode() {
        let mut settings = settings_with_distinct_model_identities();
        settings.defaults.agent_mode = Some("bogus".to_string());
        let machine = settings_model_machine();
        let rhei = rhei_core::parse(
            "# Rhei: Model override\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Model:** alternate\n",
        )
        .expect("valid plan");

        let errors = validate_machine_settings_references(&machine, &settings);
        assert_unknown_effective_mode(&errors);
        let Err(error) = resolve_agent_invocations_for_task(
            &machine,
            "pending",
            &settings,
            &default_run_options(),
            rhei.tasks.first(),
        ) else {
            panic!("the task model must not replace the identity before mode validation");
        };
        assert!(error.to_string().contains("agent 'codex' has no mode 'bogus'"), "got: {error}");
    }

    /// Settings may make an ordinary state autonomous, but a task override
    /// cannot create agent work in a human, program, terminal, or unselected state.
    /// §FS-rhei-plan-language.3.11
    #[test]
    fn task_override_agent_mode_applicability_preserves_non_agent_boundaries() {
        let machine = static_mode_machine("  pending:\n    initial: true\n    description: x\n");
        let state = machine.states.get("pending").expect("pending state");
        let mut settings = default_settings();
        let opts = default_run_options();

        assert!(!task_execution_override_applies_to_state(state, &settings, &opts));
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        assert!(task_execution_override_applies_to_state(state, &settings, &opts));

        let mut non_agent = state.clone();
        non_agent.gating = true;
        assert!(!task_execution_override_applies_to_state(&non_agent, &settings, &opts));
        non_agent.gating = false;
        non_agent.program = Some(serde_yaml::Value::String("true".to_string()));
        assert!(!task_execution_override_applies_to_state(&non_agent, &settings, &opts));
        non_agent.program = None;
        non_agent.terminal = true;
        assert!(!task_execution_override_applies_to_state(&non_agent, &settings, &opts));
    }
