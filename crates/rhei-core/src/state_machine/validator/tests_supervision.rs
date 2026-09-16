    // §FS-rhei-supervision.1.1 §FS-rhei-supervision.1.2: the `execute_on:`
    // grammar and its validation rules.

    fn supervising_machine(states: &str, transitions: &str) -> String {
        format!(
            r#"
name: supervising-test
version: 1.0
states:
{states}
transitions:
{transitions}
profiles:
  default:
    initial: supervising
    allowed: [supervising, review, human-review, completed, cancelled]
node_policy:
  root: default
  default: default
"#
        )
    }

    /// The canonical supervisor from §FS-rhei-supervision.7.
    fn canonical_states() -> &'static str {
        r#"  supervising:
    description: Supervise the subtree
    execute_on: descendant-terminal
    agent: pi
    visits: 12
  review:
    description: Review
    agent: claude-code
  human-review:
    description: Human call
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true"#
    }

    fn canonical_transitions() -> &'static str {
        r#"  - { from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }"#
    }

    #[test]
    fn accepts_the_canonical_supervisor() {
        let yaml = supervising_machine(canonical_states(), canonical_transitions());
        let machine = StateMachine::from_yaml_str(&yaml).expect("canonical supervisor is valid");
        assert_eq!(
            machine.states.get("supervising").and_then(|def| def.execute_on()),
            Some(ExecuteOn::DescendantTerminal)
        );
        assert_eq!(machine.states.get("review").and_then(|def| def.execute_on()), None);
    }

    /// §FS-rhei-supervision.1.1: all four values load, and each parses into the
    /// scope and the event it spells.
    #[test]
    fn accepts_every_scope_and_event_pair() {
        for (value, scope, event) in [
            ("child-terminal", SupervisionScope::Child, SupervisionEvent::Terminal),
            ("child-transition", SupervisionScope::Child, SupervisionEvent::Transition),
            ("descendant-terminal", SupervisionScope::Descendant, SupervisionEvent::Terminal),
            (
                "descendant-transition",
                SupervisionScope::Descendant,
                SupervisionEvent::Transition,
            ),
        ] {
            let yaml = supervising_machine(
                &canonical_states()
                    .replace("execute_on: descendant-terminal", &format!("execute_on: {value}")),
                canonical_transitions(),
            );
            let machine =
                StateMachine::from_yaml_str(&yaml).unwrap_or_else(|err| panic!("{value}: {err}"));
            let execute_on = machine
                .states
                .get("supervising")
                .and_then(|def| def.execute_on())
                .unwrap_or_else(|| panic!("{value} declares a supervisor"));
            assert_eq!(execute_on.as_str(), value);
            assert_eq!(execute_on.scope(), scope, "{value} scope");
            assert_eq!(execute_on.event(), event, "{value} event");
        }
    }

    /// A wrong value cannot be guessed back from a message that only says it is
    /// wrong: the grammar has two axes, so the refusal names all four values.
    // §FS-rhei-supervision.1.2
    #[test]
    fn rejects_an_unknown_execute_on_value() {
        for value in ["subtree", "task", "state", "child", "terminal", "child_terminal"] {
            let yaml = supervising_machine(
                &canonical_states()
                    .replace("execute_on: descendant-terminal", &format!("execute_on: {value}")),
                canonical_transitions(),
            );
            let err =
                StateMachine::from_yaml_str(&yaml).expect_err("only the four values are legal");
            let message = err.to_string();
            for legal in [
                "child-terminal",
                "child-transition",
                "descendant-terminal",
                "descendant-transition",
            ] {
                assert!(message.contains(legal), "'{value}' must name {legal}; got: {message}");
            }
        }
    }

    #[test]
    fn rejects_execute_on_on_a_state_with_no_executor() {
        let yaml =
            supervising_machine(&canonical_states().replace("    agent: pi\n", ""), canonical_transitions());
        let err = StateMachine::from_yaml_str(&yaml).expect_err("no executor");
        assert!(err.to_string().contains("not agent-bearing"), "got: {err}");
    }

    #[test]
    fn rejects_execute_on_on_a_final_state() {
        // A `target:` rather than an `agent:`, so the earlier final-plus-agent
        // rule cannot answer first and mask the supervising rule.
        let states = canonical_states()
            .replace("    agent: pi\n", "    target: pi:openai:gpt-5\n    final: true\n");
        let err = StateMachine::from_yaml_str(&supervising_machine(&states, canonical_transitions()))
            .expect_err("final supervisor");
        assert!(err.to_string().contains("is final and cannot declare 'execute_on'"), "got: {err}");
    }

    #[test]
    fn rejects_execute_on_on_a_gating_state() {
        let states = canonical_states().replace("    execute_on: descendant-terminal\n", "    execute_on: descendant-terminal\n    gating: true\n");
        let err = StateMachine::from_yaml_str(&supervising_machine(&states, canonical_transitions()))
            .expect_err("gating supervisor");
        assert!(err.to_string().contains("is gating and cannot declare 'execute_on'"), "got: {err}");
    }

    #[test]
    fn rejects_execute_on_on_a_program_state() {
        let states = canonical_states().replace("    agent: pi\n", "    program: \"./check.sh\"\n");
        let err = StateMachine::from_yaml_str(&supervising_machine(&states, canonical_transitions()))
            .expect_err("program supervisor");
        assert!(err.to_string().contains("declares both 'program' and 'execute_on'"), "got: {err}");
    }

    /// A state is triggered by one thing, and the refusal says which two it is
    /// choosing between — "they collide" leaves the author to guess which to
    /// drop. §FS-rhei-supervision.1.2
    #[test]
    fn rejects_execute_on_on_a_poll_state() {
        // `poll` and `visits` are mutually exclusive, so the budget goes too.
        let states = canonical_states().replace(
            "    visits: 12\n",
            "    poll: { interval: 5m, max_attempts: 3 }\n",
        );
        let err = StateMachine::from_yaml_str(&supervising_machine(&states, canonical_transitions()))
            .expect_err("poll supervisor");
        assert!(
            err.to_string().contains(
                "a state has one trigger: `poll:` (time) or `execute_on:` (its subtree)"
            ),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_execute_on_combined_with_fanout() {
        let states =
            canonical_states().replace("    agent: pi\n", "    all_targets: [\"pi:openai:gpt-5\", \"codex:openai:gpt-5\"]\n");
        let err = StateMachine::from_yaml_str(&supervising_machine(&states, canonical_transitions()))
            .expect_err("fanout supervisor");
        assert!(err.to_string().contains("not a fanout"), "got: {err}");
    }

    #[test]
    fn rejects_a_supervising_state_without_a_self_loop() {
        let transitions = canonical_transitions()
            .replace("  - { from: supervising, to: supervising, description: Released }\n", "");
        let err = StateMachine::from_yaml_str(&supervising_machine(canonical_states(), &transitions))
            .expect_err("no release edge");
        assert!(err.to_string().contains("no self-loop transition"), "got: {err}");
    }

    fn supervision_warnings_for(yaml: &str) -> Vec<String> {
        let machine = StateMachine::from_yaml_str(yaml).expect("machine loads");
        let rhei =
            crate::parse("# Rhei: T\n\n## Tasks\n\n### Task 1: Root\n**State:** supervising\n")
                .expect("plan parses");
        validate_with_machine(&rhei, &machine).warnings
    }

    #[test]
    fn warns_when_no_transition_finishes_the_supervisor() {
        let transitions = canonical_transitions().replace(
            "  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }\n",
            "",
        );
        let warnings = supervision_warnings_for(&supervising_machine(canonical_states(), &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("no way to finish")),
            "expected the no-terminal-edge warning; got: {warnings:?}"
        );
    }

    #[test]
    fn warns_when_neither_visits_nor_an_exhaustion_edge_is_declared() {
        let states = canonical_states().replace("    visits: 12\n", "");
        let transitions = canonical_transitions().replace(
            "  - { from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }\n",
            "",
        );
        let warnings = supervision_warnings_for(&supervising_machine(&states, &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("no safety valve")),
            "expected the unbounded-supervisor warning; got: {warnings:?}"
        );
    }

    /// A self-loop with no budget and no counted exit is warned about.
    ///
    /// Visits of such a state are counted so an authored `visitCount` exit
    /// works; nothing counts down for a machine that authored neither.
    // §FS-rhei-supervision.4.2 §FS-rhei-states.1.3
    #[test]
    fn warns_when_a_self_loop_has_neither_a_budget_nor_a_counted_exit() {
        let states = canonical_states().replace("    visits: 12\n", "");
        let transitions = canonical_transitions().replace(
            "  - { from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }\n",
            "",
        );
        let warnings = supervision_warnings_for(&supervising_machine(&states, &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("nothing ends the loop")),
            "expected the unbounded-self-loop warning; got: {warnings:?}"
        );

        // The same machine with only the budget back is bounded again.
        let budgeted =
            supervision_warnings_for(&supervising_machine(canonical_states(), &transitions));
        assert!(
            budgeted.iter().all(|w| !w.contains("nothing ends the loop")),
            "a `visits:` budget ends the loop; got: {budgeted:?}"
        );
    }

    #[test]
    fn the_canonical_supervisor_warns_about_nothing() {
        let warnings =
            supervision_warnings_for(&supervising_machine(canonical_states(), canonical_transitions()));
        assert!(
            warnings.iter().all(|w| !w.contains("execute_on")),
            "the canonical supervisor is warning-free; got: {warnings:?}"
        );
    }

    /// An `openDescendants` edge that leads only to `cancelled` is not a way to
    /// finish.
    ///
    /// Abandonment is somewhere for the engine to take the task, which is why
    /// §FS-rhei-transitions.4.6 counts it, and it is not the supervised work
    /// being declared done, which is why this rule does not.
    // §FS-rhei-supervision.1.2 §FS-rhei-states.1.4
    #[test]
    fn warns_when_the_only_open_descendants_exit_is_cancellation() {
        let transitions = canonical_transitions().replace(
            "  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }",
            "  - { from: supervising, to: cancelled, description: Subtree closed, condition: openDescendants < 1 }",
        );
        let warnings = supervision_warnings_for(&supervising_machine(canonical_states(), &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("no way to finish")),
            "an `openDescendants` edge straight at 'cancelled' abandons the subtree rather than \
             finishing it; got: {warnings:?}"
        );
    }

    /// A supervisor whose `openDescendants` edge lands in a pocket of gating
    /// states that reaches no final state but `cancelled`.
    ///
    /// This is what pins the destination test. `gate-a` is `gating: true`, so
    /// the ordinary `from: "*"` cancel edge counts as a way out of it
    /// (§FS-rhei-transitions.4.6): a walk that asks only whether *some*
    /// `final: true` state is reachable answers yes here and goes silent, and
    /// the supervisor still cannot finish.
    // §FS-rhei-supervision.1.2 §FS-rhei-states.1.4
    fn pocketed_supervisor() -> &'static str {
        r#"
name: pocketed-supervisor
version: 1.0
states:
  supervising:
    description: Supervise the subtree
    execute_on: descendant-terminal
    agent: pi
    visits: 12
  gate-a:
    description: A gate that only leads to gate-b
    gating: true
  gate-b:
    description: A gate that only leads back to gate-a
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: gate-a, description: Subtree closed, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released }
  - { from: gate-a, to: gate-b, description: Onward }
  - { from: gate-b, to: gate-a, description: Back }
  - { from: "*", to: cancelled, description: Dropped }
profiles:
  default:
    initial: supervising
    allowed: [supervising, gate-a, gate-b, completed, cancelled]
node_policy:
  root: default
  default: default
"#
    }

    #[test]
    fn warns_when_the_open_descendants_edge_reaches_no_final_state() {
        let warnings = supervision_warnings_for(pocketed_supervisor());
        assert!(
            warnings.iter().any(|w| w.contains("no way to finish")),
            "'gate-a' and 'gate-b' reach nothing final but 'cancelled', so the supervisor \
             cannot finish; got: {warnings:?}"
        );
    }

    /// The canonical supervisor's finishing edge, the line every case below
    /// swaps for the shape it is about.
    const CANONICAL_FINISH_EDGE: &str = "  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }";

    /// The one predicate both surfaces ask, answered directly on every shape
    /// of supervisor the rule distinguishes.
    ///
    /// `rhei validate`'s warning and `rhei run`'s halt were the same one-hop
    /// test written twice, and §FS-rhei-supervision.1.2 requires them to say
    /// the same thing. They now share this function, so this is the one cheap
    /// place that agreement is pinned — the surfaces themselves are pinned end
    /// to end, which is far more expensive per shape.
    // §FS-rhei-supervision.1.2 §FS-rhei-states.1.4 §FS-rhei-transitions.4.6
    #[test]
    fn the_shared_predicate_answers_every_shape_of_supervisor() {
        let finishing = |edge: &str| {
            supervising_machine(
                canonical_states(),
                &canonical_transitions().replace(CANONICAL_FINISH_EDGE, edge),
            )
        };
        let gated = finishing(
            "  - { from: supervising, to: human-review, description: Subtree done; a human rules, condition: openDescendants < 1 }\n  - { from: human-review, to: completed, description: The ruling is recorded }",
        );
        let abandoning = finishing(
            "  - { from: supervising, to: cancelled, description: Subtree closed, condition: openDescendants < 1 }",
        );
        // The edge stays, its condition goes: 'supervising' still reaches
        // 'completed', and nothing selects that edge when the subtree closes.
        let unconditional =
            finishing("  - { from: supervising, to: completed, description: Subtree done }");
        let no_edge = supervising_machine(
            canonical_states(),
            &canonical_transitions().replace(&format!("{CANONICAL_FINISH_EDGE}\n"), ""),
        );

        for (shape, yaml, finishes) in [
            ("points straight at 'completed'", finishing(CANONICAL_FINISH_EDGE), true),
            ("reaches 'completed' through the gate 'human-review'", gated, true),
            ("points straight at 'cancelled'", abandoning, false),
            ("lands in a pocket of gating states", pocketed_supervisor().to_string(), false),
            ("reaches 'completed' by an unconditional edge", unconditional, false),
            ("declares no `openDescendants` edge at all", no_edge, false),
        ] {
            let machine = StateMachine::from_yaml_str(&yaml)
                .unwrap_or_else(|err| panic!("the machine that {shape} loads: {err}"));
            assert_eq!(
                supervising_state_can_finish(&machine, "supervising"),
                finishes,
                "a supervisor that {shape} {} finish",
                if finishes { "can" } else { "cannot" }
            );
        }
    }
