    // A leading byte order mark is not part of a state machine's text. §REQ-cross-platform.9

    /// The machine from #630, which loads without the mark.
    const BOM_MACHINE: &str = "name: m\nversion: 1\nstates:\n  o:\n    initial: true\n  d:\n    final: true\ntransitions:\n  - {from: o, to: d}\n";

    fn machine_yaml(machine: &StateMachine) -> String {
        serde_yaml::to_string(machine).expect("a loaded machine serializes")
    }

    #[test]
    #[ignore = "red until #630: a leading BOM is read as a document boundary; implement removes this"]
    fn a_machine_string_with_a_leading_byte_order_mark_loads_as_the_same_machine() {
        let plain = StateMachine::from_yaml_str(BOM_MACHINE).expect("the machine loads without the mark");
        let marked = StateMachine::from_yaml_str(&format!("\u{feff}{BOM_MACHINE}"))
            .expect("the machine loads with one leading mark");
        assert_eq!(machine_yaml(&marked), machine_yaml(&plain));
    }

    #[test]
    #[ignore = "red until #630: a states.yaml saved with a BOM is refused; implement removes this"]
    fn a_machine_file_with_a_leading_byte_order_mark_loads_as_the_same_machine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plain_path = dir.path().join("plain").join("states.yaml");
        let marked_path = dir.path().join("marked").join("states.yaml");
        for (path, bytes) in [
            (&plain_path, BOM_MACHINE.as_bytes().to_vec()),
            (&marked_path, [b"\xef\xbb\xbf".as_slice(), BOM_MACHINE.as_bytes()].concat()),
        ] {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("machine dir");
            std::fs::write(path, bytes).expect("write machine");
        }
        let plain = StateMachine::from_yaml_file(&plain_path).expect("the file loads without the mark");
        let marked = StateMachine::from_yaml_file(&marked_path).expect("the file loads with the mark");
        assert_eq!(machine_yaml(&marked), machine_yaml(&plain));
    }

    /// Exactly one mark is set aside: a second one is content, so the machine is
    /// not the BOM-less one. §REQ-cross-platform.9
    #[test]
    fn a_second_byte_order_mark_is_content_and_is_not_set_aside() {
        let plain = StateMachine::from_yaml_str(BOM_MACHINE).expect("the machine loads without the mark");
        match StateMachine::from_yaml_str(&format!("\u{feff}\u{feff}{BOM_MACHINE}")) {
            Err(_) => {}
            Ok(marked) => assert_ne!(machine_yaml(&marked), machine_yaml(&plain)),
        }
    }
