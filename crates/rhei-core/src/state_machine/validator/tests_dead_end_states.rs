    // Grouped so one `cargo test ... dead_end` filter reaches these and the
    // end-to-end file that pins the same rule through the CLI.
    mod dead_end_states {
        use super::*;

        // A machine declaring a state nothing can leave is refused at load:
        // §FS-rhei-states.1.3 machine-wide, §FS-rhei-states.8.2 per profile, with
        // §FS-rhei-transitions.4.6 deciding when a wildcard edge is a way out.

        // The refusal a machine with a stranded state earns, as a message. A
        // loaded machine is reported by name rather than by `expect_err`, which
        // prints the whole struct and buries the one fact that matters.
        fn dead_end_refusal(yaml: &str, why: &str) -> String {
            match StateMachine::from_yaml_str(yaml) {
                Ok(machine) => panic!("{why}, but '{}' loaded clean", machine.name),
                Err(err) => err.to_string(),
            }
        }

        // The report: `hold` has no `from: hold` rule, so the only rule matching it
        // is the machine-wide wildcard, which `rhei run` never takes.
        #[test]
        fn rejects_a_state_left_only_by_a_wildcard_to_a_final_state() {
            let yaml = r#"
    name: wildcard-to-final
    version: 1.0
    states:
      draft: { description: Write }
      hold: { description: Nothing declares an edge out of here }
      completed: { description: Done, final: true }
      cancelled: { description: Abandoned, final: true }
    transitions:
      - from: draft
        to: hold
      - from: draft
        to: completed
      - from: "*"
        to: cancelled
    "#;

            let err = dead_end_refusal(yaml, "'hold' has no way out");

            assert!(err.contains("hold"), "the error must name the state: {err}");
            assert!(
                err.contains("cancelled"),
                "the error must say which wildcard target it will not count: {err}"
            );
            assert!(err.contains("from: hold"), "the error must print the edge to add: {err}");
        }

        // The same shape, one flag different: the wildcard lands on a non-final
        // state, which the engine does take, so the machine loads.
        #[test]
        fn accepts_a_state_left_only_by_a_wildcard_to_a_non_final_state() {
            let yaml = r#"
    name: wildcard-to-non-final
    version: 1.0
    states:
      hold: { description: Left only by the wildcard }
      review: { description: Inspect }
      completed: { description: Done, final: true }
    transitions:
      - from: review
        to: completed
      - from: "*"
        to: review
    "#;

            StateMachine::from_yaml_str(yaml).expect("a wildcard to a non-final state is a way out");
        }

        // An explicit edge is taken whatever its target, so a state its author
        // means to end only in cancellation says so with `from:` and is valid.
        #[test]
        fn accepts_an_explicit_edge_to_a_final_state_including_cancellation() {
            let yaml = r#"
    name: explicit-to-cancelled
    version: 1.0
    states:
      draft: { description: Write }
      abandon-only: { description: Ends only in cancellation and says so }
      completed: { description: Done, final: true }
      cancelled: { description: Abandoned, final: true }
    transitions:
      - from: draft
        to: completed
      - from: draft
        to: abandon-only
      - from: abandon-only
        to: cancelled
    "#;

            StateMachine::from_yaml_str(yaml).expect("an explicit edge to a final state is a way out");
        }

        // Not a one-hop check: each state in the loop has an edge, and together
        // they reach nothing.
        #[test]
        fn rejects_a_chain_of_states_that_together_reach_no_final_state() {
            let yaml = r#"
    name: closed-loop
    version: 1.0
    states:
      pending: { description: Ready }
      draft: { description: Write }
      review: { description: Inspect }
      rework: { description: Revise }
      completed: { description: Done, final: true }
    transitions:
      - from: pending
        to: completed
      - from: draft
        to: review
      - from: review
        to: rework
      - from: rework
        to: review
    "#;

            let err = dead_end_refusal(yaml, "the loop reaches no final state");

            for state in ["draft", "review", "rework"] {
                assert!(err.contains(state), "the error must name '{state}': {err}");
            }
        }

        // One error for the machine, not one per run: a load returns a single
        // `Err`, and its message accounts for every stranded state at once.
        #[test]
        fn one_error_names_every_state_that_cannot_be_left() {
            let yaml = r#"
    name: two-dead-ends
    version: 1.0
    states:
      draft: { description: Write }
      hold-a: { description: No way out }
      hold-b: { description: No way out either }
      completed: { description: Done, final: true }
      cancelled: { description: Abandoned, final: true }
    transitions:
      - from: draft
        to: completed
      - from: draft
        to: hold-a
      - from: draft
        to: hold-b
      - from: "*"
        to: cancelled
    "#;

            let err = dead_end_refusal(yaml, "both 'hold-a' and 'hold-b' are stranded");

            assert!(err.contains("hold-a"), "the one error must name 'hold-a': {err}");
            assert!(err.contains("hold-b"), "the one error must name 'hold-b': {err}");
        }

        // A gating state is left by a human with `rhei transition`, which honours
        // every declared edge, so its wildcard cancel is a way out — and a state
        // whose only edge leads into it still reaches a final state through it.
        // This is the shape of `examples/ui-test-canonical-example`, which must
        // keep loading unedited.
        #[test]
        fn accepts_a_gating_state_left_only_by_a_wildcard_cancel() {
            let yaml = r#"
    name: gating-cancel-only
    version: 1.0
    states:
      script-fail: { description: Fails into the gate }
      blocked: { description: Stop here for a human, gating: true }
      completed: { description: Done, final: true }
      cancelled: { description: Abandoned, final: true }
    transitions:
      - from: script-fail
        to: blocked
      - from: "*"
        to: cancelled
    "#;

            StateMachine::from_yaml_str(yaml).expect("a gating state's wildcard cancel is a way out");
        }

        // §FS-rhei-states.8.2 asks the machine-wide question of one profile's
        // narrowed set, and answers it about wildcards the same way: the full graph
        // is sound, and the profile still cannot leave `pending`.
        #[test]
        fn rejects_a_profile_whose_only_way_out_is_a_wildcard_to_a_final_state() {
            let yaml = r#"
    name: narrowed-to-wildcard
    version: 3.0
    states:
      pending: { description: Work }
      review: { description: Inspect }
      completed: { description: Done, final: true }
      cancelled: { description: Abandoned, final: true }
    transitions:
      - from: pending
        to: review
      - from: review
        to: completed
      - from: "*"
        to: cancelled
    profiles:
      narrow:
        initial: pending
        allowed: [pending, cancelled]
    node_policy:
      root: narrow
      default: narrow
    "#;

            let err = dead_end_refusal(yaml, "profile 'narrow' cannot leave 'pending'");

            assert!(err.contains("narrow"), "the error must name the profile: {err}");
            assert!(err.contains("pending"), "the error must name the state: {err}");
        }

        // A guard on the other half of the same predicate: an `allowed` set that
        // narrows away the only path out was already rejected, and stays rejected.
        #[test]
        fn rejects_a_profile_that_narrows_away_the_only_path_out() {
            let yaml = r#"
    name: narrowed-away
    version: 3.0
    states:
      pending: { description: Work }
      review: { description: Inspect }
      completed: { description: Done, final: true }
    transitions:
      - from: pending
        to: review
      - from: review
        to: completed
    profiles:
      narrow:
        initial: pending
        allowed: [pending, completed]
    node_policy:
      root: narrow
      default: narrow
    "#;

            let err = dead_end_refusal(yaml, "profile 'narrow' narrows away the only path out");

            assert!(err.contains("narrow"), "the error must name the profile: {err}");
            assert!(err.contains("pending"), "the error must name the state: {err}");
        }

        // The profile cut the path two hops on, so `pending`'s own edge is fine
        // and the repair belongs in the profile. Naming `pending` would send
        // the reader to the machine file to add an edge it already has.
        #[test]
        fn names_the_state_a_profile_cut_off_rather_than_the_one_the_walk_began_at() {
            let yaml = r#"
    name: narrowed-downstream
    version: 3.0
    states:
      pending: { description: Work }
      review: { description: Inspect }
      completed: { description: Done, final: true }
      cancelled: { description: Abandoned, final: true }
    transitions:
      - from: pending
        to: review
      - from: review
        to: completed
    profiles:
      narrow:
        initial: pending
        allowed: [pending, review, cancelled]
    node_policy:
      root: narrow
      default: narrow
    "#;

            let err = dead_end_refusal(yaml, "profile 'narrow' narrows away 'completed'");

            assert!(
                err.contains("dies at 'review'"),
                "the error must name where the walk died, not where it began: {err}"
            );
            assert!(
                err.contains("from: review"),
                "the edge to add belongs to the state the profile cut off: {err}"
            );
            assert!(
                err.contains("`allowed`"),
                "the repair is in the profile, so the error must offer to widen it: {err}"
            );
            assert!(
                !err.contains("from: pending"),
                "'pending' already has an edge; offering to add one sends the reader to the \
                 wrong file: {err}"
            );
        }

        // A self-loop re-reaches only the state the walk started at, which the
        // reachability walk omits. The listing had nothing to join, so the
        // error read "leads only to , and none of those is final".
        #[test]
        fn says_a_self_looping_state_leads_back_to_itself_rather_than_listing_nothing() {
            let yaml = r#"
    name: selfloop
    version: 1.0
    states:
      pending: { description: Ready }
      hold: { description: Loops on itself and nowhere else }
      completed: { description: Done, final: true }
    transitions:
      - from: pending
        to: hold
      - from: hold
        to: hold
      - from: pending
        to: completed
    "#;

            let err = dead_end_refusal(yaml, "'hold' only ever reaches itself");

            assert!(
                err.contains("leads back to itself"),
                "the error must say the loop is the reason: {err}"
            );
            assert!(
                !err.contains("leads only to ,"),
                "the error must never print an empty listing: {err}"
            );
            assert!(err.contains("from: hold"), "the error must print the edge to add: {err}");
        }
    }
