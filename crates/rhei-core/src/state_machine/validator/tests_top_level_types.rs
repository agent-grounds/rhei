    // The types of `name`, `version` and `models` held at load. Included into `mod tests` in
    // `state_machine/mod.rs`, so the indentation is the module's. §FS-rhei-states.1.1

    fn top_level_machine(top: &str) -> String {
        format!(
            "{top}states:\n  open:\n    description: open\n  done:\n    final: true\n\
             transitions:\n  - from: open\n    to: done\n"
        )
    }

    /// The loader's refusal of `top`, which must name `key`.
    fn assert_top_level_refused(top: &str, key: &str) {
        let error = StateMachine::from_yaml_str(&top_level_machine(top))
            .expect_err(&format!("{top:?} is refused"))
            .to_string();
        assert!(
            error.contains(&format!("{key}:")) || error.contains(&format!("`{key}`")),
            "the refusal of {top:?} names `{key}`: {error}"
        );
    }

    #[test]
    fn a_null_name_is_refused_at_load() {
        assert_top_level_refused("name:\nversion: 1\n", "name");
        assert_top_level_refused("name: null\nversion: 1\n", "name");
        assert_top_level_refused("name: ~\nversion: 1\n", "name");
    }

    #[test]
    fn a_version_that_is_neither_number_nor_string_is_refused_at_load() {
        for version in ["", " null", " ~", " {a: 1}", " [1, 2]", " true", " false"] {
            assert_top_level_refused(&format!("name: m\nversion:{version}\n"), "version");
        }
    }

    #[test]
    fn a_name_of_the_wrong_type_is_still_refused_at_load() {
        assert_top_level_refused("name: [a, b]\nversion: 1\n", "name");
        assert_top_level_refused("name: {a: 1}\nversion: 1\n", "name");
        for name in ["3", "1.5", "true"] {
            assert_top_level_refused(&format!("name: {name}\nversion: 1\n"), "name");
        }
    }

    #[test]
    fn a_models_entry_that_is_not_a_string_is_refused_at_load() {
        let cases = [
            (" [null]", "models[0]"),
            (" [true]", "models[0]"),
            (" [1.5]", "models[0]"),
            (" [~]", "models[0]"),
            (" [a, [b]]", "models[1]"),
            (" [{a: 1}]", "models[0]"),
            ("", "models"),
            (" ~", "models"),
        ];
        for (models, key) in cases {
            assert_top_level_refused(&format!("name: m\nversion: 1\nmodels:{models}\n"), key);
        }
    }

    #[test]
    fn models_of_strings_an_empty_list_and_no_models_load() {
        for (top, expected) in [
            ("name: m\nversion: 1\nmodels: [a, b]\n", vec!["a", "b"]),
            ("name: m\nversion: 1\nmodels: []\n", vec![]),
            ("name: m\nversion: 1\n", vec![]),
        ] {
            let machine = StateMachine::from_yaml_str(&top_level_machine(top))
                .unwrap_or_else(|error| panic!("{top:?} loads: {error}"));
            assert_eq!(machine.models, expected, "{top:?}");
        }
    }

    #[test]
    fn a_version_that_is_a_number_or_a_string_and_an_empty_name_load() {
        for version in ["1", "1.5", "\"1.2\"", "v2", "-3"] {
            let top = format!("name: m\nversion: {version}\n");
            StateMachine::from_yaml_str(&top_level_machine(&top))
                .unwrap_or_else(|error| panic!("version: {version} loads: {error}"));
        }
        let machine = StateMachine::from_yaml_str(&top_level_machine("name: \"\"\nversion: 1\n"))
            .expect("name: \"\" is a string and loads");
        assert_eq!(machine.name, "");
    }
