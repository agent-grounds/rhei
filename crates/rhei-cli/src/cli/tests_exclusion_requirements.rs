// §FS-rhei-plan-language.3.13 §FS-rhei-agents.5.2.1 §FS-rhei-agents.5.2.2

#[test]
fn exclusions_recheck_actual_invocation_before_composition() {
    let dir = memory_dir(&[("plan.rhei.md", "# Rhei: Actual invocation\n\n## Tasks\n\n### Task 1: Review\n**State:** review-2\n**Excludes:** artifact=runtime/model-b/2.md\n")]);
    let path = dir.path().join("plan.rhei.md");
    let loaded = load_plan(&path).unwrap();
    let task = &loaded.rhei.tasks[0];
    let machine = rhei_validator::StateMachine::from_yaml_str(
        r#"name: actual-invocation
version: 1
models: [model-a, model-b]
states:
  review:
    initial: true
    visits: 2
    agent: mock
    model: model-a
    inputs:
      - { name: brief, path: 'runtime/{model}/{visit_count}.md' }
  completed: { final: true }
transitions: [{ from: review, to: completed }]
"#,
    )
    .unwrap();
    let settings: RheiSettings = serde_json::from_value(serde_json::json!({
        "agents": {"mock": {"command": ["mock"]}},
        "models": {
            "model-a": {"provider": "mock", "model": "model-a"},
            "model-b": {"provider": "mock", "model": "model-b"}
        }
    }))
    .unwrap();
    let policy = loaded_task_exclusions(
        &loaded,
        task,
        dir.path(),
        dir.path(),
        &path,
        &machine,
        None,
        &settings,
        &default_run_options(),
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    let mut memory = prompt_memory(&loaded, &path, &dir.path().join("runtime"), BTreeSet::new());
    memory.exclusions = policy;
    let mut context = memory_context(dir.path(), &path, &loaded, &memory, &machine, task, "review");
    // The shared composition entry is used anew by serial/pool dispatch, every
    // fan-out identity, retries, and manual next's invocation guard.
    context.model = Some("model-a");
    assert!(compose_agent_prompt(&context).is_ok());
    context.model = Some("model-b");
    let error = compose_agent_prompt(&context).unwrap_err().to_string();
    assert!(error.contains("required input 'brief'"), "{error}");
    assert!(error.contains("visit 2"), "{error}");
    assert!(validate_invocation_exclusions(&context).is_err());
}

/// §FS-rhei-agents.1.1.2
#[test]
fn exclusions_adapter_preserves_recursion_for_logical_and_canonical_targets() {
    let dir = memory_dir(&[]);
    for recursive in [false, true] {
        let logical = dir.path().join("alias/private");
        let canonical = dir.path().join("real/private");
        let policy = ResolvedExclusions {
            entries: vec![ResolvedExclusion {
                authored: "checkout=alias/private".into(),
                logical: logical.clone(),
                canonical: canonical.clone(),
                recursive,
            }],
            ..Default::default()
        };
        let suffix = if recursive { "/" } else { "" };
        assert_eq!(
            policy.adapter_paths(),
            vec![
                format!("{}{suffix}", logical.display()),
                format!("{}{suffix}", canonical.display()),
            ]
        );
    }
}
