fn cli_target_settings() -> RheiSettings {
    serde_json::from_str(
        r#"{
          "agents": {
            "state-agent": {
              "command": ["state-agent"],
              "modes": { "yolo": [] },
              "effort": {
                "values": { "low": "state-low" },
                "args": ["--state-effort", "{value}"]
              }
            },
            "task-agent": {
              "command": ["task-agent"],
              "modes": { "safe": [] }
            },
            "override-agent": {
              "command": ["override-agent"],
              "modes": { "yolo": [], "safe": [] },
              "effort": {
                "values": { "low": "override-low" },
                "args": ["--override-effort", "{value}"]
              }
            }
          },
          "models": {
            "state-model": { "provider": "registry", "model": "state-concrete" },
            "task-model": { "provider": "registry", "model": "task-concrete" },
            "override-model": { "provider": "registry", "model": "override-concrete" },
            "other-model": { "provider": "registry", "model": "other-concrete" }
          }
        }"#,
    )
    .expect("target override settings parse")
}

fn cli_target_machine(state_fields: &str) -> rhei_validator::StateMachine {
    rhei_validator::StateMachine::from_yaml_str(&format!(
        "name: cli-target-overrides\nversion: 1\nmodels: [state-model, task-model, override-model, other-model]\nstates:\n  work:\n    initial: true\n{state_fields}  done:\n    final: true\ntransitions:\n  - from: work\n    to: done\n"
    ))
    .expect("target override machine parses")
}

fn cli_target_options(agent: Option<&str>, model: Option<&str>) -> RunOptions {
    let mut opts = default_run_options();
    opts.agent.agent = agent.map(str::to_string);
    opts.agent.model = model.map(str::to_string);
    opts
}

fn resolved_selector(resolved: &ResolvedAgent) -> Option<String> {
    resolved.target.as_ref().map(ExecutionTarget::selector)
}

/// The two run flags are independent dimensions over an ordinary selector.
/// §FS-rhei-run.2.2 §FS-rhei-agents.1.4 §FS-rhei-agents.1.5
#[test]
fn target_cli_override_dimensions_compose_over_an_explicit_state_target() {
    let settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n",
    );
    let cases = [
        (
            None,
            None,
            "state-agent[yolo]:state-provider:state-model",
        ),
        (
            Some("override-agent"),
            None,
            "override-agent[yolo]:state-provider:state-model",
        ),
        (
            None,
            Some("override-model"),
            "state-agent[yolo]:state-provider:override-model",
        ),
        (
            Some("override-agent"),
            Some("override-model"),
            "override-agent[yolo]:state-provider:override-model",
        ),
    ];
    let mut mismatches = Vec::new();

    for (agent, model, expected) in cases {
        let resolved = resolve_agent_invocations(
            &machine,
            "work",
            &settings,
            &cli_target_options(agent, model),
        )
        .expect("valid override resolves");
        let actual = resolved.first().and_then(resolved_selector);
        if actual.as_deref() != Some(expected) {
            mismatches.push(format!(
                "agent={agent:?} model={model:?}: expected {expected}, got {actual:?}"
            ));
        }
    }

    assert!(mismatches.is_empty(), "wrong composed targets:\n{}", mismatches.join("\n"));
}

/// CLI dimensions are applied after either kind of task identity override.
/// §FS-rhei-plan-language.3.11 §FS-rhei-agents.1.5
#[test]
fn target_cli_override_dimensions_take_precedence_over_task_target_and_model() {
    let settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n",
    );
    let plan = rhei_core::parse(
        "# Rhei: CLI over task identity\n\n## Tasks\n\n### Task target: Target\n**State:** work\n**Target:** task-agent[safe]:task-provider:task-model\n\n### Task model: Model\n**State:** work\n**Model:** task-model\n",
    )
    .expect("task override plan parses");
    let opts = cli_target_options(Some("override-agent"), Some("override-model"));
    let expected = [
        "override-agent[safe]:task-provider:override-model",
        "override-agent[yolo]:state-provider:override-model",
    ];
    let mut mismatches = Vec::new();

    for (task, expected) in plan.tasks.iter().zip(expected) {
        let resolved = resolve_agent_invocations_for_task(
            &machine,
            "work",
            &settings,
            &opts,
            Some(task),
        )
        .expect("task identity with CLI dimensions resolves");
        let actual = resolved.first().and_then(resolved_selector);
        if actual.as_deref() != Some(expected) {
            mismatches.push(format!("task {}: expected {expected}, got {actual:?}", task.id));
        }
    }
    assert!(mismatches.is_empty(), "wrong task targets:\n{}", mismatches.join("\n"));
}

/// Substitution preserves an absent mode rather than choosing a new default.
/// §FS-rhei-agents.1.5
#[test]
fn target_cli_override_agent_preserves_an_omitted_target_mode() {
    let settings = cli_target_settings();
    let machine = cli_target_machine("    target: state-agent:state-provider:state-model\n");

    let resolved = resolve_agent_invocations(
        &machine,
        "work",
        &settings,
        &cli_target_options(Some("override-agent"), None),
    )
    .expect("mode-less target resolves");

    assert_eq!(
        resolved.first().and_then(resolved_selector).as_deref(),
        Some("override-agent:state-provider:state-model")
    );
    assert_eq!(resolved[0].mode, None);
}

/// The effective agent owns effort mapping after identity composition.
/// §FS-rhei-agents.1.4 §FS-rhei-agents.1.4.1
#[test]
fn target_cli_override_agent_maps_effort_on_the_effective_agent() {
    let settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n    effort: low\n",
    );
    let opts = cli_target_options(Some("override-agent"), None);

    let resolved = resolve_agent_invocations(&machine, "work", &settings, &opts)
        .expect("compatible effective agent resolves");
    assert_eq!(resolved[0].autonomous_args, ["--override-effort", "override-low"]);
}

/// A preserved mode is checked against the effective, substituted agent.
/// §FS-rhei-agents.1.4
#[test]
fn target_cli_override_agent_rejects_an_incompatible_preserved_mode() {
    let mut settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n",
    );
    let opts = cli_target_options(Some("override-agent"), None);

    settings
        .agents
        .get_mut("override-agent")
        .expect("override agent")
        .modes
        .shift_remove("yolo");
    let Err(error) = resolve_agent_invocations(&machine, "work", &settings, &opts) else {
        panic!("the preserved mode must be checked against the effective agent");
    };
    assert!(
        error.to_string().contains("override-agent") && error.to_string().contains("yolo"),
        "diagnostic must name the effective agent and preserved mode: {error}"
    );
}

/// A dynamic CLI agent name must be checked before it can reach a spawn.
/// §FS-rhei-agents.1.4
#[test]
fn target_cli_override_rejects_an_unknown_agent() {
    let settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n",
    );
    let Err(error) = resolve_agent_invocations(
        &machine,
        "work",
        &settings,
        &cli_target_options(Some("missing-agent"), None),
    ) else {
        panic!("unknown run-level agent must be refused");
    };
    assert!(error.to_string().contains("missing-agent"), "wrong diagnostic: {error}");
}

/// A dynamic CLI model name must be checked before it can reach a spawn.
/// §FS-rhei-agents.1.4
#[test]
fn target_cli_override_rejects_an_unknown_model() {
    let settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n",
    );
    let Err(error) = resolve_agent_invocations(
        &machine,
        "work",
        &settings,
        &cli_target_options(None, Some("missing-model")),
    ) else {
        panic!("unknown run-level model must be refused");
    };
    assert!(error.to_string().contains("missing-model"), "wrong diagnostic: {error}");
}

/// A state lock restricts task metadata, not operator run flags.
/// §FS-rhei-plan-language.3.11
#[test]
fn target_cli_override_remains_effective_on_a_locked_state() {
    let settings = cli_target_settings();
    let machine = cli_target_machine(
        "    target: state-agent[yolo]:state-provider:state-model\n    target_locked: true\n",
    );
    let resolved = resolve_agent_invocations(
        &machine,
        "work",
        &settings,
        &cli_target_options(Some("override-agent"), Some("override-model")),
    )
    .expect("run flags are valid on a locked state");

    assert_eq!(
        resolved.first().and_then(resolved_selector).as_deref(),
        Some("override-agent[yolo]:state-provider:override-model")
    );
}

/// Selector fanout remains a complete authored identity; legacy model fanout
/// still accepts the CLI agent while retaining each declared model.
/// §FS-rhei-agents.1.4
#[test]
fn target_cli_override_preserves_all_targets_and_legacy_all_models_boundaries() {
    let settings = cli_target_settings();
    let opts = cli_target_options(Some("override-agent"), Some("override-model"));
    let targets = cli_target_machine(
        "    all_targets:\n      - state-agent[yolo]:state-provider:state-model\n      - task-agent[safe]:task-provider:task-model\n",
    );
    let resolved_targets = resolve_agent_invocations(&targets, "work", &settings, &opts)
        .expect("all_targets resolves");
    assert_eq!(
        resolved_targets.iter().filter_map(resolved_selector).collect::<Vec<_>>(),
        [
            "state-agent[yolo]:state-provider:state-model",
            "task-agent[safe]:task-provider:task-model",
        ]
    );

    let models = cli_target_machine(
        "    all_models: [state-model, other-model]\n    agent: state-agent\n    agent_mode: yolo\n",
    );
    let resolved_models = resolve_agent_invocations(&models, "work", &settings, &opts)
        .expect("all_models resolves");
    assert_eq!(
        resolved_models
            .iter()
            .map(|resolved| (resolved.agent.id(), resolved.model.as_deref()))
            .collect::<Vec<_>>(),
        [("override-agent", Some("state-model")), ("override-agent", Some("other-model"))]
    );
}
