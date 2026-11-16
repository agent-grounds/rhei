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
    #[ignore = "red until #727: an owed supervisor contributes its provider deadline; implement removes this"]
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
}
