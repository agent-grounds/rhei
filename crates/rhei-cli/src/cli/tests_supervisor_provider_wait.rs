// A supervisor's clock can release an owed visit despite open descendants;
// independent subtree, phase, claim and prior blockers still exclude it.
// §FS-rhei-run.5.1 §FS-rhei-supervision.3.2

mod supervisor_provider_wait {
    use super::provider_limits::deadline;
    use super::*;

    const MACHINE: &str = r#"name: supervisor-deadline
version: 1
states:
  supervising:
    initial: true
    execute_on: child-terminal
    target: codex:openai:alpha
    visits: 6
  work:
    target: pi:other:beta
  ordinary:
    target: codex:openai:alpha
  gate:
    gating: true
  done:
    final: true
transitions:
  - { from: supervising, to: supervising }
  - { from: supervising, to: done, condition: openDescendants < 1 }
  - { from: work, to: done }
  - { from: ordinary, to: done }
  - { from: gate, to: done }
"#;

    fn plan(state: &str, own: &str, child: &str, phase: Option<&str>) -> rhei_core::ast::Rhei {
        let mut rhei = rhei_core::parse(&format!(
            "# Rhei: Supervisor deadline\n\n## Tasks\n\n\
             ### Task 1: Parent\n**State:** {state}\n{own}\n\
             #### Task 1.1: Open child\n**State:** work\n{child}\n\n\
             ### Task 2: Gate\n**State:** gate\n"
        ))
        .expect("plan");
        let now = current_unix_secs();
        let limit = ProviderLimit {
            identity: ProviderIdentity { agent: "codex".into(), provider: "openai".into() },
            kind: ProviderWaitKind::Limit,
            signal: "recognized refusal".into(),
            observed_at: deadline(now),
            next_attempt_at: deadline(now + 7200),
        };
        let (metadata, _) = set_provider_limit_metadata(None, &parse_task_id("1"), state, &limit);
        rhei.metadata = Some(metadata);
        if let Some(phase) = phase {
            rhei.metadata = Some(match phase {
                "released" => record_supervision_release(rhei.metadata.as_ref(), &parse_task_id("1")),
                "held" => record_supervision_hold(rhei.metadata.as_ref(), &parse_task_id("1"), None),
                _ => panic!("unknown fixture phase"),
            });
        }
        rhei
    }

    fn next_attempt(rhei: &rhei_core::ast::Rhei) -> Option<u64> {
        let machines = rhei_validator::MachineSet::single(
            rhei_validator::StateMachine::from_yaml_str(MACHINE).expect("machine"),
        );
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        let dir = tempfile::tempdir().expect("scratch");
        let runtime = dir.path().join("runtime");
        let spawns = SpawnSiteRoots {
            settings: Some(&settings),
            ..SpawnSiteRoots::plan_only(dir.path(), &runtime)
        };
        earliest_pending_agent_deadline(rhei, &machines, &settings, &None, &spawns)
    }

    #[test]
    fn supervisor_provider_wait_owed_visit_contributes_deadline() {
        let rhei = plan("supervising", "", "", None);
        let limit = provider_limit_for_task_state(rhei.metadata.as_ref(), &parse_task_id("1"), "supervising")
            .expect("persisted provider wait");
        assert_eq!(next_attempt(&rhei), limit.deadline_epoch(), "open children do not block an owed visit");
    }

    /// These controls already pass before #727. An exemption for every parent
    /// or every supervising state must not turn them into timed retries.
    #[test]
    fn supervisor_provider_wait_independent_blockers_contribute_no_deadline() {
        for (name, state, own, child, phase) in [
            ("ordinary parent", "ordinary", "", "", None),
            ("released supervisor", "supervising", "", "", Some("released")),
            ("draining supervisor", "supervising", "", "**Assignee:** alice", Some("held")),
            ("claimed supervisor", "supervising", "**Assignee:** alice", "", None),
            ("unsatisfied prior", "supervising", "**Prior:** Task 2", "", None),
            ("gated parent", "gate", "", "", None),
        ] {
            let rhei = plan(state, own, child, phase);
            assert_eq!(next_attempt(&rhei), None, "{name}: expiry alone cannot admit this visit");
        }
    }

    /// Missing inputs exclude this supervisor's clock, using its member root
    /// and the actual model override rather than the project's defaults. §FS-rhei-run.5.1
    #[test]
    fn supervisor_provider_wait_requires_input_under_owning_root_and_run_options() {
        let rhei = plan("supervising", "", "", None);
        let machine = MACHINE.replace(
            "    visits: 6\n",
            "    visits: 6\n    inputs:\n      - name: brief\n        path: runtime/{model.name}/brief.md\n",
        );
        let machines = rhei_validator::MachineSet::single(
            rhei_validator::StateMachine::from_yaml_str(&machine).expect("machine with input"),
        );
        let dir = tempfile::tempdir().expect("scratch");
        let settings_dir = dir.path().join(".agent-grounds/rhei");
        fs::create_dir_all(&settings_dir).unwrap();
        fs::write(
            settings_dir.join("settings.json"),
            r#"{"models":{"override":{"provider":"openai","model":"override"}}}"#,
        ).unwrap();
        let settings = load_merged_settings(dir.path()).expect("fixture model registry");
        let member = dir.path().join("member");
        let runtime = dir.path().join("runtime");
        fs::create_dir_all(runtime.join("override")).unwrap();
        fs::create_dir_all(member.join("runtime/alpha")).unwrap();
        fs::create_dir_all(member.join("runtime/override")).unwrap();
        let roots = HashMap::from([("1".into(), member.clone())]);
        let mut opts = default_run_options();
        opts.agent.model = Some("override".into());
        resolve_agent_invocations_for_task(
            machines.for_task(&parse_task_id("1")), "supervising", &settings, &opts, Some(&rhei.tasks[0]),
        ).expect("the fixture's registered model override resolves");
        let spawns = SpawnSiteRoots {
            tasks: ReadySetRoots { workspace_root: dir.path(), task_roots: &roots },
            settings: Some(&settings),
            opts: &opts,
            ..SpawnSiteRoots::plan_only(dir.path(), &runtime)
        };
        let next = || earliest_pending_agent_deadline(&rhei, &machines, &settings, &None, &spawns);
        assert_eq!(next(), None, "a missing input cannot be supplied by a deadline");
        fs::write(runtime.join("override/brief.md"), "wrong root").unwrap();
        fs::write(member.join("runtime/alpha/brief.md"), "wrong model").unwrap();
        assert_eq!(next(), None, "neither the project root nor the default model supplies the input");
        assert!(!remaining_work_is_only_gating_or_poll_blocked(&rhei, &machines, &None, &spawns));
        fs::write(member.join("runtime/override/brief.md"), "required input").unwrap();
        let limit = provider_limit_for_task_state(rhei.metadata.as_ref(), &parse_task_id("1"), "supervising")
            .expect("persisted provider wait");
        assert_eq!(next(), limit.deadline_epoch(), "the owning root and override admit the wait");
    }
}
