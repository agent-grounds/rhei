    // §FS-rhei-programs.8.2: two exit_code rules from one state that can match the same code, with
    // nothing to tell them apart, are refused when the machine loads; disjoint and told-apart pairs stay.

    /// A machine whose program state `analyze` (a poll state when `poll` is set) leaves by `rules`.
    fn exit_code_machine(poll: bool, rules: &str) -> String {
        let poll = if poll { "\n    poll: { interval: 1m, max_attempts: 3 }" } else { "" };
        format!(
            r#"name: exit-code-overlap
version: 1
states:
  analyze:
    initial: true
    program:
      command: ["sh", "-c", "exit 12"]{poll}
  native: {{ gating: true }}
  human-intervention: {{ gating: true }}
  cover: {{ gating: true }}
  failed: {{ final: true }}
  done: {{ final: true }}
transitions:
{rules}
  - {{ from: native, to: done }}
  - {{ from: human-intervention, to: done }}
  - {{ from: cover, to: done }}
"#
        )
    }

    /// Load `rules` and require the refusal to name the state, the shared code and both rules.
    fn assert_overlap_refused(poll: bool, rules: &str, code: &str, first: &str, second: &str) {
        let err = StateMachine::from_yaml_str(&exit_code_machine(poll, rules))
            .expect_err("overlapping exit_code rules must be refused");
        let message = err.to_string();
        for needle in [
            "state 'analyze'".to_string(),
            code.to_string(),
            format!("'analyze' -> '{first}'"),
            format!("'analyze' -> '{second}'"),
        ] {
            assert!(message.contains(&needle), "refusal should name {needle:?}; got: {message}");
        }
    }

    fn assert_accepted(poll: bool, rules: &str) {
        if let Err(err) = StateMachine::from_yaml_str(&exit_code_machine(poll, rules)) {
            panic!("these exit_code rules are told apart and must load; got: {err}");
        }
    }

    #[test]
    fn exit_code_overlap_refuses_equal_integers() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: cover, exit_code: 10 }
  - { from: analyze, to: human-intervention, exit_code: 12 }
  - { from: analyze, to: failed, exit_code: nonzero }
  - { from: analyze, to: native, exit_code: 12 }",
            "exit code 12",
            "human-intervention",
            "native",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_exact_duplicate() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: human-intervention, exit_code: 12 }
  - { from: analyze, to: failed, exit_code: nonzero }
  - { from: analyze, to: human-intervention, exit_code: 12 }",
            "exit code 12",
            "human-intervention",
            "human-intervention",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_integer_in_array() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: native, exit_code: [12, 20] }
  - { from: analyze, to: human-intervention, exit_code: 12 }",
            "exit code 12",
            "native",
            "human-intervention",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_arrays_sharing_a_value() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: native, exit_code: [12, 20] }
  - { from: analyze, to: human-intervention, exit_code: [11, 12] }",
            "exit code 12",
            "native",
            "human-intervention",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_two_nonzero() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: human-intervention, exit_code: nonzero }
  - { from: analyze, to: failed, exit_code: nonzero }",
            "nonzero",
            "human-intervention",
            "failed",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_when_only_one_rule_has_a_condition() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: native, exit_code: 12, condition: visitCount < 2 }
  - { from: analyze, to: human-intervention, exit_code: 12 }",
            "exit code 12",
            "native",
            "human-intervention",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_identical_conditions() {
        assert_overlap_refused(
            false,
            "  - { from: analyze, to: native, exit_code: 12, condition: visitCount < 2 }
  - { from: analyze, to: human-intervention, exit_code: 12, condition: ' visitCount < 2 ' }",
            "exit code 12",
            "native",
            "human-intervention",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_two_exits_from_a_poll_state() {
        assert_overlap_refused(
            true,
            "  - { from: analyze, to: analyze, exit_code: 75 }
  - { from: analyze, to: native, exit_code: 12 }
  - { from: analyze, to: human-intervention, exit_code: 12 }",
            "exit code 12",
            "native",
            "human-intervention",
        );
    }

    #[test]
    fn exit_code_overlap_refuses_two_self_loops_on_a_poll_state() {
        assert_overlap_refused(
            true,
            "  - { from: analyze, to: analyze, exit_code: 75 }
  - { from: analyze, to: analyze, exit_code: [75, 76] }
  - { from: analyze, to: native, exit_code: 0 }",
            "exit code 75",
            "analyze",
            "analyze",
        );
    }

    #[test]
    fn exit_code_overlap_accepts_disjoint_codes() {
        assert_accepted(
            false,
            "  - { from: analyze, to: cover, exit_code: 10 }
  - { from: analyze, to: native, exit_code: [12, 20] }
  - { from: analyze, to: human-intervention, exit_code: [11, 13] }
  - { from: analyze, to: failed, exit_code: nonzero }",
        );
    }

    #[test]
    fn exit_code_overlap_accepts_specific_beside_nonzero() {
        assert_accepted(
            false,
            "  - { from: analyze, to: native, exit_code: 12 }
  - { from: analyze, to: failed, exit_code: nonzero }",
        );
    }

    #[test]
    fn exit_code_overlap_accepts_distinct_conditions() {
        // The laid grounded-ticket machines' `green` and `merging` pairs have this shape.
        assert_accepted(
            false,
            "  - { from: analyze, to: native, exit_code: 12, condition: visitCount < visits }
  - { from: analyze, to: human-intervention, exit_code: 12, condition: visitCount >= visits }
  - { from: analyze, to: cover, exit_code: nonzero, condition: visitCount < visits }
  - { from: analyze, to: failed, exit_code: nonzero, condition: visitCount >= visits }",
        );
    }

    #[test]
    fn exit_code_overlap_accepts_poll_retry_beside_its_exhaustion_edge() {
        // §FS-rhei-run.5.1: the shared grounded-ticket template's `consulting` state, laid 416 times.
        assert_accepted(
            true,
            "  - { from: analyze, to: analyze, exit_code: 75 }
  - { from: analyze, to: human-intervention, exit_code: 75 }
  - { from: analyze, to: native, exit_code: 0 }",
        );
    }
