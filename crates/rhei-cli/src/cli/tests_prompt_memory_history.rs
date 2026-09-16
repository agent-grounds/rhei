    // §FS-rhei-memory.3.2 and .3.4: what finished before this invocation, who is
    // working now, who waits on it, and the map that reaches the rest of the
    // project. The fixtures are the ones in `tests_prompt_memory.rs`.

    /// Frontmatter that puts a task on its `n`th visit of one state.
    fn visit_metadata(task_id: &str, state: &str, visits: u64) -> Metadata {
        serde_yaml::from_str(&format!(
            "metadata:\n  tasks:\n    \"{task_id}\":\n      stateVisits:\n        \
             {state}: {visits}\n"
        ))
        .expect("visit metadata parses")
    }

    /// §FS-rhei-memory.3.2: the finished tasks of the owning rhei, in the order
    /// the ledger finished them, each with a summary derived from its result.
    #[test]
    fn plan_history_orders_by_the_ledger_and_summarizes_results() {
        let dir = memory_plan_dir(&[
            (
                "runtime/state-transitions.log",
                "plan.1.2 review@completed\nplan.1.1 pending@completed\n",
            ),
            ("runtime/results/plan.1.1.md", "## Result\n\nLanded the parser rewrite.\n"),
            ("runtime/results/plan.1.2.md", "## Result\n\nFirst pass\n\n## Result\n\nTwo bugs found.\n"),
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
            history.starts_with(
                "\n## Plan History\n\nFinished work, oldest first. Full text: \
                 `runtime/results/<id>.md` under the owning rhei's execution root.\n\n"
            ),
            "got:\n{history}"
        );
        // 1.2 entered a terminal state first, so it leads — plan order does not.
        assert!(
            history.contains(
                "- Task plan.1.2: Review round 1 \u{2014} completed \u{2014} Two bugs found.\n\
                 - Task plan.1.1: Implement \u{2014} completed \u{2014} Landed the parser \
                 rewrite.\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: the fan-out fold writes one `## Result — <identity>`
    /// entry per invocation (§FS-rhei-states.3.3), and the standing verdict is
    /// the last of them — not the heading the file happens to open with.
    #[test]
    fn the_summary_of_a_folded_result_is_its_last_entry() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 review@completed\n"),
            (
                "runtime/results/plan.1.1.md",
                "## Result \u{2014} claude-code\n\nApproved with two nits.\n\n\
                 ## Result \u{2014} codex\n\nBLOCKED: the migration is unsafe.\n",
            ),
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
            history.contains(
                "- Task plan.1.1: Implement \u{2014} completed \u{2014} BLOCKED: the migration \
                 is unsafe.\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("## Result"), "a heading became the summary:\n{history}");
    }

    /// §FS-rhei-memory.4.3: a plain entry ahead of a fold is an older verdict,
    /// and reporting it would tell the agent the opposite of what stands.
    #[test]
    fn a_plain_entry_before_a_fold_is_not_the_summary() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 review@completed\n"),
            (
                "runtime/results/plan.1.1.md",
                "## Result\n\nFirst round: everything looked fine.\n\n\
                 ## Result \u{2014} claude-code\n\nSecond round: still fine.\n\n\
                 ## Result \u{2014} codex\n\nSecond round: BLOCKED, do not ship.\n",
            ),
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
            history.contains(
                "- Task plan.1.1: Implement \u{2014} completed \u{2014} Second round: BLOCKED, \
                 do not ship.\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: a task the ledger never moved cannot be placed in
    /// time, so it comes first, in plan order.
    #[test]
    fn tasks_with_no_ledger_line_come_first_in_plan_order() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.2 review@completed\n"),
            ("runtime/results/plan.1.2.md", "## Result\n\nTwo bugs found.\n"),
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
            history.contains(
                "- Task plan.1.1: Implement \u{2014} completed \u{2014} (no result)\n\
                 - Task plan.1.2: Review round 1 \u{2014} completed \u{2014} Two bugs found.\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: a result already pasted in full under `## Prior
    /// Task Results` is referred to, not repeated.
    #[test]
    fn a_result_pasted_in_full_shows_see_above() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.2 review@completed\n"),
            ("runtime/results/plan.1.2.md", "## Result\n\nTwo bugs found.\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        // 1.3 lists 1.2 as its prior, so `## Prior Task Results` pastes it.
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Task plan.1.2: Review round 1 \u{2014} completed \u{2014} see above\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: a prior in another rhei is listed, tagged, and
    /// never dropped by the cap.
    #[test]
    fn a_cross_rhei_prior_is_tagged_and_kept() {
        let dir = memory_dir(&[
            ("index.panta.md", "# Panta: Two Rheis\n"),
            (
                "upstream.rhei.md",
                "# Rhei: Upstream\n\n## Tasks\n\n### Task 1: Publish the schema\n\
                 **State:** completed\n\n### Task 2: Announce the schema\n**State:** completed\n\
                 **Prior:** upstream.1\n",
            ),
            (
                "downstream.rhei.md",
                "# Rhei: Downstream\n\n## Tasks\n\n### Task 1: Consume the schema\n\
                 **State:** pending\n**Prior:** upstream.2\n",
            ),
            ("runtime/results/upstream.1.md", "## Result\n\nSchema v2 published.\n"),
            ("runtime/results/upstream.2.md", "## Result\n\nAnnounced on the list.\n"),
        ]);
        let project = dir.path().to_path_buf();
        let loaded = load_plan(&project).expect("project loads");
        let memory = prompt_memory(&loaded, &project, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "downstream.1").expect("downstream.1");
        let context =
            memory_context(dir.path(), &project, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Task upstream.1: Publish the schema \u{2014} completed \u{2014} Schema v2 \
                 published. (rhei `upstream`, prior)\n"
            ),
            "a transitive prior is listed with its own summary; got:\n{history}"
        );
        // The direct prior is pasted in full under `## Prior Task Results`, so
        // the history line refers to it rather than repeating it.
        assert!(
            history.contains(
                "- Task upstream.2: Announce the schema \u{2014} completed \u{2014} see above \
                 (rhei `upstream`, prior)\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.3.4: the map names every rhei's execution root, so a
    /// rhei the prompt does not list is one path away.
    #[test]
    fn reading_the_rhei_lists_every_execution_root() {
        let dir = memory_dir(&[
            ("index.panta.md", "# Panta: Two Rheis\n\n## House Rules\n\nAlways run the tests.\n"),
            (
                "upstream.rhei.md",
                "# Rhei: Upstream\n\n## Tasks\n\n### Task 1: Publish\n**State:** completed\n",
            ),
            (
                "downstream/index.rhei.md",
                "# Rhei: Downstream\n\n## Ground Rules\n\nKeep the schema stable.\n",
            ),
            (
                "downstream/tasks/work.md",
                "### Task 1: Consume\n**State:** pending\n**Prior:** upstream.1\n",
            ),
        ]);
        let project = dir.path().to_path_buf();
        let loaded = load_plan(&project).expect("project loads");
        let memory = prompt_memory(&loaded, &project, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "downstream.1").expect("downstream.1");
        let root = dir.path().join("downstream");
        let context = memory_context(&root, &project, &loaded, &memory, &machine, task, "pending");

        let navigation = render_rhei_navigation(&context);
        // Joined, not written with `/`: the prompt spells a path the way the
        // platform does. §FS-rhei-memory.3.4
        let task_file = Path::new("tasks").join("work.md");
        assert!(
            navigation.contains(&format!(
                "- This rhei: `.` \u{2014} plan `index.rhei.md`, this task's file `{}`\n",
                task_file.display()
            )),
            "got:\n{navigation}"
        );
        assert!(navigation.contains("  - `downstream` \u{2014} `.`\n"), "got:\n{navigation}");
        // A root outside this rhei has no relative form, and keeps the name the
        // run was given rather than another one for the same directory — macOS
        // reaches a temp dir as `/var/…` and `/private/var/…`. §FS-rhei-agents.4.1
        let upstream = dir.path().to_path_buf();
        assert!(
            navigation.contains(&format!(
                "  - `upstream` \u{2014} `{}`\n",
                spelled_path(&upstream)
            )),
            "a root outside this rhei has no relative form; got:\n{navigation}"
        );
        assert!(navigation.contains("### Leaving a trail"), "got:\n{navigation}");

        // §FS-rhei-memory.3.1: a project manifest's own sections are Project
        // Context; the rhei's own are Rhei Context.
        let position = render_position(&context);
        assert!(position.contains("Panta: Two Rheis \u{203a} rhei `downstream`: Downstream"),
            "got:\n{position}");
        assert!(position.contains("## Ground Rules"), "got:\n{position}");
        assert!(position.contains("### Project Context"), "got:\n{position}");
        assert!(position.contains("Always run the tests."), "got:\n{position}");
    }

    /// §FS-rhei-memory.3.2: `### In Flight` names the other agents at work —
    /// the claim a manual worker wrote, and the pass's own spawned tickets.
    #[test]
    fn in_flight_names_claimed_and_spawned_tickets() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let spawned: BTreeSet<String> = ["plan.1.3".to_string()].into_iter().collect();
        let memory = prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), spawned);
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1").expect("task 1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "### In Flight\n\n\
                 - Task plan.1.3: Fix round 1 [review] \u{2014} this run\n\
                 - Task plan.1.4: Review round 2 [pending] \u{2014} codex\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.3.2: `### Dependents` names who reads what this task
    /// writes, with the relation that makes them wait.
    #[test]
    fn dependents_name_the_relation() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.1").expect("task 1.1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                 "### Dependents\n\n\
                 - Task plan.1.2: Review round 1 [completed] \u{2014} prior\n\
                 - Task plan.1.3: Fix round 1 [review] \u{2014} prior, consumes `findings`\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3: a plan with nothing finished, nobody working, and
    /// nobody waiting renders no `## Plan History` at all.
    #[test]
    fn an_empty_history_renders_nothing() {
        let dir = memory_dir(&[(
            "plan.rhei.md",
            "# Rhei: Lonely\n\n## Tasks\n\n### Task 1: The only task\n**State:** pending\n",
        )]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1").expect("task 1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        assert_eq!(render_plan_history(&context).expect("history"), "");
    }

    /// §FS-rhei-memory.1.2: the same inputs produce the same bytes. Nothing
    /// that varies per run — a run id, a timestamp, a pid — is in the prompt.
    #[test]
    fn composing_twice_gives_identical_bytes() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.2 review@completed\n"),
            ("runtime/results/plan.1.2.md", "## Result\n\nTwo bugs found.\n"),
            ("runtime/exports/plan.1.1/findings.md", "Parser findings.\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let first = compose_agent_prompt(&context).expect("prompt");
        let second = compose_agent_prompt(&context).expect("prompt");
        assert_eq!(first, second);
        assert!(first.contains("## Position"), "got:\n{first}");
        assert!(first.contains("## Plan History"), "got:\n{first}");
        assert!(first.contains("### Reading the rhei"), "got:\n{first}");
    }

    /// §FS-rhei-memory.3: the four sections join the prompt at the positions
    /// the spec states, around the sections that were already there.
    #[test]
    fn the_sections_land_in_the_order_the_spec_states() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.3 pending@review\n"),
            ("runtime/results/plan.1.3.md", "## Result\n\nFirst attempt stalled.\n"),
            ("runtime/results/plan.1.2.md", "## Result\n\nTwo bugs found.\n"),
            ("runtime/exports/plan.1.1/findings.md", "Parser findings.\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let prompt = compose_agent_prompt(&context).expect("prompt");
        let at = |needle: &str| prompt.find(needle).unwrap_or_else(|| panic!("{needle} in:\n{prompt}"));
        assert!(at("## State:") < at("\n## Position"));
        assert!(at("\n## Position") < at("\n## Instructions"));
        assert!(at("\n## Prior Task Results") < at("\n## Plan History"));
        assert!(at("\n## Plan History") < at("\n## Previous Visits"));
        assert!(at("\n## Previous Visits") < at("\n## Rhei Commands"));
        assert!(at("Available transitions from") < at("\n### Reading the rhei"));
        assert!(at("\n### Reading the rhei") < at("\n### Leaving a trail"));
        // §FS-rhei-memory.4.5: the retrofitted fence on a pasted prior result.
        assert!(
            prompt.contains("### Task plan.1.2\n\n```markdown\n## Result"),
            "got:\n{prompt}"
        );
    }

    /// §FS-rhei-memory.3.4: the synthetic basin has no authored index, so there
    /// is no plan document to name — the map names this ticket's own file and
    /// leaves the `— plan …` clause off rather than pointing at a directory.
    #[test]
    fn the_basin_names_no_plan_document() {
        let dir = memory_dir(&[
            ("index.panta.md", "# Panta: With Basin\n"),
            ("basin/loose.md", "### Task 3: Unfiled capture\n**State:** pending\n"),
        ]);
        let project = dir.path().to_path_buf();
        let loaded = load_plan(&project).expect("project loads");
        let memory = prompt_memory(&loaded, &project, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "basin.3").expect("basin.3");
        let root = dir.path().join("basin");
        let context = memory_context(&root, &project, &loaded, &memory, &machine, task, "pending");

        let navigation = render_rhei_navigation(&context);
        assert!(
            navigation.contains("- This rhei: `.` \u{2014} this task's file `loose.md`\n"),
            "got:\n{navigation}"
        );
        assert!(!navigation.contains("plan `.`"), "got:\n{navigation}");
    }

    /// §FS-rhei-memory.4.3: a plan finished before ids were qualified keeps its
    /// account under the rhei-local name its `> **Result:**` block links, and
    /// `(no result)` under a body that shows that link is a false statement.
    #[test]
    fn a_legacy_result_link_is_read_when_the_qualified_file_is_missing() {
        let files: &[(&str, &str)] = &[
            (
                "plan.rhei.md",
                "# Rhei: Legacy\n\n## Tasks\n\n\
                 ### Task 1: Old finished task\n**State:** completed\n\n\
                 > **Result:** [1](runtime/results/1.md)\n\n\
                 ### Task 2: New task\n**State:** pending\n**Prior:** 1\n",
            ),
            ("runtime/results/1.md", "## Result\n\nLegacy rhei-local result body.\n"),
        ];
        let dir = memory_dir(files);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.2").expect("task 2");

        // `rhei run` pastes the prior in full, so the history refers to it.
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");
        let priors = render_prior_task_results(&context).expect("prior results");
        assert!(priors.contains("Legacy rhei-local result body."), "got:\n{priors}");
        let history = render_plan_history(&context).expect("history");
        assert!(
            history
                .contains("- Task plan.1: Old finished task \u{2014} completed \u{2014} see above\n"),
            "got:\n{history}"
        );

        // `rhei next` pastes neither, so the same file is what the summary reads.
        let mut memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        memory.pastes_task_inputs = false;
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");
        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Task plan.1: Old finished task \u{2014} completed \u{2014} Legacy rhei-local \
                 result body.\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("(no result)"), "got:\n{history}");
    }
