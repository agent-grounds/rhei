    // A state machine may never carry the machine-only `clamp_projects`: it is
    // refused by its presence, whatever its value, before deserialization can
    // drop it. §FS-rhei-agents.1.1.1 §FS-rhei-budgets.2.1

    const MACHINE_ONLY_BASE: &str = r#"name: machine-only
version: 1
states:
  work:
    description: Work
  done:
    description: Done
    final: true
transitions:
  - from: work
    to: done
profiles:
  loop:
    initial: work
    allowed: [work, done]
node_policy:
  root: loop
  default: loop
"#;

    #[test]
    fn clamp_projects_is_refused_anywhere_in_a_state_machine_whatever_its_value() {
        let places: [(&str, &str, &str); 4] = [
            ("clamp_projects", "version: 1\n", "version: 1\nclamp_projects: {v}\n"),
            ("defaults.clamp_projects", "version: 1\n", "version: 1\ndefaults:\n  clamp_projects: {v}\n"),
            ("profiles.loop.clamp_projects", "    initial: work\n", "    clamp_projects: {v}\n    initial: work\n"),
            (
                "profiles.loop.defaults.clamp_projects",
                "    initial: work\n",
                "    defaults:\n      clamp_projects: {v}\n    initial: work\n",
            ),
        ];
        assert!(StateMachine::from_yaml_str(MACHINE_ONLY_BASE).is_ok(), "the base machine loads");
        for (field, anchor, edit) in places {
            for value in ["true", "false", "null"] {
                let yaml = MACHINE_ONLY_BASE.replacen(anchor, &edit.replace("{v}", value), 1);
                let message = StateMachine::from_yaml_str(&yaml)
                    .expect_err("the switch in a state machine is refused")
                    .to_string();
                assert!(
                    message.contains(
                        "`defaults.clamp_projects` may only be set in the machine settings file"
                    ),
                    "{field} = {value}: {message}"
                );
                assert!(message.contains(&format!("({field})")), "{field} = {value}: {message}");
                assert!(message.contains("set it in: "), "{field} = {value}: {message}");
            }
        }
    }

    #[test]
    fn clamp_projects_refusal_names_the_state_machine_file() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("states.yaml");
        let yaml = MACHINE_ONLY_BASE.replacen("version: 1\n", "version: 1\nclamp_projects: false\n", 1);
        std::fs::write(&path, yaml).expect("write machine");
        let message = StateMachine::from_yaml_file(&path).expect_err("refused").to_string();
        assert!(
            message.contains(&format!("found in:  {} (clamp_projects)", path.display())),
            "{message}"
        );
    }
