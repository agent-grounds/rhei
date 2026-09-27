    // Which edge a program's exit selects, case by case, for every shape the
    // evaluation order has to tell apart.

    /// A program state declaring `edges`, plus the two gating states and the
    /// terminal one those edges land on.
    ///
    /// The cases below pin *selection* and the classification that travels
    /// with it: whether the exit was a declared route is the same question
    /// asked of the same rule (§FS-rhei-programs.3.2), answered where the
    /// rule's `condition:` was evaluated. The e2e suite
    /// (`tests/e2e/declared_exit_route_tests.rs`) pins the same split where it
    /// is observable — in the ticket's result file.
    fn exit_route_machine(edges: &str) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(&format!(
            r#"name: routing-exit
version: 1
states:
  route:
    description: Route on the exit code
    program:
      command: ["true"]
  checked:
    description: Routed here by the declared edge
    gating: true
  build-failed:
    description: The program died
    gating: true
  completed:
    description: Done
    final: true
transitions:
{edges}
  - from: checked
    to: completed
  - from: build-failed
    to: completed
"#
        ))
        .expect("valid routing machine")
    }

    /// One leaf ticket sitting in `route`, which is all the helper reads a task
    /// for: its id, its state, and its subtree.
    fn routing_task() -> rhei_core::ast::Task {
        let rhei = rhei_core::parse(
            "# Rhei: Routing exit\n\n## Tasks\n\n### Task 1: Route\n**State:** route\n",
        )
        .expect("parse routing plan");
        rhei.tasks.into_iter().next().expect("one task")
    }

    fn selected_route(edges: &str, exit_code: i32) -> Option<ProgramExitRoute> {
        let machine = exit_route_machine(edges);
        find_program_exit_transition(&machine, None, &routing_task(), "route", exit_code)
            .expect("selection should not error")
    }

    fn selected_edge(edges: &str, exit_code: i32) -> Option<String> {
        selected_route(edges, exit_code).map(|route| route.to)
    }

    /// Whether the selected edge was a declared route, or `None` when the exit
    /// selected no edge at all. §FS-rhei-programs.3.2
    fn selected_is_declared_route(edges: &str, exit_code: i32) -> Option<bool> {
        selected_route(edges, exit_code).map(|route| route.matched.is_declared_route())
    }

    const EXACT: &str = "  - from: route\n    to: checked\n    exit_code: 3\n";
    const ARRAY: &str = "  - from: route\n    to: checked\n    exit_code: [2, 3]\n";
    const CATCH_ALL: &str = "  - from: route\n    to: build-failed\n    exit_code: nonzero\n";
    const DISQUALIFIED: &str =
        "  - from: route\n    to: checked\n    exit_code: 3\n    condition: visitCount >= 2\n";

    #[test]
    fn an_exact_integer_exit_code_selects_its_own_edge() {
        assert_eq!(selected_edge(EXACT, 3).as_deref(), Some("checked"));
        assert_eq!(selected_is_declared_route(EXACT, 3), Some(true));
    }

    #[test]
    fn an_exact_integer_array_exit_code_selects_its_own_edge() {
        assert_eq!(selected_edge(ARRAY, 3).as_deref(), Some("checked"));
        assert_eq!(selected_edge(ARRAY, 2).as_deref(), Some("checked"));
        assert_eq!(selected_edge(ARRAY, 4), None);
        assert_eq!(selected_is_declared_route(ARRAY, 3), Some(true));
        assert_eq!(selected_is_declared_route(ARRAY, 2), Some(true));
    }

    #[test]
    fn a_catch_all_alone_selects_the_failure_edge() {
        assert_eq!(selected_edge(CATCH_ALL, 3).as_deref(), Some("build-failed"));
        // The program did not choose the code, so the engine still owes the
        // account of it. §FS-rhei-programs.3.2
        assert_eq!(selected_is_declared_route(CATCH_ALL, 3), Some(false));
    }

    #[test]
    fn an_exact_edge_beats_a_catch_all_declared_beside_it() {
        let both = format!("{CATCH_ALL}{EXACT}");
        assert_eq!(selected_edge(&both, 3).as_deref(), Some("checked"));
        // A code the exact rule does not name still falls to the catch-all, so
        // the precedence is per exit code rather than per state.
        assert_eq!(selected_edge(&both, 4).as_deref(), Some("build-failed"));
        // The entry follows the edge that fired, not the edges declared.
        // §FS-rhei-programs.3.2
        assert_eq!(selected_is_declared_route(&both, 3), Some(true));
        assert_eq!(selected_is_declared_route(&both, 4), Some(false));
    }

    /// The case a fix that re-derived the classification from the machine would
    /// get wrong: the exact rule matches the code, its `condition:` is false, so
    /// the catch-all is what fired. Asking the machine afterwards whether an
    /// exact rule from `route` to `build-failed` exists cannot reproduce that —
    /// the answer lives in the selection, not in the machine.
    #[test]
    fn an_exact_edge_its_condition_disqualified_leaves_the_catch_all_to_fire() {
        let both = format!("{DISQUALIFIED}{CATCH_ALL}");
        assert_eq!(selected_edge(&both, 3).as_deref(), Some("build-failed"));
        // The assertion that would fail if the classification were ever
        // re-derived from the machine instead of carried from the selection.
        // §FS-rhei-programs.3.2
        assert_eq!(selected_is_declared_route(&both, 3), Some(false));
    }

    /// The ticket's pair, read at the selection boundary. The leading rule
    /// declares no `exit_code:` at all, so step 1 never collects it; the rule
    /// the exit code chose is the second one declared for the pair. Which of
    /// the two it was has to leave the function, because `(route, checked)`
    /// names both of them. §FS-rhei-programs.3.2 §FS-rhei-transitions.4.4
    #[test]
    fn a_selection_behind_a_leading_conditional_rule_carries_the_rule_it_chose() {
        let leading = "  - from: route\n    to: checked\n    condition: visitCount >= 1\n";
        let both = format!("{leading}{EXACT}");
        let route = selected_route(&both, 3).expect("the exit-coded edge is selected");
        assert_eq!(route.to, "checked");
        assert_eq!(
            route.selected_rule, 1,
            "the exit code chose the second rule declared for the pair; the first \
             is not a candidate at all"
        );
    }

    /// The shape every poll fixture has: one `poll:` state whose self-loop and
    /// whose exhaustion edge declare the *same* exact code, so which of them
    /// fires is decided by the attempt budget and never by the exit.
    // §FS-rhei-run.5.1
    fn poll_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"name: polling-exit
version: 1
states:
  waiting:
    description: Wait for the gate
    program:
      command: ["true"]
    poll:
      interval: 0s
      max_attempts: 2
  exhausted:
    description: The budget ran out
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: exhausted
    exit_code: 75
"#,
        )
        .expect("valid polling machine")
    }

    /// One leaf ticket sitting in `waiting`, on its `visits`th attempt.
    fn polling_task(visits: u64) -> (rhei_core::ast::Task, Metadata) {
        let rhei = rhei_core::parse(
            "# Rhei: Polling exit\n\n## Tasks\n\n### Task 1: Wait\n**State:** waiting\n",
        )
        .expect("parse polling plan");
        let task = rhei.tasks.into_iter().next().expect("one task");
        // The id is a bare number here, and frontmatter keys a single-segment
        // numeric id numerically, so quoting it would miss the lookup.
        let metadata = serde_yaml::from_str(&format!(
            "metadata:\n  tasks:\n    {}:\n      stateVisits:\n        waiting: {visits}\n",
            task.id
        ))
        .expect("visit metadata parses");
        (task, metadata)
    }

    /// A spent poll budget picks the exhaustion edge *regardless of*
    /// `exit_code:` (§FS-rhei-programs.3.2), so the exit named nothing and the
    /// engine still owes the account of it — the same edge, taken while the
    /// budget still had an attempt, is the self-loop instead. Reading the rule
    /// alone would call this a declared route and silently drop the engine's
    /// message on every poll machine whose exhaustion edge names a code.
    #[test]
    fn a_spent_poll_budget_selects_its_edge_rather_than_the_exit_choosing_it() {
        let machine = poll_machine();
        let (task, metadata) = polling_task(2);
        let route =
            find_program_exit_transition(&machine, Some(&metadata), &task, "waiting", 75)
                .expect("selection should not error")
                .expect("the exhaustion edge is selected");
        assert_eq!(route.to, "exhausted");
        assert!(!route.matched.is_declared_route());
        // Carrying the rule's identity does not move which rule a spent budget
        // picks: it is the exhaustion edge, the second rule declared here.
        // §FS-rhei-run.5.1
        assert_eq!(route.selected_rule, 1);
    }

    /// The budget is not spent, so the self-loop is still open and the attempt
    /// is a wait rather than a route out. §FS-rhei-run.5.1
    #[test]
    fn a_poll_with_an_attempt_left_stays_on_its_self_loop() {
        let machine = poll_machine();
        let (task, metadata) = polling_task(1);
        let route =
            find_program_exit_transition(&machine, Some(&metadata), &task, "waiting", 75)
                .expect("selection should not error")
                .expect("the self-loop is selected");
        assert_eq!(route.to, "waiting");
        // The self-loop is the first rule declared, and an unspent budget
        // leaves that unchanged too. §FS-rhei-run.5.1
        assert_eq!(route.selected_rule, 0);
    }
