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
