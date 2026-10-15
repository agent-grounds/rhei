    // `## Previous Visits`: what the ledger and the result file say has already
    // happened to *this* task, and where the last transcript is. The fixtures
    // are the ones in `tests_prompt_memory.rs`.

    /// §FS-rhei-memory.3.3: a task with neither a ledger line nor a result file
    /// has had no previous visit, so the section is not rendered at all.
    #[test]
    fn a_first_visit_renders_no_previous_visits() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        assert_eq!(render_previous_visits(&context).expect("visits"), "");
    }

    /// §FS-rhei-memory.3.3: the trail is the ledger's lines for this task with
    /// the current state appended, the result file is pasted whole, and the
    /// engine's own failure entry is visible in it.
    #[test]
    fn a_revisit_renders_its_trail_result_and_previous_log() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.3 pending@review\n"),
            (
                "runtime/results/plan.1.3.md",
                "## Result\n\nagent timed out in state 'review' after 30m\n",
            ),
            ("runtime/logs/task-plan.1.3-review.log", "=== rhei agent log v1 ===\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let mut context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");
        let metadata = visit_metadata("plan.1.3", "review", 2);
        context.metadata = Some(&metadata);

        let visits = render_previous_visits(&context).expect("visits");
        // §FS-rhei-memory.4.4: the ledger already ends in `review` — the line
        // that moved the task here — so that state is annotated, not repeated.
        assert!(
            visits.contains("Trail for this task: pending \u{2192} review (this visit, visit 2).\n"),
            "got:\n{visits}"
        );
        assert!(visits.contains("Result entries so far:\n\n```markdown\n"), "got:\n{visits}");
        assert!(
            visits.contains("agent timed out in state 'review' after 30m"),
            "the engine's own entry is why a retry knows what stalled; got:\n{visits}"
        );
        // Joined, not written with `/`: the prompt spells a path the way the
        // platform does. §FS-rhei-memory.3.4
        let previous_log = Path::new("runtime").join("logs").join("task-plan.1.3-review.log");
        assert!(
            visits.contains(&format!("Previous log: `{}`\n", previous_log.display())),
            "got:\n{visits}"
        );
    }

    /// §FS-rhei-memory.4.4: the `Previous log:` line is emitted only when the
    /// file it names is on disk.
    #[test]
    fn a_missing_previous_log_is_not_named() {
        let dir = memory_plan_dir(&[("runtime/state-transitions.log", "plan.1.3 pending@review\n")]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let mut context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");
        let metadata = visit_metadata("plan.1.3", "review", 2);
        context.metadata = Some(&metadata);

        let visits = render_previous_visits(&context).expect("visits");
        assert!(visits.contains("Trail for this task:"), "got:\n{visits}");
        assert!(!visits.contains("Previous log:"), "got:\n{visits}");
    }

    /// §FS-rhei-memory.4.5: a pasted body whose own text contains a fence gets
    /// a longer one, so nothing it holds can close the block early.
    #[test]
    fn a_result_holding_a_fence_gets_a_longer_one() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.3 pending@review\n"),
            ("runtime/results/plan.1.3.md", "## Result\n\n```\ncode\n```\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let visits = render_previous_visits(&context).expect("visits");
        assert!(visits.contains("````markdown\n## Result"), "got:\n{visits}");
        assert!(visits.contains("\n````\n"), "got:\n{visits}");
    }

    /// The trail is the ledger's own state sequence. When it already ends in
    /// the state being entered — the engine wrote the line that moved the task
    /// here — that state is annotated in place; a self-loop still leaves the
    /// state twice, because the earlier one is the earlier visit.
    // §FS-rhei-memory.4.4
    #[test]
    fn a_trail_ending_in_this_state_is_annotated_not_repeated() {
        let dir = memory_plan_dir(&[(
            "runtime/state-transitions.log",
            "plan.1.3 pending@review\nplan.1.3 review@review\n",
        )]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let mut context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");
        let metadata = visit_metadata("plan.1.3", "review", 3);
        context.metadata = Some(&metadata);

        let visits = render_previous_visits(&context).expect("visits");
        assert!(
            visits.contains(
                "Trail for this task: pending \u{2192} review \u{2192} review \
                 (this visit, visit 3).\n"
            ),
            "got:\n{visits}"
        );
    }

    /// The other branch: a task with a result file but no ledger line — an
    /// imported plan, or one finished before the ledger existed — has a trail
    /// the current state is appended to rather than folded into.
    // §FS-rhei-memory.4.4
    #[test]
    fn a_trail_that_does_not_end_here_gets_this_state_appended() {
        let dir = memory_plan_dir(&[
            ("runtime/results/plan.1.3.md", "## Result\n\nImported verdict.\n"),
            // A line for a *different* task: the ledger exists, this task is
            // simply not in it.
            ("runtime/state-transitions.log", "plan.1.1 pending@completed\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let visits = render_previous_visits(&context).expect("visits");
        assert!(
            visits.contains("Trail for this task: review (this visit, visit 1).\n"),
            "got:\n{visits}"
        );
        assert!(visits.contains("Imported verdict."), "got:\n{visits}");
    }

    /// §FS-rhei-memory.3.1 and step 1 of §FS-rhei-memory.4.4: a ledger line carrying the `-<visit>`
    /// suffix names the state the machine declares. Left raw it spelled one
    /// state three ways and appended this visit instead of annotating it.
    #[test]
    fn a_suffixed_ledger_line_still_names_one_state() {
        let dir = memory_plan_dir(&[(
            "runtime/state-transitions.log",
            "plan.1.3 pending@review\nplan.1.3 review@review-2\nplan.1.3 review-2@review-3\n",
        )]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let mut context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");
        let metadata = visit_metadata("plan.1.3", "review", 3);
        context.metadata = Some(&metadata);

        let visits = render_previous_visits(&context).expect("visits");
        assert!(
            visits.contains(
                "Trail for this task: pending \u{2192} review \u{2192} review \u{2192} review \
                 (this visit, visit 3).\n"
            ),
            "got:\n{visits}"
        );
        assert!(!visits.contains("review-"), "no raw suffix survives; got:\n{visits}");
    }
