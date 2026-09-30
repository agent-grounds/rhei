    /// One machine per case, so a test says exactly which states it declares.
    /// The trailing terminal state keeps every machine well-formed, and each
    /// non-final state the caller declared is given its way on to that state,
    /// so a test spells out only what it is actually about.
    fn timeout_machine(states: &str) -> rhei_validator::StateMachine {
        let mut transitions = String::new();
        let mut open: Option<(&str, bool)> = None;
        let close = |open: Option<(&str, bool)>, transitions: &mut String| {
            if let Some((name, terminal)) = open {
                if !terminal {
                    transitions.push_str(&format!("  - from: {name}\n    to: done\n"));
                }
            }
        };
        for line in states.lines() {
            match line.strip_prefix("  ") {
                Some(rest) if !rest.starts_with(' ') && rest.ends_with(':') => {
                    close(open.take(), &mut transitions);
                    open = Some((rest.trim_end_matches(':'), false));
                }
                _ if line.trim() == "final: true" => {
                    open = open.map(|(name, _)| (name, true));
                }
                _ => {}
            }
        }
        close(open, &mut transitions);

        rhei_validator::StateMachine::from_yaml_str(&format!(
            "name: orchestrator-timeout\nversion: 1\nstates:\n{states}  done:\n    description: terminal\n    final: true\ntransitions:\n{transitions}"
        ))
        .expect("valid state machine")
    }

    /// A single agent state bound by settings alone, so each test varies only
    /// the level of the chain it is about.
    fn pending_agent_state() -> &'static str {
        "  pending:\n    initial: true\n    description: x\n    agent: codex\n"
    }

    fn plan_without_tasks() -> rhei_core::ast::Rhei {
        rhei_core::parse("# Rhei: Timeouts\n\n## Tasks\n").expect("valid plan")
    }

    fn refusals(
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        settings: &RheiSettings,
    ) -> Vec<String> {
        validate_orchestrator_timeouts(
            rhei,
            &rhei_validator::MachineSet::single(machine.clone()),
            settings,
            &default_run_options(),
        )
    }

    fn assert_refuses(errors: &[String], state: &str, agent: &str) {
        assert_eq!(errors.len(), 1, "exactly one refusal was expected: {errors:?}");
        let error = &errors[0];
        for expected in ["agent_timeout", state, agent] {
            assert!(
                error.contains(expected),
                "the refusal must name {expected:?} so it can be acted on: {error}"
            );
        }
    }

    /// Nothing in the four-level chain resolves, so the state is refused by
    /// name, and by the name of the agent that resolved no timeout.
    /// §FS-rhei-validate.4 §FS-rhei-agents.3.2.2
    #[test]
    fn validate_orchestrator_timeouts_refuses_an_unbounded_agent_state() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        let machine = timeout_machine(pending_agent_state());

        let errors = refusals(&plan_without_tasks(), &machine, &settings);

        assert_refuses(&errors, "pending", "codex");
        assert!(
            errors[0].contains("defaults.agent_timeout"),
            "the refusal carries the four places the value may go: {}",
            errors[0]
        );
    }

    /// Each of the four levels satisfies the rule on its own: the state's own
    /// `agent_timeout`, the model-agent binding's, the agent profile's, and
    /// `defaults.agent_timeout`. §FS-rhei-agents.7.1
    #[test]
    fn validate_orchestrator_timeouts_accepts_a_timeout_at_any_chain_level() {
        let state_level = timeout_machine(
            "  pending:\n    initial: true\n    description: x\n    agent: codex\n    agent_timeout: 30m\n",
        );
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        assert!(
            refusals(&plan_without_tasks(), &state_level, &settings).is_empty(),
            "a state-level timeout satisfies the rule"
        );

        let machine = timeout_machine(pending_agent_state());

        let mut profile_level = default_settings();
        profile_level.defaults.agent = Some(AgentConfig::from("codex"));
        profile_level
            .agents
            .get_mut("codex")
            .expect("built-in codex profile")
            .timeout = Some("30m".to_string());
        assert!(
            refusals(&plan_without_tasks(), &machine, &profile_level).is_empty(),
            "an agent-profile timeout satisfies the rule"
        );

        let mut defaults_level = default_settings();
        defaults_level.defaults.agent = Some(AgentConfig::from("codex"));
        defaults_level.defaults.agent_timeout = Some("30m".to_string());
        assert!(
            refusals(&plan_without_tasks(), &machine, &defaults_level).is_empty(),
            "a defaults timeout satisfies the rule"
        );

        let mut binding_level = default_settings();
        binding_level.defaults.model = Some("impl".to_string());
        let mut bound = BTreeMap::new();
        bound.insert(
            "codex".to_string(),
            ModelAgentBinding { timeout: Some("30m".to_string()), ..Default::default() },
        );
        binding_level.models.insert(
            "impl".to_string(),
            ModelProfile {
                provider: Some("openai".to_string()),
                model: Some("gpt".to_string()),
                default_agent: Some("codex".to_string()),
                agents: bound,
                prices: None,
            },
        );
        let bound_machine =
            timeout_machine("  pending:\n    initial: true\n    description: x\n");
        assert!(
            refusals(&plan_without_tasks(), &bound_machine, &binding_level).is_empty(),
            "a model-agent binding timeout satisfies the rule"
        );
    }

    /// The rule ranges over states that resolve to an agent invocation. A
    /// gating state, a final state and a program state spawn no agent, so an
    /// unbounded one of each is not this error — and `program_timeout` is
    /// checked nowhere, which this pins deliberately.
    /// §FS-rhei-agents.3.2.2 §FS-rhei-state-machine-writer.4
    #[test]
    fn validate_orchestrator_timeouts_ignores_states_that_spawn_no_agent() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        let machine = timeout_machine(
            "  pending:\n    initial: true\n    description: x\n    gating: true\n  \
             collect:\n    description: x\n    program: scripts/collect.sh\n  \
             stopped:\n    description: x\n    final: true\n",
        );

        let errors = refusals(&plan_without_tasks(), &machine, &settings);

        assert!(errors.is_empty(), "no agent is spawned by any of these states: {errors:?}");
    }

    /// A state that resolves no agent at all earns whatever diagnostic its own
    /// binding earns; this check must not pre-empt it with a timeout refusal.
    /// §FS-rhei-agents.3.2.2
    #[test]
    fn validate_orchestrator_timeouts_is_silent_where_no_agent_resolves() {
        let settings = default_settings();
        let machine = timeout_machine("  pending:\n    initial: true\n    description: x\n");

        let errors = refusals(&plan_without_tasks(), &machine, &settings);

        assert!(errors.is_empty(), "a state with no effective agent is not refused: {errors:?}");
    }

    /// A fan-out is judged member by member: the member that resolves a
    /// timeout is accepted and the one that does not is named alone.
    /// §FS-rhei-agents.3.2.2
    #[test]
    fn validate_orchestrator_timeouts_names_only_the_unbounded_fanout_member() {
        let mut settings = default_settings();
        settings
            .agents
            .get_mut("claude-code")
            .expect("built-in claude-code profile")
            .timeout = Some("30m".to_string());
        let machine = timeout_machine(
            "  pending:\n    initial: true\n    description: x\n    all_targets:\n      \
             - claude-code[yolo]:anthropic:claude-opus-5\n      - codex[yolo]:openai:gpt-5.5\n",
        );

        let errors = refusals(&plan_without_tasks(), &machine, &settings);

        assert_refuses(&errors, "pending", "codex");
    }

    /// Two offending states are two refusals in one report, so a single
    /// invocation says everything that must change. §FS-rhei-validate.4
    #[test]
    fn validate_orchestrator_timeouts_reports_every_offending_state() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        let machine = timeout_machine(
            "  pending:\n    initial: true\n    description: x\n    agent: codex\n  \
             review:\n    description: x\n    agent: codex\n",
        );

        let errors = refusals(&plan_without_tasks(), &machine, &settings);

        assert_eq!(errors.len(), 2, "each offending state is named: {errors:?}");
        assert!(errors.iter().any(|error| error.contains("'pending'")), "{errors:?}");
        assert!(errors.iter().any(|error| error.contains("'review'")), "{errors:?}");
    }

    /// The check resolves per task identity, so a `**Target:**` override is
    /// judged as it would be spawned: one that introduces a timeout clears the
    /// refusal, and one that removes it raises it.
    /// §FS-rhei-plan-language.3.11 §FS-rhei-agents.3.2.2
    #[test]
    fn validate_orchestrator_timeouts_follows_a_task_target_override() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        settings
            .agents
            .get_mut("claude-code")
            .expect("built-in claude-code profile")
            .timeout = Some("30m".to_string());
        let machine = timeout_machine(pending_agent_state());

        let bounded = rhei_core::parse(
            "# Rhei: Timeouts\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Target:** claude-code[yolo]:anthropic:claude-opus-5\n",
        )
        .expect("valid plan");
        assert!(
            refusals(&bounded, &machine, &settings).is_empty(),
            "an override onto a bounded agent clears the refusal"
        );

        let unbounded = rhei_core::parse(
            "# Rhei: Timeouts\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Target:** codex[yolo]:openai:gpt-5.5\n",
        )
        .expect("valid plan");

        assert_refuses(&refusals(&unbounded, &machine, &settings), "pending", "codex");
    }

    /// `rhei run --no-agent` resolves no agent invocation, so it resolves
    /// nothing to reject. §FS-rhei-validate.4
    #[test]
    fn validate_orchestrator_timeouts_exempts_no_agent_runs() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        let machine = timeout_machine(pending_agent_state());
        let mut opts = default_run_options();
        opts.agent.no_agent = true;

        let errors = validate_orchestrator_timeouts(
            &plan_without_tasks(),
            &rhei_validator::MachineSet::single(machine),
            &settings,
            &opts,
        );

        assert!(errors.is_empty(), "--no-agent spawns nothing, so it refuses nothing: {errors:?}");
    }

    /// A binding that does not resolve at all belongs to the check that owns it
    /// — an undeclared agent here — and the run refuses it at resolution,
    /// before any spawn. So this check steps over it rather than restating it
    /// as a missing timeout, which would replace a diagnostic the author can
    /// act on with one they cannot. The state is otherwise exactly the refused
    /// one: nothing but the unresolved agent keeps it out of the set.
    /// §FS-rhei-agents.3.2.2
    #[test]
    fn validate_orchestrator_timeouts_leaves_an_unresolvable_binding_to_its_own_check() {
        let mut settings = default_settings();
        settings.defaults.agent = Some(AgentConfig::from("codex"));
        assert!(
            !settings.agents.contains_key("ghost"),
            "the fixture rests on `ghost` being undeclared"
        );
        let machine = timeout_machine(
            "  pending:\n    initial: true\n    description: x\n    \
             target: ghost:anthropic:claude-opus-5\n",
        );
        assert!(
            resolve_agent_invocations_for_task(
                &machine,
                "pending",
                &settings,
                &default_run_options(),
                None,
            )
            .is_err(),
            "the fixture must exercise the resolution-failure path, not the empty one"
        );

        let errors = refusals(&plan_without_tasks(), &machine, &settings);

        assert!(
            errors.is_empty(),
            "an unresolvable binding is not restated as a missing timeout: {errors:?}"
        );
    }
