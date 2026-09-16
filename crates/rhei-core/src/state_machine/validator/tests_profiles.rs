    fn profiles_machine_yaml() -> &'static str {
        r#"
name: profiled
version: 3.0
states:
  pending:
    description: Work
  review:
    description: Inspect
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: review
  - from: pending
    to: completed
  - from: review
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, review, completed]
  fast-track:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
  by_type:
    bug: fast-track
"#
    }

    #[test]
    fn loads_profiles_and_node_policy() {
        let sm = StateMachine::from_yaml_str(profiles_machine_yaml()).expect("load ok");
        let profiles = sm.profiles.as_ref().expect("profiles present");
        assert_eq!(profiles.len(), 2);
        let default = sm.profile_for_node("task", 1).expect("default resolves");
        assert_eq!(default.initial, "pending");
        let fast = sm.profile_for_node("bug", 1).expect("bug resolves to fast-track");
        assert_eq!(fast.allowed, vec!["pending".to_string(), "completed".to_string()]);
        let root = sm.root_profile().expect("root profile");
        assert_eq!(root.initial, "pending");
    }

    #[test]
    fn custom_agent_profile_modes_preserve_declaration_order() {
        let profile: CustomAgentProfile = serde_yaml::from_str(
            r#"command: [custom-agent]
modes:
  yolo: [--yolo]
  safe: [--safe]
"#,
        )
        .expect("profile parses");

        assert_eq!(
            profile.modes.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["yolo", "safe"]
        );
    }

    #[test]
    fn profile_for_returns_none_when_not_declared() {
        let sm = sample_machine();
        assert!(sm.profile_for_node("task", 1).is_none());
        assert!(sm.root_profile().is_none());
    }

    #[test]
    fn rejects_profiles_without_node_policy() {
        let yaml = r#"
name: half-config
version: 1.0
states:
  pending: { description: Work }
profiles:
  default:
    initial: pending
    allowed: [pending]
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("missing node_policy");
        assert!(err.to_string().contains("no 'node_policy'"));
    }

    #[test]
    fn rejects_version_three_machine_without_profiles_and_node_policy() {
        let yaml = r#"
name: v3-missing-policy
version: 3.0
states:
  pending: { description: Work }
  completed: { description: Done, final: true }
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("v3 requires profiles");
        assert!(err.to_string().contains("version 3 requires 'profiles' and 'node_policy'"));
    }

    #[test]
    fn rejects_node_policy_without_profiles() {
        let yaml = r#"
name: half-config
version: 1.0
states:
  pending: { description: Work }
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("missing profiles");
        assert!(err.to_string().contains("no 'profiles'"));
    }

    #[test]
    fn rejects_profile_with_initial_not_in_allowed() {
        let yaml = r#"
name: bad-initial
version: 1.0
states:
  pending: { description: Work }
  review: { description: Inspect }
profiles:
  default:
    initial: review
    allowed: [pending]
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("initial not in allowed");
        assert!(err.to_string().contains("is not in its own 'allowed' list"));
    }

    #[test]
    fn rejects_profile_with_unknown_state_in_allowed() {
        let yaml = r#"
name: unknown-allowed
version: 1.0
states:
  pending: { description: Work }
profiles:
  default:
    initial: pending
    allowed: [pending, missing]
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("unknown allowed");
        assert!(err.to_string().contains("unknown state 'missing'"));
    }

    #[test]
    fn rejects_node_policy_default_with_undefined_profile() {
        let yaml = r#"
name: dangling-default
version: 1.0
states:
  pending: { description: Work }
  completed: { description: Done, final: true }
transitions:
  - from: pending
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: nonexistent
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("dangling profile");
        assert!(err.to_string().contains("'node_policy.default' references undefined profile"));
    }

    #[test]
    fn rejects_node_policy_by_type_with_reserved_kind() {
        let yaml = r#"
name: reserved-kind
version: 1.0
states:
  pending: { description: Work }
  completed: { description: Done, final: true }
transitions:
  - from: pending
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
  by_type:
    rhei: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("reserved kind");
        assert!(err.to_string().contains("reserved kind 'rhei'"));
    }

    #[test]
    fn rejects_state_initial_true_when_profiles_present() {
        let yaml = r#"
name: legacy-initial
version: 1.0
states:
  pending:
    description: Work
    initial: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("initial forbidden with profiles");
        assert!(err.to_string().contains("declares 'initial: true'"));
    }

    #[test]
    fn enforces_profile_allowed_on_task_state() {
        let sm = StateMachine::from_yaml_str(profiles_machine_yaml()).expect("load ok");
        let input = r#"# Rhei: profile-check
**States:** profiled
---
structure:
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: First
**State:** review
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sm);
        assert!(!report.has_errors(), "review is allowed: {:?}", report.errors);
    }

    #[test]
    fn rejects_task_state_outside_profile_allowed() {
        // Build a machine where `default` profile excludes `review`, then
        // author a task in `review` — it's a defined state but disallowed
        // for this node.
        let yaml = r#"
name: restricted
version: 1.0
states:
  pending: { description: Work }
  review: { description: Inspect }
  completed: { description: Done, final: true }
transitions:
  - from: pending
    to: completed
  - from: review
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("load ok");
        let input = "# Rhei: restricted-check\n**States:** restricted\n\n## Tasks\n\n### Task 1: First\n**State:** review\n";
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sm);
        assert!(
            report.errors.iter().any(|e| e.contains("not allowed by its resolved profile")),
            "expected profile-allowed error, got {:?}",
            report.errors
        );
    }

    #[test]
    fn node_policy_overrides_match_type_and_level_before_by_type() {
        let yaml = r#"
name: policy-overrides
version: 3.0
states:
  draft: { description: Draft }
  pending: { description: Work }
  completed: { description: Done, final: true }
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
profiles:
  reviewed:
    initial: draft
    allowed: [draft, pending, completed]
  simple:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: reviewed
  default: reviewed
  by_type:
    task: reviewed
  overrides:
    - match: { type: task, level: 2 }
      profile: simple
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("load ok");

        assert_eq!(sm.profile_for_node("task", 1).expect("top-level profile").initial, "draft");
        assert_eq!(sm.profile_for_node("task", 2).expect("override profile").initial, "pending");
    }

    #[test]
    fn rejects_profile_without_path_to_final_state() {
        let yaml = r#"
name: unreachable-profile
version: 3.0
states:
  pending: { description: Work }
  blocked: { description: Blocked }
  completed: { description: Done, final: true }
transitions:
  - from: pending
    to: blocked
profiles:
  default:
    initial: pending
    allowed: [pending, blocked, completed]
node_policy:
  root: default
  default: default
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("profile is unreachable");
        assert!(err
            .to_string()
            .contains("no path using only allowed states reaches a final state"));
    }

    #[test]
    fn validates_node_policy_against_plan_structure() {
        let yaml = r#"
name: policy-structure
version: 3.0
states:
  pending: { description: Work }
  completed: { description: Done, final: true }
transitions:
  - from: pending
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
  by_type:
    bug: default
  overrides:
    - match: { type: task, level: 3 }
      profile: default
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("load ok");
        let input =
            "# Rhei: structure-check\n\n## Tasks\n\n### Task 1: First\n**State:** pending\n";
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sm);
        let joined = report.errors.join("\n");

        assert!(joined.contains("node_policy.by_type references node kind 'bug'"), "{joined}");
        assert!(joined.contains("match.level is 3"), "{joined}");
    }
