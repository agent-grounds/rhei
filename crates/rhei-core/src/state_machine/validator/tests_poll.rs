    fn poll_machine(body: &str) -> String {
        format!(
            r#"
name: poll-test
version: 1.0
states:
{body}
profiles:
  default:
    initial: ci-wait
    allowed: [ci-wait, done]
node_policy:
  root: default
  default: default
"#
        )
    }

    #[test]
    fn accepts_well_formed_poll_state() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait for CI
    program: "./check.sh"
    poll:
      interval: 5m
      max_attempts: 12
  done:
    description: Done
    final: true
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75
  - from: ci-wait
    to: done
    exit_code: 0"#,
        );
        let sm = StateMachine::from_yaml_str(&yaml).expect("valid poll state");
        let poll = sm.states.get("ci-wait").and_then(|s| s.poll.as_ref()).expect("poll present");
        assert_eq!(poll.max_attempts, 12);
        assert_eq!(poll.interval, "5m");
    }

    #[test]
    fn rejects_poll_with_invalid_interval() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: sometimes
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("bad interval");
        assert!(err.to_string().contains("poll.interval"));
    }

    #[test]
    fn rejects_poll_with_zero_max_attempts() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: 1m
      max_attempts: 0
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("bad max_attempts");
        assert!(err.to_string().contains("poll.max_attempts"));
    }

    #[test]
    fn rejects_poll_with_visits() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    visits: 5
    poll:
      interval: 1m
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("visits conflict");
        assert!(err.to_string().contains("'poll' and 'visits'"));
    }

    #[test]
    fn rejects_poll_on_gating_state() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    gating: true
    poll:
      interval: 1m
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("gating conflict");
        assert!(err.to_string().contains("gating"));
    }

    #[test]
    fn rejects_poll_without_self_loop() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: 1m
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: done
    exit_code: 0"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("missing self-loop");
        assert!(err.to_string().contains("self-loop"));
    }

    #[test]
    fn rejects_poll_with_snapshot_inherit() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: 1m
      max_attempts: 3
    snapshot:
      inherit:
        name: build
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("snapshot inherit conflict");
        assert!(err.to_string().contains("snapshot.inherit"));
    }

    /// A poll may declare the person it waits on; the load trims the label
    /// into the machine, so the stored field a serializer would print and the
    /// accessor every text surface reads name the same person.
    /// §FS-rhei-states.2.5
    #[test]
    fn accepts_poll_waiting_on_a_person() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait for the author
    program: "./check.sh"
    poll:
      interval: 10m
      max_attempts: 60
      waiting_on: "  author  "
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75
  - from: ci-wait
    to: done
    exit_code: 0"#,
        );
        let sm = StateMachine::from_yaml_str(&yaml).expect("valid person-waiting poll");
        let def = sm.states.get("ci-wait").expect("state present");
        assert_eq!(def.waiting_on_person(), Some("author"));
        assert_eq!(
            def.poll.as_ref().and_then(|poll| poll.waiting_on.as_deref()),
            Some("author"),
            "the padded label must be normalized into the machine, not on each read"
        );
    }

    /// Absence is the machine-backoff reading, so an ordinary poll answers
    /// `None` and every surface keeps treating it as live work.
    /// §FS-rhei-states.2.5
    #[test]
    fn poll_without_waiting_on_waits_on_no_person() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait for CI
    program: "./check.sh"
    poll:
      interval: 5m
      max_attempts: 12
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75
  - from: ci-wait
    to: done
    exit_code: 0"#,
        );
        let sm = StateMachine::from_yaml_str(&yaml).expect("valid poll state");
        assert_eq!(sm.states.get("ci-wait").and_then(|def| def.waiting_on_person()), None);
    }

    /// A blank label declares the person wait and then names nobody, which is
    /// the one shape no surface can print. §FS-rhei-states.1.3
    #[test]
    fn rejects_poll_with_blank_waiting_on() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: 1m
      max_attempts: 3
      waiting_on: "   "
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("blank waiting_on");
        assert!(
            err.to_string().contains("empty poll.waiting_on"),
            "unexpected error: {err}"
        );
    }
