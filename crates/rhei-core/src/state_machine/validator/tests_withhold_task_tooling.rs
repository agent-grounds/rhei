    // ---- withhold_task_tooling as loaded: §FS-rhei-task-tooling.4 §FS-rhei-states-cmd.5 ----

    /// The `review` state of a machine whose only difference is `review_extra`.
    fn loaded_review_state(review_extra: &str) -> StateDef {
        let yaml = format!(
            "name: withhold-load\nversion: 1.0\nstates:\n  pending:\n    agent: claude-code\n\
             \x20 review:\n    agent: claude-code\n{review_extra}\
             \x20 completed:\n    final: true\ntransitions:\n  - from: pending\n    to: review\n\
             \x20 - from: review\n    to: completed\n"
        );
        let machine = StateMachine::from_yaml_str(&yaml).expect("machine loads");
        machine.states.get("review").expect("review state").clone()
    }

    #[test]
    fn withhold_task_tooling_true_loads_as_withholding() {
        let state = loaded_review_state("    withhold_task_tooling: true\n");
        assert_eq!(state.withhold_task_tooling, Some(true));
    }

    #[test]
    fn an_absent_withhold_task_tooling_loads_as_not_withholding() {
        let state = loaded_review_state("");
        assert_eq!(state.withhold_task_tooling, None);
    }

    /// An authored `false` stays authored, so the surfaces that show an
    /// authored field can show it.
    #[test]
    fn an_authored_false_withhold_task_tooling_is_kept_as_authored() {
        let state = loaded_review_state("    withhold_task_tooling: false\n");
        assert_eq!(state.withhold_task_tooling, Some(false));
        let serialized = serde_yaml::to_string(&state).expect("serialize");
        assert!(serialized.contains("withhold_task_tooling: false"), "{serialized}");
    }

    #[test]
    fn an_unauthored_state_serialises_without_withhold_task_tooling() {
        let serialized = serde_yaml::to_string(&loaded_review_state("")).expect("serialize");
        assert!(!serialized.contains("withhold_task_tooling"), "{serialized}");
    }
