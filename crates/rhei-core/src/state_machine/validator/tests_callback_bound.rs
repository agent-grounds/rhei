    /// A machine with a `callback_timeout` of `root` at its root and `edge`
    /// on its `gate -> opening` rule; an empty string leaves that level out.
    fn callback_bound_machine(root: &str, edge: &str, callback: bool) -> String {
        let root = if root.is_empty() { String::new() } else { format!("callback_timeout: {root}\n") };
        let edge = if edge.is_empty() { String::new() } else { format!("    callback_timeout: {edge}\n") };
        let callback = if callback { "    on_enter: \"cli:exit 0\"\n" } else { "" };
        format!(
            "name: bound\nversion: 1\n{root}states:\n  gate:\n    initial: true\n    description: Gate\n  \
             opening:\n    description: Opening\n  review:\n    description: Review\n  done:\n    \
             final: true\n    description: Done\ntransitions:\n  - from: gate\n    to: opening\n    \
             description: Open\n{callback}{edge}  - from: gate\n    to: review\n    description: Review\n  \
             - from: opening\n    to: done\n    description: Done\n  - from: review\n    to: done\n    \
             description: Done\n"
        )
    }

    fn rule<'a>(machine: &'a StateMachine, to: &str) -> &'a TransitionRule {
        machine.transitions.iter().find(|rule| rule.to.0 == to).expect("rule declared")
    }

    // §FS-rhei-validate.4 §FS-rhei-transitions.4.10
    #[test]
    fn callback_timeout_parses_at_the_root_and_on_an_edge() {
        let machine =
            StateMachine::from_yaml_str(&callback_bound_machine("1m", "2h30m", true)).expect("loads");
        assert_eq!(machine.callback_timeout.as_deref(), Some("1m"));
        assert_eq!(rule(&machine, "opening").callback_timeout.as_deref(), Some("2h30m"));
        assert_eq!(rule(&machine, "review").callback_timeout, None);
    }

    // §FS-rhei-validate.4
    #[test]
    fn callback_timeout_refuses_zero_and_malformed_values_at_either_level() {
        for value in ["0s", "0m", "soon", "5"] {
            for (level, yaml, place) in [
                ("root", callback_bound_machine(value, "", true), "the machine"),
                ("edge", callback_bound_machine("", value, true), "the edge 'gate -> opening'"),
            ] {
                let err = StateMachine::from_yaml_str(&yaml)
                    .expect_err(&format!("a {level} callback_timeout of '{value}' is refused"));
                let message = err.to_string();
                assert!(
                    message.contains("callback_timeout")
                        && message.contains(&format!("'{value}'"))
                        && message.contains(place),
                    "the refusal names the field, the place and the value; got: {message}"
                );
            }
        }
    }

    // §FS-rhei-transitions.4.10
    #[test]
    fn callback_bound_resolves_edge_then_machine_then_none() {
        let both = StateMachine::from_yaml_str(&callback_bound_machine("1s", "30s", true)).unwrap();
        let own = both.callback_bound(rule(&both, "opening")).expect("the edge's bound");
        assert_eq!((own.limit.as_secs(), own.authored.as_str()), (30, "30s"));
        let inherited = both.callback_bound(rule(&both, "review")).expect("the machine's bound");
        assert_eq!((inherited.limit.as_secs(), inherited.authored.as_str()), (1, "1s"));

        let edge_only = StateMachine::from_yaml_str(&callback_bound_machine("", "2m", true)).unwrap();
        let authored = edge_only.callback_bound(rule(&edge_only, "opening")).expect("edge bound");
        assert_eq!((authored.limit.as_secs(), authored.authored.as_str()), (120, "2m"));
        assert_eq!(edge_only.callback_bound(rule(&edge_only, "review")), None);

        let neither = StateMachine::from_yaml_str(&callback_bound_machine("", "", true)).unwrap();
        assert_eq!(neither.callback_bound(rule(&neither, "opening")), None);
    }

    // §FS-rhei-validate.4
    #[test]
    fn a_callback_timeout_on_an_edge_without_a_callback_is_warned_about() {
        let rhei = parse("# Rhei: T\n\n## Tasks\n\n### Task 1: One\n**State:** gate\n").unwrap();

        let idle = StateMachine::from_yaml_str(&callback_bound_machine("", "2m", false))
            .expect("an idle bound is not refused");
        let warnings = validate_with_machine(&rhei, &idle).warnings;
        assert!(
            warnings.iter().any(|w| w.contains("callback_timeout") && w.contains("gate -> opening")),
            "expected the idle-bound warning naming the edge; got: {warnings:?}"
        );

        let bounded = StateMachine::from_yaml_str(&callback_bound_machine("", "2m", true)).unwrap();
        let warnings = validate_with_machine(&rhei, &bounded).warnings;
        assert!(
            !warnings.iter().any(|w| w.contains("callback_timeout")),
            "a bound with a callback is not warned about; got: {warnings:?}"
        );
    }
