    // How far down a supervisor hears, and where a move it declines goes next.
    //
    // Its own part because the delivery tests next door hold the scope fixed at
    // `descendant` and vary the event; these hold the event fixed and vary the
    // scope, which needs a machine with two supervising states and a plan four
    // levels deep.

    // §AR-source-file-size.3 §FS-rhei-supervision.1.1 §FS-rhei-supervision.2.2

    /// A machine with one supervising state per scope, so one plan can put a
    /// `child`-scoped supervisor under a `descendant`-scoped one.
    fn scope_machine() -> rhei_validator::StateMachine {
        machine_with_states(
            r#"name: supervision-scope
version: 1
states:
  supervising:
    description: Watches everything beneath it
    execute_on: descendant-terminal
    agent: pi
    visits: 12
  watching:
    description: Watches its own children finish
    execute_on: child-terminal
    agent: pi
    visits: 12
  hopping:
    description: Watches its own children hop
    execute_on: child-transition
    agent: pi
    visits: 12
  review:
    description: Review
    agent: pi
  human-review:
    description: Human call
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released }
  - { from: watching, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: watching, to: watching, description: Released }
  - { from: hopping, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: hopping, to: hopping, description: Released }
  - { from: review, to: human-review, description: Escalated }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }
"#,
        )
    }

    /// Deliver one applied transition under [`scope_machine`].
    fn deliver_scoped(
        plan: &rhei_core::ast::Rhei,
        local_id: &str,
        from: &str,
        to: &str,
    ) -> Option<Metadata> {
        let machine = scope_machine();
        let target = parse_task_id(local_id);
        let task = find_task_by_id(&plan.tasks, &target).expect("task in plan");
        let ancestors: Vec<rhei_core::ast::Task> =
            ancestor_chain(&plan.tasks, &target).into_iter().cloned().collect();
        let supervising_owner = nearest_in_scope_supervising_owner(&machine, &ancestors);
        apply_supervision_transition(
            plan.metadata.as_ref(),
            SupervisionTransition {
                machine: &machine,
                task,
                supervising_owner,
                metadata_key: &target,
                metadata_prefix: "",
                local_id,
                from,
                to,
                to_visit: 1,
                operation_supervisor: None,
            },
        )
    }

    /// `Task 1` in `state`, over a child that has a child of its own.
    fn three_level_plan(state: &str) -> rhei_core::ast::Rhei {
        rhei_core::parse(&format!(
            "# Rhei: Scoped\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
             ### Task 1: Top\n**State:** {state}\n\n\
             #### Task 1.1: Child\n**State:** review\n\n\
             ##### Task 1.1.1: Grandchild\n**State:** review\n"
        ))
        .expect("parse scoped plan")
    }

    /// A `child-terminal` supervisor hears its own child finish, nothing deeper.
    ///
    /// The grandchild's exit is not merely uninteresting to it — with no other
    /// supervising ancestor it reaches nobody, and the transition is ordinary.
    // §FS-rhei-supervision.1.1
    #[test]
    fn a_child_scoped_supervisor_hears_its_child_and_not_its_grandchild() {
        let plan = three_level_plan("watching");
        let top = parse_task_id("1");

        let own_child =
            deliver_scoped(&plan, "1.1", "review", "completed").expect("its own child is in scope");
        assert_eq!(
            supervision_checkpoints(Some(&own_child), &top),
            vec![checkpoint("1.1", "review", "completed", 1)]
        );

        assert!(
            deliver_scoped(&plan, "1.1.1", "review", "completed").is_none(),
            "a grandchild is outside a `child-terminal` scope, and nobody above claims it"
        );
    }

    /// §FS-rhei-supervision.1.1: a non-leaf child is terminal only once its own
    /// subtree is, so `child-terminal` costs exactly one visit per finished
    /// child subtree — however many steps that subtree took.
    #[test]
    fn child_terminal_wakes_once_per_finished_child_subtree() {
        let plan = three_level_plan("watching");
        let top = parse_task_id("1");

        // Everything the child's own subtree does is silent…
        for (from, to) in [("review", "human-review"), ("human-review", "completed")] {
            assert!(
                deliver_scoped(&plan, "1.1.1", from, to).is_none(),
                "the grandchild's {from} -> {to} must not wake the supervisor"
            );
        }
        assert!(
            deliver_scoped(&plan, "1.1", "review", "human-review").is_none(),
            "the child's own non-terminal hop is not a `*-terminal` checkpoint"
        );

        // …until the child itself is terminal, which is one checkpoint.
        let finished = deliver_scoped(&plan, "1.1", "human-review", "completed")
            .expect("the finished child subtree is the checkpoint");
        assert_eq!(
            supervision_checkpoints(Some(&finished), &top),
            vec![checkpoint("1.1", "human-review", "completed", 1)]
        );
    }

    /// §FS-rhei-supervision.2.2: a move a `child-*` supervisor declines climbs
    /// to the next ancestor whose scope reaches it, and stops at the first
    /// ancestor that takes it.
    #[test]
    fn a_declined_move_climbs_to_the_next_ancestor_whose_scope_reaches_it() {
        let plan = rhei_core::parse(
            "# Rhei: Nested\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n\
             ### Task 1: Top\n**State:** supervising\n\n\
             #### Task 1.1: Middle\n**State:** watching\n\n\
             ##### Task 1.1.1: Inner\n**State:** review\n\n\
             ###### Task 1.1.1.1: Leaf\n**State:** review\n",
        )
        .expect("parse nested plan");
        let top = parse_task_id("1");
        let middle = parse_task_id("1.1");

        // Two levels below the `child-terminal` supervisor: it declines, and
        // the outer `descendant-terminal` one is where the event lands.
        let climbed = deliver_scoped(&plan, "1.1.1.1", "review", "completed")
            .expect("the outer supervisor's scope reaches this deep");
        assert_eq!(
            supervision_checkpoints(Some(&climbed), &top),
            vec![checkpoint("1.1.1.1", "review", "completed", 1)]
        );
        assert!(
            supervision_checkpoints(Some(&climbed), &middle).is_empty(),
            "a `child-terminal` supervisor is not woken by its grandchildren"
        );

        // Its own child, though, stops at it: the nearest in-scope ancestor is
        // the only one that hears a move.
        let nearest = deliver_scoped(&plan, "1.1.1", "review", "completed")
            .expect("its own child is in scope");
        assert_eq!(
            supervision_checkpoints(Some(&nearest), &middle),
            vec![checkpoint("1.1.1", "review", "completed", 1)]
        );
        assert!(
            supervision_checkpoints(Some(&nearest), &top).is_empty(),
            "the event stops at the first ancestor whose scope includes the task"
        );
    }

    /// §FS-rhei-supervision.1.1: `child-transition` watches a child's own hops
    /// — its review/fix loop — without hearing what that child dispatches.
    #[test]
    fn child_transition_hears_a_childs_hop_but_never_a_grandchilds() {
        let plan = three_level_plan("hopping");
        let top = parse_task_id("1");

        let hop = deliver_scoped(&plan, "1.1", "review", "human-review")
            .expect("a child's own hop is a checkpoint");
        assert_eq!(
            supervision_checkpoints(Some(&hop), &top),
            vec![checkpoint("1.1", "review", "human-review", 1)]
        );

        assert!(
            deliver_scoped(&plan, "1.1.1", "review", "human-review").is_none(),
            "a grandchild's hop is outside the scope"
        );
        assert!(
            deliver_scoped(&plan, "1.1.1", "review", "completed").is_none(),
            "and so is its terminal exit: scope decides whose moves, not which"
        );
    }

    /// Scope narrows what *wakes* a supervisor, never what it is responsible for.
    ///
    /// A `child-terminal` supervisor is the barrier over its whole subtree: the
    /// grandchild it never hears about is still held while a visit is owed, and
    /// still runs freely once the subtree is released.
    // §FS-rhei-supervision.3.1
    #[test]
    fn a_child_scoped_supervisor_still_holds_its_whole_subtree() {
        let ready = |plan: &rhei_core::ast::Rhei| -> Vec<String> {
            let machines = rhei_validator::MachineSet::single(scope_machine());
            let dir = tempfile::tempdir().expect("tmpdir");
            find_ready_tasks(plan, &machines, &ReadySetRoots::plan_only(dir.path()), &HashSet::new())
            .iter()
            .map(|task| task.id.to_string())
            .collect()
        };

        let mut plan = three_level_plan("watching");
        assert_eq!(
            ready(&plan),
            vec!["1".to_string()],
            "the out-of-scope grandchild is held like everything else beneath it"
        );

        plan.metadata = Some(record_supervision_release(None, &parse_task_id("1")));
        assert_eq!(
            ready(&plan),
            vec!["1.1.1".to_string()],
            "and runs freely between visits, without waking the supervisor"
        );
    }
