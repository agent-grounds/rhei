// A collection the target writes in flow style is refused rather than extended,
// for the two collections the end-to-end tests do not reach: `models`, and
// `node_policy` written in flow style as a whole. §FS-rhei-library.7.4
mod templates_union_flow_tests {
    use super::super::*;

    const HOST: &str = r#"name: host
version: 1
states:
  pending:
    description: Ready for work
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
profiles:
  host:
    initial: pending
    allowed: [pending, completed, cancelled]
models: [fast]
node_policy:
  root: host
  default: host
  by_type:
    task: host
"#;

    const PART: &str = r#"name: handoff
version: 1
states:
  work:
    description: Do the work
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: work
    to: completed
profiles:
  handoff:
    initial: work
    allowed: [work, completed, cancelled]
models:
  - fast
node_policy:
  root: handoff
  default: handoff
  by_type:
    step: handoff
"#;

    fn part(yaml: &str) -> PartMachine {
        PartMachine {
            name: "handoff".to_owned(),
            text: yaml.to_owned(),
            value: serde_yaml::from_str(yaml).expect("the fixture parses"),
            machine: rhei_core::state_machine::StateMachine::parse_fragment(yaml)
                .expect("the fixture is a readable machine"),
        }
    }

    /// The refusal names the target's file and the dotted key, and its remedy
    /// is block style.
    fn assert_refused(host: &str, part_yaml: &str, key: &str) {
        let error = union_machine(host, Path::new("host/states.yaml"), &part(part_yaml))
            .expect_err("a flow collection the union must extend is refused");
        let message = error.to_string();
        assert!(message.starts_with("host/states.yaml: "), "names the target's file; got: {message}");
        assert!(message.contains(&format!("`{key}`")), "names `{key}`; got: {message}");
        let help = error.help().map(|help| help.to_string()).unwrap_or_default();
        assert!(help.contains("block style"), "the remedy is block style; got: {help}");
    }

    /// A flow `models` list the template adds a model to. §FS-rhei-library.7.4
    #[test]
    fn a_flow_models_list_the_union_extends_is_refused() {
        assert_refused(HOST, &PART.replace("  - fast\n", "  - fast\n  - deep\n"), "models");
    }

    /// A flow `models` list that already holds every model the template names
    /// is left as written. §FS-rhei-library.7.4
    #[test]
    fn a_flow_models_list_the_union_adds_nothing_to_is_not_refused() {
        let unioned = union_machine(HOST, Path::new("host/states.yaml"), &part(PART))
            .expect("nothing is written into the flow list");
        assert!(unioned.contains("models: [fast]\n"), "the line is untouched; got:\n{unioned}");
    }

    /// `node_policy` written in flow style as a whole is refused under its own
    /// key, not under `node_policy.by_type`. §FS-rhei-library.7.4
    #[test]
    fn a_flow_node_policy_the_union_extends_is_refused() {
        let host = HOST.replace(
            "node_policy:\n  root: host\n  default: host\n  by_type:\n    task: host\n",
            "node_policy: { root: host, default: host, by_type: { task: host } }\n",
        );
        assert_refused(&host, PART, "node_policy");
    }

    /// A flow `by_type` inside a block `node_policy` is refused by its dotted
    /// key. §FS-rhei-library.7.4
    #[test]
    fn a_flow_by_type_the_union_extends_is_refused() {
        let host = HOST.replace("  by_type:\n    task: host\n", "  by_type: { task: host }\n");
        assert_refused(&host, PART, "node_policy.by_type");
    }
}
