    fn snapshot_machine(state_body: &str) -> String {
        format!(
            r#"
name: snapshot-test
version: 1.0
states:
  source:
    description: Source
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: build
  pending:
    description: Pending
    target: claude-code:anthropic:model
{state_body}
  done:
    description: Done
    final: true
transitions:
  - from: source
    to: pending
  - from: pending
    to: done
"#
        )
    }

    #[test]
    fn rejects_snapshot_inherit_unsupported_compat() {
        let yaml = snapshot_machine(
            r#"    snapshot:
      inherit:
        name: build
        compat: replay
"#,
        );

        let err = StateMachine::from_yaml_str(&yaml).expect_err("unsupported compat");

        assert!(err.to_string().contains("snapshot.inherit.compat"));
    }

    #[test]
    fn rejects_snapshot_inherit_select_target_all() {
        let yaml = snapshot_machine(
            r#"    snapshot:
      inherit:
        name: build
        select:
          state: source
          target: all
"#,
        );

        let err = StateMachine::from_yaml_str(&yaml).expect_err("select.target all");

        assert!(err.to_string().contains("snapshot.inherit.select.target 'all'"));
    }

    #[test]
    fn rejects_snapshot_unknown_keys_in_closed_objects() {
        let yaml = snapshot_machine(
            r#"    snapshot:
      inherit:
        name: build
        unexpected: true
"#,
        );

        let err = StateMachine::from_yaml_str(&yaml).expect_err("unknown snapshot key");

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn target_slug_preserves_dot_underscore_and_detects_fanout_collisions() {
        let target =
            parse_execution_target("pi:openai:org_name/gpt.4o").expect("target should parse");
        assert_eq!(target.slug(), "pi-openai-org_name-gpt.4o");

        let yaml = r#"
name: collision
version: 1
states:
  pending:
    description: pending
    all_targets:
      - pi:openai:gpt/4o
      - pi:openai:gpt-4o
  done:
    description: done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("slug collision");
        assert!(err.to_string().contains("snapshot target slug"));
    }

    #[test]
    fn rejects_unresolvable_and_ambiguous_snapshot_inherit() {
        let missing = snapshot_machine(
            r#"    snapshot:
      inherit:
        name: missing
"#,
        );
        let err = StateMachine::from_yaml_str(&missing).expect_err("missing emitter");
        assert!(err.to_string().contains("unresolvable snapshot.inherit"));

        let ambiguous = r#"
name: snapshot-test
version: 1
states:
  source-a:
    description: Source A
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: build
  source-b:
    description: Source B
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: build
  pending:
    description: Pending
    target: claude-code:anthropic:model
    snapshot:
      inherit:
        name: build
  done:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(ambiguous).expect_err("ambiguous emitter");
        assert!(err.to_string().contains("ambiguous snapshot.inherit"));
    }

    #[test]
    fn rejects_snapshot_inherit_same_without_effective_target_and_fanout_source_without_target() {
        let same_without_target = r#"
name: snapshot-test
version: 1
states:
  source:
    description: Source
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: build
  pending:
    description: Pending
    snapshot:
      inherit:
        name: build
        select:
          target: same
  done:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(same_without_target).expect_err("same target");
        assert!(err.to_string().contains("select.target: same"));

        let fanout = r#"
name: snapshot-test
version: 1
states:
  source:
    description: Source
    all_targets:
      - claude-code:anthropic:model-a
      - claude-code:anthropic:model-b
    snapshot:
      emit:
        name: build
  pending:
    description: Pending
    target: claude-code:anthropic:model-a
    snapshot:
      inherit:
        name: build
        select:
          state: source
  done:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(fanout).expect_err("fanout source");
        assert!(err.to_string().contains("fanout source"));
    }
