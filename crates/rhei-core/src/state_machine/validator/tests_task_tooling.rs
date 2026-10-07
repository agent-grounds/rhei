    // ---- withhold_task_tooling: §FS-rhei-states.1.3 §FS-rhei-task-tooling.4 ----

    /// A machine whose `review` state carries `withhold:` verbatim under the
    /// keys `extra:` adds.
    fn withholding_machine(extra: &str, withhold: &str) -> String {
        format!(
            r#"
name: withhold
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
  review:
    description: Review
{extra}    withhold_task_tooling: {withhold}
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#
        )
    }

    #[test]
    #[ignore = "red until #475 lets a task name its own MCP servers and skills"]
    fn withhold_task_tooling_must_be_a_boolean() {
        for value in ["\"yes\"", "1", "[thunderbird-mail]"] {
            let yaml = withholding_machine("    agent: claude-code\n", value);
            let err = StateMachine::from_yaml_str(&yaml)
                .expect_err("a non-boolean withhold_task_tooling must be refused");
            assert!(err.to_string().contains("withhold_task_tooling"), "{value}: {err}");
        }
    }

    #[test]
    #[ignore = "red until #475 lets a task name its own MCP servers and skills"]
    fn withhold_task_tooling_rejected_on_program_state() {
        let yaml = withholding_machine("    program: \"make review\"\n", "true");
        let err = StateMachine::from_yaml_str(&yaml).expect_err("program excludes withholding");
        let message = err.to_string();
        assert!(
            message.contains("withhold_task_tooling") && message.contains("program"),
            "{message}"
        );
    }

    #[test]
    #[ignore = "red until #475 lets a task name its own MCP servers and skills"]
    fn withhold_task_tooling_rejected_on_gating_state() {
        let yaml = withholding_machine("    gating: true\n", "true");
        let err = StateMachine::from_yaml_str(&yaml).expect_err("gating excludes withholding");
        let message = err.to_string();
        assert!(
            message.contains("withhold_task_tooling") && message.contains("gating"),
            "{message}"
        );
    }

    #[test]
    #[ignore = "red until #475 lets a task name its own MCP servers and skills"]
    fn withhold_task_tooling_rejected_on_final_state() {
        let yaml = r#"
name: withhold-final
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
  completed:
    description: Done
    final: true
    withhold_task_tooling: true
transitions:
  - from: pending
    to: completed
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("final excludes withholding");
        let message = err.to_string();
        assert!(
            message.contains("withhold_task_tooling") && message.contains("final"),
            "{message}"
        );
    }
