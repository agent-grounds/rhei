    // §FS-rhei-supervision.3 and .5: which tasks the barrier admits, and what a
    // supervised subtree puts in a prompt. The fixtures live beside the
    // metadata tests in `tests_subtree_supervision.rs`.

    // -----------------------------------------------------------------------
    // Readiness
    // -----------------------------------------------------------------------

    fn ready_ids(rhei: &rhei_core::ast::Rhei, spawned: &[&str]) -> Vec<String> {
        let machines = rhei_validator::MachineSet::single(supervision_machine());
        let dir = tempfile::tempdir().expect("tmpdir");
        let spawned: HashSet<String> = spawned.iter().map(|id| (*id).to_string()).collect();
        find_ready_tasks(rhei, &machines, &SpawnSiteRoots::plan_only(dir.path(), &dir.path().join("runtime")), &spawned)
        .iter()
        .map(|task| task.id.to_string())
        .collect()
    }

    fn with_metadata(mut rhei: rhei_core::ast::Rhei, metadata: Metadata) -> rhei_core::ast::Rhei {
        rhei.metadata = Some(metadata);
        rhei
    }

    /// §FS-rhei-supervision.3.1: entry holds — the supervisor is ready while
    /// its subtree is open, and nothing beneath it is.
    #[test]
    fn a_held_supervisor_is_ready_and_its_subtree_is_not() {
        let plan = supervised_plan(&["review", "review"]);
        assert_eq!(ready_ids(&plan, &[]), vec!["1".to_string()]);
    }

    /// §FS-rhei-supervision.3.1: the self-loop releases; the supervisor drops
    /// out of the ready set and its descendants join it.
    #[test]
    fn releasing_hands_the_ready_set_to_the_descendants() {
        let released = record_supervision_release(None, &parse_task_id("1"));
        let plan = with_metadata(supervised_plan(&["review", "review"]), released);
        assert_eq!(ready_ids(&plan, &[]), vec!["1.1".to_string(), "1.2".to_string()]);
    }

    /// §FS-rhei-supervision.3.1: a checkpoint holds again, and the supervisor
    /// waits for the drain — siblings already running finish first.
    #[test]
    fn a_held_supervisor_waits_for_its_in_flight_descendants() {
        let held = record_supervision_hold(
            None,
            &parse_task_id("1"),
            Some(&checkpoint("1.1", "review", "completed", 1)),
        );
        let plan = with_metadata(supervised_plan(&["completed", "review"]), held);
        assert!(
            ready_ids(&plan, &["1.2"]).is_empty(),
            "nothing new starts, and the supervisor waits for the sibling to exit"
        );
        assert_eq!(ready_ids(&plan, &[]), vec!["1".to_string()]);
    }

    /// §FS-rhei-supervision.3.2: every supervising ancestor must have released,
    /// so a held outer supervisor holds a released inner one's children too.
    #[test]
    fn a_held_outer_supervisor_holds_the_whole_subtree() {
        let plan = rhei_core::parse(
            "# Rhei: Nested\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n### Task 1: Top\n**State:** supervising\n\n#### Task 1.1: Middle\n**State:** supervising\n\n##### Task 1.1.1: Leaf\n**State:** review\n",
        )
        .expect("parse nested plan");
        let inner_released = record_supervision_release(None, &parse_task_id("1.1"));
        let plan = with_metadata(plan, inner_released);
        assert_eq!(ready_ids(&plan, &[]), vec!["1".to_string()]);

        let both_released =
            record_supervision_release(plan.metadata.as_ref(), &parse_task_id("1"));
        let plan = with_metadata(plan.clone(), both_released);
        assert_eq!(ready_ids(&plan, &[]), vec!["1.1.1".to_string()]);
    }

    /// §FS-rhei-supervision.3.2: a task an assignee holds is in flight, so its
    /// supervisor is not ready and neither are its siblings.
    #[test]
    fn a_claimed_descendant_keeps_its_supervisor_out_of_the_ready_set() {
        let plan = rhei_core::parse(
            "# Rhei: Claimed\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n### Task 1: Parent\n**State:** supervising\n\n#### Task 1.1: Child\n**State:** review\n**Assignee:** pi\n",
        )
        .expect("parse claimed plan");
        assert!(ready_ids(&plan, &[]).is_empty());
    }

    /// §FS-rhei-supervision.3.4: the reason a held descendant is not moving is
    /// its supervisor, named — never a stall.
    #[test]
    fn a_held_descendant_reports_its_supervisor() {
        let released = record_supervision_release(None, &parse_task_id("1"));
        let plan = supervised_plan(&["review"]);
        let machines = rhei_validator::MachineSet::single(supervision_machine());
        let child = &plan.tasks[0].children[0];
        let hold = held_by_supervisor(child, &plan, &machines).expect("the parent holds it");
        assert_eq!(hold.supervisor.to_string(), "1");
        assert_eq!(hold.state, "supervising");
        assert!(!hold.awaiting_human, "a supervising state is not a human gate");

        let plan = with_metadata(plan, released);
        assert!(
            held_by_supervisor(&plan.tasks[0].children[0], &plan, &machines).is_none(),
            "a released supervisor holds nothing"
        );
    }

    // -----------------------------------------------------------------------
    // Prompt composition
    // -----------------------------------------------------------------------

    fn supervision_context<'a>(
        workspace: &'a Path,
        rhei: &'a rhei_core::ast::Rhei,
        machine: &'a rhei_validator::StateMachine,
        task: &'a rhei_core::ast::Task,
        state_name: &'a str,
    ) -> RuntimeTemplateContext<'a> {
        RuntimeTemplateContext {
            workspace_root: workspace,
            task_roots: None,
            plan_tasks: Some(&rhei.tasks),
            checkout_root: workspace,
            plan_path: workspace,
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name,
            current_state_raw: task.state.as_str(),
            machine,
            metadata: rhei.metadata.as_ref(),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("pi"),
            agent_mode: None,
            tooling: None,
            memory: None,
        }
    }

    fn write_under(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, contents).expect("write fixture");
    }

    /// §FS-rhei-supervision.5.1: a supervisor's visit renders the checkpoints
    /// it is owed, each carrying what the step left behind.
    #[test]
    fn a_supervisors_prompt_renders_its_checkpoints() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let machine = supervision_machine();
        let held = record_supervision_hold(
            None,
            &parse_task_id("1"),
            Some(&checkpoint("1.1", "review", "completed", 1)),
        );
        let plan = with_metadata(supervised_plan(&["completed", "review"]), held);
        write_under(dir.path(), "runtime/results/1.1.md", "Found two parser bugs.\n");

        let context =
            supervision_context(dir.path(), &plan, &machine, &plan.tasks[0], "supervising");
        let prompt = compose_agent_prompt(&context).expect("prompt composes");
        assert!(prompt.contains("## Checkpoints"), "got:\n{prompt}");
        assert!(
            prompt.contains("### Task 1.1: Child 1 \u{2014} review \u{2192} completed (visit 1)"),
            "got:\n{prompt}"
        );
        assert!(prompt.contains("Found two parser bugs."), "got:\n{prompt}");
        assert!(
            !prompt.contains("## Child Task Results"),
            "a supervising state renders checkpoints instead; got:\n{prompt}"
        );
        assert!(
            prompt.contains("You are supervising this task's subtree."),
            "the command permissions must name the supervisor's extra reach; got:\n{prompt}"
        );
        assert!(prompt.contains("`--supervisor 1`"), "got:\n{prompt}");
    }

    /// §FS-rhei-supervision.5.1: the first visit has nothing to judge yet.
    #[test]
    fn a_first_visit_renders_no_checkpoints_section() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let machine = supervision_machine();
        let plan = supervised_plan(&["review"]);
        let context =
            supervision_context(dir.path(), &plan, &machine, &plan.tasks[0], "supervising");
        let prompt = compose_agent_prompt(&context).expect("prompt composes");
        assert!(!prompt.contains("## Checkpoints"), "got:\n{prompt}");
        assert!(prompt.contains("## Child Tasks"), "the map is rendered every visit; got:\n{prompt}");
    }

    /// §FS-rhei-supervision.5.1: an unsupervised parent sees what its subtree
    /// produced, which it never did before.
    #[test]
    fn an_unsupervised_parent_sees_its_child_results() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let machine = supervision_machine();
        let plan = rhei_core::parse(
            "# Rhei: Plain\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n### Task 1: Parent\n**State:** review\n\n#### Task 1.1: Child\n**State:** completed\n",
        )
        .expect("parse plan");
        write_under(dir.path(), "runtime/results/1.1.md", "The child's account.\n");
        let context = supervision_context(dir.path(), &plan, &machine, &plan.tasks[0], "review");
        let prompt = compose_agent_prompt(&context).expect("prompt composes");
        assert!(prompt.contains("## Child Task Results"), "got:\n{prompt}");
        assert!(prompt.contains("The child's account."), "got:\n{prompt}");
    }

    /// §FS-rhei-supervision.5.2: both reserved brief paths render, task-level
    /// first, under one heading that names the supervisor.
    #[test]
    fn a_descendant_reads_the_briefs_its_supervisor_wrote() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let machine = supervision_machine();
        let plan = supervised_plan(&["review"]);
        write_under(dir.path(), "runtime/supervise/1.1.md", "Look at the lexer first.\n");
        write_under(dir.path(), "runtime/supervise/1.1/review.md", "Only the error paths.\n");

        let child = &plan.tasks[0].children[0];
        let context = supervision_context(dir.path(), &plan, &machine, child, "review");
        let prompt = compose_agent_prompt(&context).expect("prompt composes");
        assert!(prompt.contains("## Supervisor Brief"), "got:\n{prompt}");
        assert!(prompt.contains("directions from the supervising Task 1."), "got:\n{prompt}");
        let task_level = prompt.find("Look at the lexer first.").expect("task-level brief");
        let state_level = prompt.find("Only the error paths.").expect("state-level brief");
        assert!(task_level < state_level, "task-level brief comes first; got:\n{prompt}");
    }

    #[test]
    fn no_brief_renders_no_section() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let machine = supervision_machine();
        let plan = supervised_plan(&["review"]);
        let child = &plan.tasks[0].children[0];
        let context = supervision_context(dir.path(), &plan, &machine, child, "review");
        let prompt = compose_agent_prompt(&context).expect("prompt composes");
        assert!(!prompt.contains("## Supervisor Brief"), "got:\n{prompt}");
        assert!(
            !prompt.contains("You are supervising this task's subtree."),
            "an ordinary state gets no supervisor permissions; got:\n{prompt}"
        );
    }

    // -----------------------------------------------------------------------
    // Edge selection
    // -----------------------------------------------------------------------

    fn supervisor_next_edge(plan: &str) -> Option<String> {
        let rhei = rhei_core::parse(plan).expect("parse plan");
        let machine = supervision_machine();
        find_next_transition(&rhei.tasks[0], &rhei, &machine).expect("selection succeeds")
    }

    /// §FS-rhei-supervision.4.1: transitions are tried in declaration order, so
    /// the exhaustion edge comes first, the terminal edge second, and the
    /// unconditional self-loop last.
    #[test]
    fn a_supervisor_selects_release_then_finish_then_escalation() {
        let open = "# Rhei: Edges\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n### Task 1: Parent\n**State:** supervising\n\n#### Task 1.1: Child\n**State:** review\n";
        assert_eq!(supervisor_next_edge(open).as_deref(), Some("supervising"));

        let closed = open.replace("**State:** review", "**State:** completed");
        assert_eq!(
            supervisor_next_edge(&closed).as_deref(),
            Some("completed"),
            "`openDescendants < 1` selects the terminal edge over the self-loop"
        );

        // The budget is spent: the self-loop is refused and the escalation edge
        // is what is left. §FS-rhei-transitions.4.3
        let exhausted = open.replace(
            "structure:\n  maxLevels: 3\n",
            "structure:\n  maxLevels: 3\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        supervising: 12\n",
        );
        assert_eq!(supervisor_next_edge(&exhausted).as_deref(), Some("human-review"));
    }

    /// The release self-loop is the only non-terminal edge that drops a claim.
    ///
    /// Scoped this tightly on purpose: assignment is otherwise owned by
    /// `rhei next`, and unassignment by the shared terminal finalization.
    // §FS-rhei-supervision.3.4
    #[test]
    fn only_a_supervising_self_loop_ends_the_claim_it_was_taken_under() {
        let machine = supervision_machine();
        assert!(transition_ends_supervisor_visit(&machine, "supervising", "supervising"));
        assert!(!transition_ends_supervisor_visit(&machine, "supervising", "completed"));
        assert!(!transition_ends_supervisor_visit(&machine, "supervising", "human-review"));
        assert!(
            !transition_ends_supervisor_visit(&machine, "review", "review"),
            "an ordinary state's self-loop is not a supervisor's visit"
        );
        assert!(!transition_ends_supervisor_visit(&machine, "review", "completed"));
    }
