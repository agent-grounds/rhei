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
        let reset = clear_runtime_task_metadata(Some(&held)).expect("metadata survives");
        assert!(supervision_map(Some(&reset), &id).is_none());
    }

