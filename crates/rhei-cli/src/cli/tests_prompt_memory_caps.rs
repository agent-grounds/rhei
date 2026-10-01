    // Every cap the spec states, exercised where it bites: the line that is
    // kept, the line that is dropped, and the literal overflow line that names
    // what holds the rest. Fixtures live in `tests_prompt_memory.rs`.

    // §FS-rhei-memory.4

    /// §FS-rhei-memory.4.2: the parent's body is capped at 200 lines and says
    /// where the rest is.
    #[test]
    fn a_long_parent_body_is_truncated_with_its_source_named() {
        let body = repeated_lines("line", 250);
        let plan = format!(
            "# Rhei: Long\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
             ### Task 1: Parent\n**State:** pending\n\n{body}\n#### Task 1.1: Child\n\
             **State:** review\n"
        );
        let dir = memory_dir(&[("plan.rhei.md", plan.as_str())]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.1").expect("task 1.1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let position = render_position(&context);
        assert!(position.contains("line 200\n"), "got:\n{position}");
        assert!(!position.contains("line 201\n"), "got:\n{position}");
        assert!(
            position.contains("\u{2026} truncated; read plan.rhei.md\n"),
            "got:\n{position}"
        );
    }

    /// §FS-rhei-memory.4.4: only the last 100 lines of the result file are
    /// pasted, with the overflow line first.
    #[test]
    fn a_long_result_file_keeps_its_newest_entries() {
        let body = repeated_lines("entry", 150);
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.3 pending@review\n"),
            ("runtime/results/plan.1.3.md", body.as_str()),
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
        // The prompt spells a path the way the platform does, so the
        // expectation is joined rather than written with `/`. §FS-rhei-memory.3.4
        let result_file = Path::new("runtime").join("results").join("plan.1.3.md");
        assert!(
            visits.contains(&format!(
                "\u{2026} earlier entries omitted; read {}\n",
                result_file.display()
            )),
            "got:\n{visits}"
        );
        assert!(visits.contains("entry 150\n"), "got:\n{visits}");
        assert!(!visits.contains("entry 50\n"), "got:\n{visits}");
    }

    /// §FS-rhei-memory.4.3: a summary is one line, cut at 120 columns.
    #[test]
    fn a_long_summary_is_cut_to_120_columns() {
        let long = "x".repeat(200);
        let body = format!("## Result\n\n{long}\n\nmore detail below\n");
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 pending@completed\n"),
            ("runtime/results/plan.1.1.md", body.as_str()),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.4").expect("task 1.4");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        let expected = format!("{}\u{2026}\n", "x".repeat(120));
        assert!(history.contains(&expected), "got:\n{history}");
        assert!(!history.contains("more detail below"), "got:\n{history}");
    }

    /// §FS-rhei-memory.4.3: the list is capped at 40, the oldest own entries go
    /// first, and the overflow line is emitted once, before the entries.
    #[test]
    fn the_cap_drops_the_oldest_own_entries_and_names_the_command() {
        let mut plan = String::from(
            "# Rhei: Wide\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
             ### Task 1: Parent\n**State:** pending\n\n",
        );
        let mut ledger = String::new();
        for n in 1..=50 {
            plan.push_str(&format!("#### Task 1.{n}: Step {n}\n**State:** completed\n\n"));
            ledger.push_str(&format!("plan.1.{n} pending@completed\n"));
        }
        plan.push_str("#### Task 1.51: Last\n**State:** pending\n");
        let dir = memory_dir(&[
            ("plan.rhei.md", plan.as_str()),
            ("runtime/state-transitions.log", ledger.as_str()),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.51").expect("task 1.51");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "\u{2026} 10 earlier tasks not shown \u{2014} rhei list --rhei plan --terminal\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("Task plan.1.10:"), "the oldest go first; got:\n{history}");
        assert!(history.contains("Task plan.1.11:"), "got:\n{history}");
        assert_eq!(
            history.lines().filter(|line| line.starts_with("- Task ")).count(),
            memory_caps::PLAN_HISTORY
        );
    }

    /// §FS-rhei-memory.4.3: the cap comes out of the owning rhei's own
    /// backlog — a prior in another rhei is what this task depends on and is
    /// never dropped, however long the backlog is.
    #[test]
    fn the_cap_never_drops_a_cross_rhei_prior() {
        let mut downstream = String::from("# Rhei: Downstream\n\n## Tasks\n\n");
        let mut ledger = String::new();
        for n in 1..=45 {
            downstream.push_str(&format!("### Task {n}: Step {n}\n**State:** completed\n\n"));
            ledger.push_str(&format!("downstream.{n} pending@completed\n"));
        }
        downstream.push_str("### Task 46: Consume the schema\n**State:** pending\n**Prior:** upstream.1\n");
        let dir = memory_dir(&[
            ("index.panta.md", "# Panta: Backlog\n"),
            (
                "upstream.rhei.md",
                "# Rhei: Upstream\n\n## Tasks\n\n### Task 1: Publish the schema\n\
                 **State:** completed\n",
            ),
            ("downstream.rhei.md", downstream.as_str()),
            ("runtime/state-transitions.log", ledger.as_str()),
        ]);
        let project = dir.path().to_path_buf();
        let loaded = load_plan(&project).expect("project loads");
        let mut memory =
            prompt_memory(&loaded, &project, &dir.path().join("runtime"), BTreeSet::new());
        // Keep the prior out of `## Prior Task Results` so its own line shows.
        memory.pastes_task_inputs = false;
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "downstream.46").expect("downstream.46");
        let context =
            memory_context(dir.path(), &project, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        // 45 own plus the prior is 46 entries; the six oldest own entries go.
        assert!(
            history.contains(
                "\u{2026} 6 earlier tasks not shown \u{2014} rhei list --rhei downstream \
                 --terminal\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("Task downstream.6:"), "the oldest own go first; got:\n{history}");
        assert!(history.contains("- Task downstream.7: Step 7"), "got:\n{history}");
        assert!(
            history.contains(
                "- Task upstream.1: Publish the schema \u{2014} completed \u{2014} (no result) \
                 (rhei `upstream`, prior)\n"
            ),
            "the prior survives the cap; got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: `### Dependents` is capped at 30 and names the
    /// command that lists the rest.
    #[test]
    fn dependents_are_capped_and_name_the_command() {
        let mut plan = String::from(
            "# Rhei: Fanout\n\n## Tasks\n\n### Task 1: Source\n**State:** pending\n\n",
        );
        for n in 2..=40 {
            plan.push_str(&format!(
                "### Task {n}: Reader {n}\n**State:** pending\n**Prior:** 1\n\n"
            ));
        }
        let dir = memory_dir(&[("plan.rhei.md", plan.as_str())]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1").expect("task 1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        // §FS-rhei-memory.4.3: nothing has finished, so the preamble that
        // introduces the list would be a lie; the sub-section carries the
        // section on its own.
        assert!(
            history.starts_with("\n## Plan History\n\n### Dependents\n"),
            "got:\n{history}"
        );
        let listed = history
            .lines()
            .filter(|line| line.starts_with("- Task ") && line.ends_with("\u{2014} prior"))
            .count();
        assert_eq!(listed, memory_caps::DEPENDENTS);
        assert!(
            history.contains("\u{2026} 9 more \u{2014} rhei list --has-prior plan.1\n"),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.2: a pasted context block is capped at 1000 lines and
    /// names the document that holds the rest.
    #[test]
    fn a_long_context_block_is_truncated_with_its_source_named() {
        let notes = repeated_lines("note", 1200);
        let plan = format!(
            "# Rhei: Verbose\n\n## Notes\n\n{notes}\n## Tasks\n\n\
             ### Task 1: Only\n**State:** pending\n"
        );
        let dir = memory_dir(&[("plan.rhei.md", plan.as_str())]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1").expect("task 1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let position = render_position(&context);
        // The `## Notes` heading is the block's first line, so 1000 lines of
        // block reach `note 998`.
        assert!(position.contains("note 998\n"), "got the tail of the block");
        assert!(!position.contains("note 999\n"), "the cap bites");
        assert!(
            position.contains("\u{2026} truncated; read plan.rhei.md\n"),
            "got:\n{position}"
        );
    }

    /// §FS-rhei-memory.4.2: `### Siblings` is capped at 30 and names the
    /// command that lists the rest.
    #[test]
    fn siblings_are_capped_and_name_the_command() {
        let mut plan = String::from(
            "# Rhei: Wide\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
             ### Task 1: Parent\n**State:** pending\n\n",
        );
        for n in 1..=40 {
            plan.push_str(&format!("#### Task 1.{n}: Step {n}\n**State:** pending\n\n"));
        }
        let dir = memory_dir(&[("plan.rhei.md", plan.as_str())]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.1").expect("task 1.1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let position = render_position(&context);
        let listed = position.lines().filter(|line| line.starts_with("- Task plan.1.")).count();
        assert_eq!(listed, memory_caps::SIBLINGS);
        assert!(
            position.contains("\u{2026} 9 more \u{2014} rhei list --parent plan.1\n"),
            "got:\n{position}"
        );
    }

    /// §FS-rhei-memory.4.3: `### In Flight` is capped at 20 and names the
    /// command that lists the rest.
    #[test]
    fn in_flight_is_capped_and_names_the_command() {
        let mut plan = String::from("# Rhei: Busy\n\n## Tasks\n\n");
        for n in 1..=30 {
            plan.push_str(&format!(
                "### Task {n}: Step {n}\n**State:** pending\n**Assignee:** worker-{n}\n\n"
            ));
        }
        plan.push_str("### Task 31: Mine\n**State:** review\n");
        let dir = memory_dir(&[("plan.rhei.md", plan.as_str())]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.31").expect("task 31");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let history = render_plan_history(&context).expect("history");
        let listed = history.lines().filter(|line| line.contains("\u{2014} worker-")).count();
        assert_eq!(listed, memory_caps::IN_FLIGHT);
        assert!(
            history.contains("\u{2026} 10 more \u{2014} rhei list --non-terminal\n"),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3 step 5: the cap drops the rows admitted only by a
    /// terminal descendant before any row naming a real agent, so a subtree
    /// reporting progress never pushes out an agent someone can ask.
    #[test]
    fn in_flight_drops_a_progress_row_before_a_row_naming_an_agent() {
        let mut plan =
            String::from("# Rhei: Priority\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n");
        // First in plan order, so only the drop priority can keep them out.
        for n in 1..=3 {
            plan.push_str(&format!(
                "### Task p{n}: Subtree {n}\n**State:** pending\n\n\
                 #### Task p{n}.1: Finished inside it\n**State:** completed\n\n\
                 #### Task p{n}.2: Still open\n**State:** pending\n\n"
            ));
        }
        for n in 1..=20 {
            plan.push_str(&format!(
                "### Task w{n}: Step {n}\n**State:** pending\n**Assignee:** worker-{n}\n\n"
            ));
        }
        plan.push_str("### Task mine: Mine\n**State:** review\n");
        let dir = memory_dir(&[("plan.rhei.md", plan.as_str())]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.mine").expect("task mine");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let history = render_plan_history(&context).expect("history");
        // Every row naming an agent survives, and fills the cap on its own.
        let held = history.lines().filter(|line| line.contains("\u{2014} worker-")).count();
        assert_eq!(held, memory_caps::IN_FLIGHT);
        // None of the three rows admitted only by progress does.
        assert!(!history.contains("- Task plan.p"), "got:\n{history}");
        assert!(!history.contains("subtasks finished"), "got:\n{history}");
        assert!(
            history.contains("\u{2026} 3 more \u{2014} rhei list --non-terminal\n"),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: the cut counts characters, not bytes — a summary of
    /// multi-byte characters keeps 120 of them and stays valid UTF-8.
    #[test]
    fn a_summary_is_cut_by_characters_not_bytes() {
        let long = "\u{e9}".repeat(200);
        let body = format!("## Result\n\n{long}\n");
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 pending@completed\n"),
            ("runtime/results/plan.1.1.md", body.as_str()),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.4").expect("task 1.4");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        let expected = format!("{}\u{2026}\n", "\u{e9}".repeat(120));
        assert!(history.contains(&expected), "got:\n{history}");
    }

    /// §FS-rhei-memory.4.3: a task whose result file is missing — or present
    /// and empty — reads `(no result)`, never a blank column.
    #[test]
    fn an_empty_result_file_reads_no_result() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 pending@completed\n"),
            ("runtime/results/plan.1.1.md", "## Result\n\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.4").expect("task 1.4");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains("- Task plan.1.1: Implement \u{2014} completed \u{2014} (no result)\n"),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.3.2: a cancelled task is history too — why something was
    /// not done is memory, so it is listed like any other terminal task.
    #[test]
    fn a_cancelled_task_is_listed_in_the_history() {
        let plan = "# Rhei: Dropped\n\n## Tasks\n\n\
                    ### Task 1: Abandoned approach\n**State:** cancelled\n\n\
                    ### Task 2: The replacement\n**State:** pending\n";
        let dir = memory_dir(&[
            ("plan.rhei.md", plan),
            ("runtime/state-transitions.log", "plan.1 pending@cancelled\n"),
            ("runtime/results/plan.1.md", "## Result\n\nSuperseded by Task 2.\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.2").expect("task 2");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Task plan.1: Abandoned approach \u{2014} cancelled \u{2014} Superseded by \
                 Task 2.\n"
            ),
            "got:\n{history}"
        );
    }
