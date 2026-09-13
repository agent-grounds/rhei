    // §FS-rhei-supervision: the operand, the metadata block, and the hold and
    // release the shared transition path maintains.

    /// The canonical supervisor machine of §FS-rhei-supervision.7, trimmed to
    /// what these tests exercise.
    fn supervision_machine() -> rhei_validator::StateMachine {
        machine_with_states(
            r#"name: supervision
version: 1
states:
  supervising:
    description: Supervise
    execute_on: descendant-terminal
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
  - { from: supervising, to: human-review, description: Budget spent, condition: visitCount >= visits }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }
"#,
        )
    }

    fn supervised_plan(child_states: &[&str]) -> rhei_core::ast::Rhei {
        let mut plan = String::from(
            "# Rhei: Supervised\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n### Task 1: Parent\n**State:** supervising\n",
        );
        for (index, state) in child_states.iter().enumerate() {
            plan.push_str(&format!(
                "\n#### Task 1.{}: Child {}\n**State:** {}\n",
                index + 1,
                index + 1,
                state
            ));
        }
        rhei_core::parse(&plan).expect("parse supervised plan")
    }

    #[test]
    fn open_descendants_counts_every_non_terminal_node_below_a_task() {
        let machine = supervision_machine();
        let rhei = supervised_plan(&["completed", "review", "cancelled"]);
        let parent = &rhei.tasks[0];
        assert_eq!(open_descendant_count(parent, &machine), 1);
        assert_eq!(open_descendant_count(&parent.children[0], &machine), 0);
    }

    /// §FS-rhei-supervision.4.1: `openDescendants` selects the supervisor's
    /// terminal edge, and only once the subtree is closed.
    #[test]
    fn the_open_descendants_operand_selects_the_supervisors_terminal_edge() {
        let machine = supervision_machine();
        let terminal_edge = machine
            .transitions()
            .iter()
            .find(|rule| rule.from.0 == "supervising" && rule.to.0 == "completed")
            .expect("terminal edge declared");

        let open = supervised_plan(&["completed", "review"]);
        let parent = &open.tasks[0];
        assert!(!transition_rule_is_applicable(
            terminal_edge,
            &machine,
            None,
            &parent.id,
            Some(parent),
            "supervising",
            "supervising",
        )
        .expect("condition evaluates"));

        let closed = supervised_plan(&["completed", "cancelled"]);
        let parent = &closed.tasks[0];
        assert!(transition_rule_is_applicable(
            terminal_edge,
            &machine,
            None,
            &parent.id,
            Some(parent),
            "supervising",
            "supervising",
        )
        .expect("condition evaluates"));
    }

    /// The operand is available from any state, not only a supervising one.
    // §FS-rhei-supervision.4.1
    fn evaluate_open_descendants(condition: &str, task: &rhei_core::ast::Task) -> bool {
        evaluate_transition_condition(
            condition,
            None,
            &task.id,
            Some(task),
            "review",
            "review",
            &supervision_machine(),
        )
        .expect("condition evaluates")
    }

    #[test]
    fn the_open_descendants_operand_reads_from_a_non_supervising_state_too() {
        let rhei = supervised_plan(&["review"]);
        assert!(evaluate_open_descendants("openDescendants >= 1", &rhei.tasks[0]));
        assert!(evaluate_open_descendants("openDescendants < 1", &rhei.tasks[0].children[0]));
    }

    #[test]
    fn the_open_descendants_operand_says_so_when_no_subtree_is_in_hand() {
        let machine = supervision_machine();
        let err = evaluate_transition_condition(
            "openDescendants < 1",
            None,
            &parse_task_id("1"),
            None,
            "supervising",
            "supervising",
            &machine,
        )
        .expect_err("no task node");
        assert!(err.to_string().contains("openDescendants"), "got: {err}");
    }

    // -----------------------------------------------------------------------
    // The `supervision` metadata block
    // -----------------------------------------------------------------------

    fn checkpoint(task: &str, from: &str, to: &str, visit: u64) -> SupervisionCheckpoint {
        SupervisionCheckpoint {
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            visit,
        }
    }

    /// §FS-rhei-supervision.3.3: `held` on entry, `held` plus an appended
    /// record on every checkpoint, `released` with the list cleared on the
    /// self-loop.
    #[test]
    fn the_supervision_block_accumulates_checkpoints_and_clears_them_on_release() {
        let id = parse_task_id("1");
        assert_eq!(supervision_phase(None, &id), SupervisionPhase::Held);
        assert!(supervision_checkpoints(None, &id).is_empty());

        let entered = record_supervision_hold(None, &id, None);
        assert_eq!(supervision_phase(Some(&entered), &id), SupervisionPhase::Held);
        assert!(supervision_checkpoints(Some(&entered), &id).is_empty());

        let released = record_supervision_release(Some(&entered), &id);
        assert_eq!(supervision_phase(Some(&released), &id), SupervisionPhase::Released);

        let first =
            record_supervision_hold(Some(&released), &id, Some(&checkpoint("1.1", "review", "completed", 1)));
        let second =
            record_supervision_hold(Some(&first), &id, Some(&checkpoint("1.2", "fix", "completed", 2)));
        assert_eq!(supervision_phase(Some(&second), &id), SupervisionPhase::Held);
        assert_eq!(
            supervision_checkpoints(Some(&second), &id),
            vec![
                checkpoint("1.1", "review", "completed", 1),
                checkpoint("1.2", "fix", "completed", 2),
            ]
        );

        let consumed = record_supervision_release(Some(&second), &id);
        assert!(supervision_checkpoints(Some(&consumed), &id).is_empty());
    }

    #[test]
    fn leaving_a_supervising_state_by_any_other_edge_removes_the_block() {
        let id = parse_task_id("1");
        let held = record_supervision_hold(None, &id, Some(&checkpoint("1.1", "review", "completed", 1)));
        let cleared = clear_supervision_for_task(Some(&held), &id).expect("metadata survives");
        assert!(supervision_map(Some(&cleared), &id).is_none());
        // `rhei reset` clears every task's block the same way.
        let held = record_supervision_hold(None, &id, None);
        let reset = clear_runtime_supervision(Some(&held)).expect("metadata survives");
        assert!(supervision_map(Some(&reset), &id).is_none());
    }

    // -----------------------------------------------------------------------
    // Checkpoint delivery
    // -----------------------------------------------------------------------

    /// Deliver one applied transition and report the resulting metadata.
    fn deliver_with_supervisor(
        plan: &rhei_core::ast::Rhei,
        local_id: &str,
        from: &str,
        to: &str,
        operation_supervisor: Option<&TaskId>,
    ) -> Option<Metadata> {
        let machine = supervision_machine();
        let target = parse_task_id(local_id);
        let task = find_task_by_id(&plan.tasks, &target).expect("task in plan");
        let ancestors: Vec<rhei_core::ast::Task> =
            ancestor_chain(&plan.tasks, &target).into_iter().cloned().collect();
        apply_supervision_transition(
            plan.metadata.as_ref(),
            SupervisionTransition {
                machine: &machine,
                task,
                ancestors: &ancestors,
                metadata_key: &target,
                metadata_prefix: "",
                local_id,
                from,
                to,
                to_visit: 1,
                operation_supervisor,
            },
        )
    }

    fn deliver(
        plan: &rhei_core::ast::Rhei,
        local_id: &str,
        from: &str,
        to: &str,
    ) -> Option<Metadata> {
        deliver_with_supervisor(plan, local_id, from, to, None)
    }

    /// §FS-rhei-supervision.2.1: under `execute_on: descendant-terminal` only a terminal entry
    /// is a checkpoint, and the nearest supervisor is the one that hears it.
    #[test]
    fn a_terminal_descendant_checkpoints_its_nearest_supervisor() {
        let plan = supervised_plan(&["review", "review"]);
        let parent = parse_task_id("1");

        let non_terminal = deliver(&plan, "1.1", "review", "review");
        assert!(
            non_terminal.is_none(),
            "a non-terminal hop is not a `execute_on: descendant-terminal` checkpoint"
        );

        let delivered = deliver(&plan, "1.1", "review", "completed").expect("checkpoint delivered");
        assert_eq!(supervision_phase(Some(&delivered), &parent), SupervisionPhase::Held);
        assert_eq!(
            supervision_checkpoints(Some(&delivered), &parent),
            vec![checkpoint("1.1", "review", "completed", 1)]
        );
    }

    /// §FS-rhei-supervision.3.1: the supervisor's own self-loop releases; every
    /// other exit clears the block.
    #[test]
    fn the_supervisors_own_edges_move_its_phase() {
        let plan = supervised_plan(&["completed"]);
        let parent = parse_task_id("1");

        let released = deliver(&plan, "1", "supervising", "supervising").expect("release recorded");
        assert_eq!(supervision_phase(Some(&released), &parent), SupervisionPhase::Released);

        let held = record_supervision_hold(None, &parent, None);
        let mut with_block = plan.clone();
        with_block.metadata = Some(held);
        let finished =
            deliver(&with_block, "1", "supervising", "completed").expect("block removed on exit");
        assert!(supervision_map(Some(&finished), &parent).is_none());
    }

    /// §FS-rhei-supervision.3.1 rule 4: an exit into a `gating: true` state
    /// keeps the block, so the subtree stays held until a human moves the
    /// supervisor on — and moving it on is what clears the block.
    #[test]
    fn an_exit_to_a_human_gate_keeps_the_hold_until_the_human_moves_it() {
        let plan = supervised_plan(&["review"]);
        let parent = parse_task_id("1");

        let mut exhausted = plan.clone();
        exhausted.metadata = Some(record_supervision_hold(None, &parent, None));
        let parked = deliver(&exhausted, "1", "supervising", "human-review")
            .expect("the block survives the move");
        assert_eq!(supervision_phase(Some(&parked), &parent), SupervisionPhase::Held);
        assert!(
            supervision_checkpoints(Some(&parked), &parent).is_empty(),
            "the visit that took this edge already consumed its checkpoints"
        );

        // The human moves it on: the hold ends wherever it goes.
        let mut at_gate = plan.clone();
        at_gate.tasks[0].state = "human-review".to_string();
        at_gate.metadata = Some(parked);
        let released =
            deliver(&at_gate, "1", "human-review", "cancelled").expect("block removed");
        assert!(supervision_map(Some(&released), &parent).is_none());
    }

    /// §FS-rhei-supervision.2.2: a supervisor's self-loop is the supervisor
    /// waiting, so it is never news for an ancestor of its own.
    #[test]
    fn a_supervisors_self_loop_is_not_a_checkpoint_for_its_own_ancestor() {
        let plan = rhei_core::parse(
            "# Rhei: Nested\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n### Task 1: Top\n**State:** supervising\n\n#### Task 1.1: Middle\n**State:** supervising\n\n##### Task 1.1.1: Leaf\n**State:** review\n",
        )
        .expect("parse nested plan");
        let top = parse_task_id("1");

        let released = deliver(&plan, "1.1", "supervising", "supervising").expect("release recorded");
        assert!(
            supervision_checkpoints(Some(&released), &top).is_empty(),
            "the inner supervisor's release must not wake the outer one"
        );

        // Its terminal exit, though, is an ordinary descendant finishing.
        let finished = deliver(&plan, "1.1", "supervising", "completed").expect("checkpoint delivered");
        assert_eq!(
            supervision_checkpoints(Some(&finished), &top),
            vec![checkpoint("1.1", "supervising", "completed", 1)]
        );
    }

    /// §FS-rhei-supervision.2.2: the nearest supervising ancestor hears it, and
    /// only that one.
    #[test]
    fn only_the_nearest_supervising_ancestor_hears_a_checkpoint() {
        let plan = rhei_core::parse(
            "# Rhei: Nested\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n### Task 1: Top\n**State:** supervising\n\n#### Task 1.1: Middle\n**State:** supervising\n\n##### Task 1.1.1: Leaf\n**State:** review\n",
        )
        .expect("parse nested plan");
        let delivered =
            deliver(&plan, "1.1.1", "review", "completed").expect("checkpoint delivered");
        assert_eq!(
            supervision_checkpoints(Some(&delivered), &parse_task_id("1.1")),
            vec![checkpoint("1.1.1", "review", "completed", 1)]
        );
        assert!(supervision_checkpoints(Some(&delivered), &parse_task_id("1")).is_empty());
    }

    /// §FS-rhei-supervision.2.1: a cancel the supervisor issues during its own
    /// visit is not news it has to be woken for.
    #[test]
    fn a_move_the_supervisor_itself_makes_is_not_a_checkpoint() {
        let plan = rhei_core::parse(
            "# Rhei: Claimed\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n### Task 1: Parent\n**State:** supervising\n**Assignee:** pi\n\n#### Task 1.1: Child\n**State:** review\n",
        )
        .expect("parse claimed plan");
        assert!(
            deliver(&plan, "1.1", "review", "cancelled").is_none(),
            "the supervisor holds the claim, so it already knows"
        );
    }

    /// §FS-rhei-supervision.2.1: an autonomous supervisor carries its identity
    /// only on the explicit descendant operation; the same move without that
    /// context is ordinary news from the descendant.
    #[test]
    fn explicit_supervisor_operation_context_suppresses_only_its_own_checkpoint() {
        let plan = supervised_plan(&["review"]);
        let parent = parse_task_id("1");

        assert!(
            deliver_with_supervisor(
                &plan,
                "1.1",
                "review",
                "cancelled",
                Some(&parent),
            )
            .is_none(),
            "the supervisor already knows about the move it explicitly issued"
        );

        let ordinary = deliver(&plan, "1.1", "review", "cancelled")
            .expect("the same descendant move without context is a checkpoint");
        assert_eq!(
            supervision_checkpoints(Some(&ordinary), &parent),
            vec![checkpoint("1.1", "review", "cancelled", 1)]
        );

    }

    /// §FS-rhei-supervision.2.1: `execute_on: descendant-transition` hears every hop.
    #[test]
    fn descendant_transition_checkpoints_every_hop() {
        let machine = machine_with_states(&supervision_machine_yaml().replace(
            "execute_on: descendant-terminal",
            "execute_on: descendant-transition",
        ));
        let plan = supervised_plan(&["review"]);
        let target = parse_task_id("1.1");
        let task = find_task_by_id(&plan.tasks, &target).expect("child in plan");
        let ancestors: Vec<rhei_core::ast::Task> =
            ancestor_chain(&plan.tasks, &target).into_iter().cloned().collect();
        let delivered = apply_supervision_transition(
            None,
            SupervisionTransition {
                machine: &machine,
                task,
                ancestors: &ancestors,
                metadata_key: &target,
                metadata_prefix: "",
                local_id: "1.1",
                from: "review",
                to: "human-review",
                to_visit: 1,
                operation_supervisor: None,
            },
        )
        .expect("every hop is a checkpoint under `execute_on: descendant-transition`");
        assert_eq!(
            supervision_checkpoints(Some(&delivered), &parse_task_id("1")),
            vec![checkpoint("1.1", "review", "human-review", 1)]
        );
    }

    fn supervision_machine_yaml() -> String {
        r#"name: supervision
version: 1
states:
  supervising:
    description: Supervise
    execute_on: descendant-terminal
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
  - { from: supervising, to: human-review, description: Budget spent, condition: visitCount >= visits }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released }
  - { from: review, to: completed, description: Reviewed }
  - { from: review, to: human-review, description: Escalated }
  - { from: "*", to: cancelled, description: Dropped }
"#
        .to_string()
    }

    /// §FS-rhei-supervision.4.2: a non-poll self-loop is a loop-back re-entry,
    /// so the supervisor's visits are counted whether or not `visits` caps them.
    #[test]
    fn a_supervising_states_visits_are_counted_without_a_declared_budget() {
        let machine = machine_with_states(
            &supervision_machine_yaml().replace("    visits: 12\n", ""),
        );
        let id = parse_task_id("1");
        assert!(state_counts_visits(&machine, "supervising"));
        assert!(!state_counts_visits(&machine, "review"));

        let first = update_metadata_for_transition(None, &id, "supervising", &machine)
            .expect("a supervising state is counted");
        assert_eq!(task_visit_count(Some(&first), &id, "supervising"), 1);
        let second = update_metadata_for_transition(Some(&first), &id, "supervising", &machine)
            .expect("the re-entry increments");
        assert_eq!(task_visit_count(Some(&second), &id, "supervising"), 2);
        // Without a `visits:` budget the rendered state name stays unsuffixed.
        assert_eq!(format_task_state_value("supervising", Some(2), &machine), "supervising");
    }

    /// A checkpoint names one descendant exactly.
    ///
    /// A three-level subtree whose ids collide on the tail — `1.2` beside
    /// `1.1.2` — is the case a suffix match gets wrong: it descends into
    /// `1.1` first and reports the cousin's title and result.
    // §FS-rhei-supervision.5.1
    #[test]
    fn a_checkpoint_resolves_its_descendant_by_exact_qualified_id() {
        let rhei = rhei_core::parse(
            "# Rhei: Nested\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n\
             ### Task 1: Outer\n**State:** supervising\n\n\
             #### Task 1.1: Inner\n**State:** supervising\n\n\
             ##### Task 1.1.1: A\n**State:** review\n\n\
             ##### Task 1.1.2: B\n**State:** review\n\n\
             #### Task 1.2: Sibling\n**State:** review\n",
        )
        .expect("parse nested plan");
        let project = rhei_core::workspace::implicit_panta_from_file_rhei(
            rhei,
            std::path::Path::new("/plans/plan.rhei.md"),
        )
        .expect("qualify");
        let outer = &project.rhei.tasks[0];

        let sibling = checkpoint_qualified_id(outer, "1.2");
        assert_eq!(sibling, "plan.1.2");
        assert_eq!(
            checkpoint_descendant(outer, &sibling).map(|task| task.title.as_str()),
            Some("Sibling"),
            "the tail-colliding cousin plan.1.1.2 must not answer for plan.1.2"
        );
        let cousin = checkpoint_qualified_id(outer, "1.1.2");
        assert_eq!(
            checkpoint_descendant(outer, &cousin).map(|task| task.title.as_str()),
            Some("B")
        );
        // A checkpoint for a descendant the supervisor cancelled out of the
        // plan resolves to nothing rather than to whatever is nearby.
        assert!(checkpoint_descendant(outer, &checkpoint_qualified_id(outer, "1.3")).is_none());
    }

    /// Any non-poll state a self-loop is declared from counts its visits.
    ///
    /// The loop's own exit reads `visitCount`; uncounted, it compares against
    /// `0` forever and the run never leaves the state. A poll state keeps its
    /// own attempt accounting and is left alone.
    // §FS-rhei-supervision.4.2
    #[test]
    fn a_self_looping_state_counts_its_visits_without_a_declared_budget() {
        let machine = machine_with_states(
            r#"name: loop
version: 1
states:
  work:
    description: Work
    agent: pi
  poll-me:
    description: Poll
    agent: pi
    poll: { interval: 5m, max_attempts: 3 }
  plain:
    description: One shot
    agent: pi
  done:
    description: Done
    final: true
transitions:
  - { from: work, to: done, description: Second visit, condition: visitCount >= 2 }
  - { from: work, to: work, description: Loop back }
  - { from: poll-me, to: poll-me, description: Retry }
  - { from: poll-me, to: done, description: Gave up, condition: pollAttempts >= pollMaxAttempts }
  - { from: plain, to: done, description: Finished }
"#,
        );
        assert!(state_counts_visits(&machine, "work"));
        assert!(!state_counts_visits(&machine, "poll-me"), "poll attempts are their own accounting");
        assert!(!state_counts_visits(&machine, "plain"));

        let id = parse_task_id("1");
        let first = update_metadata_for_transition(None, &id, "work", &machine)
            .expect("a self-looping state is counted");
        assert_eq!(task_visit_count(Some(&first), &id, "work"), 1);
        let second = update_metadata_for_transition(Some(&first), &id, "work", &machine)
            .expect("the re-entry increments");
        assert_eq!(task_visit_count(Some(&second), &id, "work"), 2);
        // §FS-rhei-transitions.2.3: no `visits:` budget, no `-<n>` suffix.
        assert_eq!(format_task_state_value("work", Some(2), &machine), "work");
    }
