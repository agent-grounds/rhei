    // The path rule and the folded shapes: which finished tasks a reader is told
    // about, and what one line says about a subtree it speaks for. Order, summary
    // derivation and the sub-sections are next door. §FS-rhei-memory.4.3

    /// A machine whose terminal states are not the default ones, declared in an
    /// order no sort would produce — so a breakdown that followed the alphabet,
    /// or plan order, reads differently from one that followed the machine.
    fn custom_terminal_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"
name: custom-terminal
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {task_id}.
  shipped:
    description: Shipped it
    final: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: shipped }
  - { from: pending, to: completed }
  - { from: "*", to: cancelled }
"#,
        )
        .expect("machine should parse")
    }

    /// `## Position` and `## Plan History` for one task of a fixture tree, both
    /// composed the way `rhei run` and `rhei next` compose them.
    fn fold_sections(
        files: &[(&str, &str)],
        plan_file: &str,
        task_id: &str,
        machine: &rhei_validator::StateMachine,
        pastes_task_inputs: bool,
    ) -> (String, String) {
        let dir = memory_dir(files);
        let plan_path = dir.path().join(plan_file);
        let loaded = load_plan(&plan_path).expect("plan loads");
        let mut memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        memory.pastes_task_inputs = pastes_task_inputs;
        let task = find_task_by_id_str(&loaded.rhei.tasks, task_id).expect("the reader");
        let state = task.state.clone();
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, machine, task, &state);
        (render_position(&context), render_plan_history(&context).expect("history"))
    }

    /// The history of one task of a fixture tree under the default test machine.
    fn fold_history(files: &[(&str, &str)], task_id: &str) -> String {
        fold_sections(files, "plan.rhei.md", task_id, &memory_machine(), true).1
    }

    /// A rhei with one folded finished root, one finished leaf root, and a reader.
    const FOLD_PLAN: &str = "# Rhei: Fold\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
         ### Task 1: Survey the field\n**State:** completed\n\n\
         #### Task 1.1: Read the papers\n**State:** completed\n\n\
         #### Task 1.2: Drop the dead end\n**State:** cancelled\n\n\
         ### Task 2: Pin the budget\n**State:** completed\n\n\
         ### Task 3: Write it up\n**State:** pending\n";

    /// The fixture files of [`FOLD_PLAN`], finished in a fixed ledger order.
    fn fold_plan_files() -> Vec<(&'static str, &'static str)> {
        vec![
            ("plan.rhei.md", FOLD_PLAN),
            (
                "runtime/state-transitions.log",
                "plan.1.1 pending@completed\nplan.1.2 pending@cancelled\n\
                 plan.1 pending@completed\nplan.2 pending@completed\n",
            ),
            ("runtime/results/plan.1.md", "## Result\n\nThe field is three papers wide.\n"),
            ("runtime/results/plan.2.md", "## Result\n\nTwo weeks, one reviewer.\n"),
        ]
    }

    /// §FS-rhei-memory.3.2: a listed task with descendants speaks for its whole
    /// subtree in one line, and the breakdown buckets every descendant by its
    /// own state name — so the numbers add up to the count beside them.
    #[test]
    fn a_finished_root_folds_its_subtree_into_one_line() {
        let history = fold_history(&fold_plan_files(), "plan.3");

        assert!(
            history.contains(
                "- Task plan.1: Survey the field \u{2014} completed \u{2014} The field is three \
                 papers wide. \u{2014} 2 subtasks: 1 completed, 1 cancelled\n"
            ),
            "got:\n{history}"
        );
        // §FS-rhei-memory.4.3 step 1: the children are spoken for, not listed.
        assert!(!history.contains("- Task plan.1."), "got:\n{history}");
    }

    /// §FS-rhei-memory.3.2: a task with no descendant renders no fold clause —
    /// its line is byte-for-byte the one it has always had.
    #[test]
    fn a_leaf_renders_no_fold_clause() {
        let history = fold_history(&fold_plan_files(), "plan.3");

        assert!(
            history.contains(
                "- Task plan.2: Pin the budget \u{2014} completed \u{2014} Two weeks, one \
                 reviewer.\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("Pin the budget \u{2014} completed \u{2014} Two weeks, one \
                 reviewer. \u{2014}"), "a leaf gained a clause:\n{history}");
    }

    /// §FS-rhei-memory.3.2: the preamble says the list is the path rather than
    /// the tree, and names the drill-down once so no folded line repeats it.
    #[test]
    fn the_preamble_names_the_path_and_the_drill_down() {
        let history = fold_history(&fold_plan_files(), "plan.3");

        assert!(
            history.starts_with(
                "\n## Plan History\n\nFinished work on the way from the plan's roots to this \
                 task, oldest first. Full text: `runtime/results/<id>.md` under the owning \
                 rhei's execution root; a folded subtree: `rhei list --parent <id>`.\n\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.5: one renderer feeds every surface, so the section the
    /// `rhei run` prompt carries is the same bytes `rhei next` prints and the
    /// same bytes, trimmed, that `plan_history` carries in `--json`.
    #[test]
    fn every_surface_carries_the_same_history_bytes() {
        let dir = memory_dir(&fold_plan_files());
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.3").expect("task 3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let section = render_plan_history(&context).expect("history");
        let prompt = compose_agent_prompt(&context).expect("prompt");
        assert!(prompt.contains(&section), "the run prompt pastes the section verbatim");
        // `--json` serializes the same string trimmed, so the fold survives it.
        assert!(section.trim().contains("\u{2014} 2 subtasks: 1 completed, 1 cancelled"));
    }

    /// §FS-rhei-memory.4.3 step 1: at depth 2 the path is the plan's top-level
    /// tasks and the reader's own siblings — another root's children are off it.
    #[test]
    fn a_depth_two_reader_sees_the_roots_and_its_own_siblings() {
        let history = fold_history(
            &[(
                "plan.rhei.md",
                "# Rhei: Two Roots\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
                 ### Task 1: Decide the approach\n**State:** completed\n\n\
                 #### Task 1.1: Weigh the options\n**State:** completed\n\n\
                 ### Task 2: Build it\n**State:** pending\n\n\
                 #### Task 2.1: Write the spec\n**State:** completed\n\n\
                 #### Task 2.2: Implement it\n**State:** pending\n",
            )],
            "plan.2.2",
        );

        assert!(history.contains("- Task plan.1: Decide the approach"), "got:\n{history}");
        assert!(history.contains("- Task plan.2.1: Write the spec"), "got:\n{history}");
        // The other root's child is off the path; the reader's own parent is an
        // ancestor, which `## Position` already names. §FS-rhei-memory.4.2
        assert!(!history.contains("- Task plan.1.1"), "got:\n{history}");
        assert!(!history.contains("- Task plan.2: Build it"), "got:\n{history}");
    }

    /// The release plan of §FS-rhei-memory.6.2, four roots wide and three deep,
    /// with the invocation at `release.ship.gate.test`.
    const RELEASE_PLAN: &str = "# Rhei: Release 2.0\n\n---\nstructure:\n  maxLevels: 4\n---\n\n\
         ## Tasks\n\n\
         ### Task audit: Audit the dependencies\n**State:** completed\n\n\
         #### Task audit.scan: Scan the lockfile\n**State:** completed\n\n\
         #### Task audit.triage: Triage the findings\n**State:** cancelled\n\n\
         ### Task ship: Ship 2.0\n**State:** pending\n\n\
         #### Task ship.notes: Write the notes\n**State:** completed\n\n\
         #### Task ship.gate: Clear the gate\n**State:** pending\n\n\
         ##### Task ship.gate.lint: Run the linter\n**State:** completed\n\n\
         ##### Task ship.gate.test: Run the suite\n**State:** pending\n\n\
         ##### Task ship.gate.perf: Run the benchmarks\n**State:** pending\n\
         **Assignee:** bob\n\n\
         ###### Task ship.gate.perf.micro: Microbenchmarks\n**State:** pending\n\n\
         ###### Task ship.gate.perf.macro: End-to-end timings\n**State:** pending\n\n\
         ### Task announce: Announce it\n**State:** pending\n**Assignee:** alice\n\n\
         #### Task announce.draft: Draft the post\n**State:** completed\n\n\
         #### Task announce.send: Send it\n**State:** pending\n\n\
         ### Task archive: Archive the artifacts\n**State:** pending\n\n\
         #### Task archive.pack: Pack the binaries\n**State:** completed\n\n\
         #### Task archive.upload: Upload them\n**State:** pending\n";

    /// §FS-rhei-memory.6.2 is written as the exact bytes the renderer must
    /// produce, so it is the arbiter of every question the prose leaves open:
    /// three history lines where the tree has seven finished tasks, the fold
    /// clause on the one listed parent, the three `### In Flight` row shapes,
    /// and the sibling suffix beside them.
    // §FS-rhei-memory.3.2 §FS-rhei-memory.4.2 §FS-rhei-memory.4.3
    #[test]
    fn the_depth_three_example_renders_the_bytes_the_spec_states() {
        let (position, history) = fold_sections(
            &[
                ("release.rhei.md", RELEASE_PLAN),
                (
                    "runtime/state-transitions.log",
                    "release.audit.scan pending@completed\n\
                     release.audit.triage pending@cancelled\n\
                     release.audit pending@completed\n\
                     release.announce.draft pending@completed\n\
                     release.ship.notes pending@completed\n\
                     release.archive.pack pending@completed\n\
                     release.ship.gate.lint pending@completed\n",
                ),
                (
                    "runtime/results/release.audit.md",
                    "## Result\n\nLockfile clean after two pins moved.\n",
                ),
                (
                    "runtime/results/release.ship.notes.md",
                    "## Result\n\nNotes at docs/changelog.md under 2.0.\n",
                ),
                (
                    "runtime/results/release.ship.gate.lint.md",
                    "## Result\n\nClean; two allow attributes removed.\n",
                ),
            ],
            "release.rhei.md",
            "release.ship.gate.test",
            &memory_machine(),
            true,
        );

        assert!(
            position.contains(
                "\n### Siblings\n\n\
                 - Task release.ship.gate.lint: Run the linter [completed]\n\
                 - Task release.ship.gate.perf: Run the benchmarks [pending] \u{2014} 2 \
                 subtasks\n"
            ),
            "got:\n{position}"
        );
        assert_eq!(
            history,
            "\n## Plan History\n\nFinished work on the way from the plan's roots to this task, \
             oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's \
             execution root; a folded subtree: `rhei list --parent <id>`.\n\n\
             - Task release.audit: Audit the dependencies \u{2014} completed \u{2014} Lockfile \
             clean after two pins moved. \u{2014} 2 subtasks: 1 completed, 1 cancelled\n\
             - Task release.ship.notes: Write the notes \u{2014} completed \u{2014} Notes at \
             docs/changelog.md under 2.0.\n\
             - Task release.ship.gate.lint: Run the linter \u{2014} completed \u{2014} Clean; \
             two allow attributes removed.\n\
             \n### In Flight\n\n\
             - Task release.ship.gate.perf: Run the benchmarks [pending] \u{2014} bob \u{2014} \
             0 of 2 subtasks finished\n\
             - Task release.announce: Announce it [pending] \u{2014} alice \u{2014} 1 of 2 \
             subtasks finished\n\
             - Task release.archive: Archive the artifacts [pending] \u{2014} 1 of 2 subtasks \
             finished\n"
        );
    }

    /// §FS-rhei-memory.4.3 step 5: the reader's ancestors are excluded by name.
    /// Both of them are non-terminal with a terminal descendant, so the bare
    /// predicate would admit both — `## Position` has already named each one.
    #[test]
    fn every_ancestor_of_the_reader_is_absent_from_in_flight() {
        let history = fold_history(
            &[(
                "plan.rhei.md",
                "# Rhei: Three Deep\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
                 ### Task 1: The root\n**State:** pending\n\n\
                 #### Task 1.1: The middle\n**State:** pending\n\n\
                 ##### Task 1.1.1: The finished one\n**State:** completed\n\n\
                 ##### Task 1.1.2: The reader\n**State:** pending\n",
            )],
            "plan.1.1.2",
        );

        assert!(history.contains("- Task plan.1.1.1: The finished one"), "got:\n{history}");
        assert!(!history.contains("- Task plan.1: The root"), "got:\n{history}");
        assert!(!history.contains("- Task plan.1.1: The middle"), "got:\n{history}");
    }

    /// §FS-rhei-memory.4.3 step 2: a transitive prior is read by name, so it
    /// keeps its line even inside a folded subtree — and is still counted in
    /// full by the parent that folds, because `n` is the size of the subtree
    /// rather than the number of lines it replaced.
    #[test]
    fn a_prior_inside_a_folded_subtree_keeps_its_line_and_its_place_in_the_count() {
        // `rhei next` pastes no prior results, so the prior's own summary shows.
        let (_, history) = fold_sections(
            &[
                (
                    "plan.rhei.md",
                    "# Rhei: Prior In A Fold\n\n---\nstructure:\n  maxLevels: 3\n---\n\n\
                     ## Tasks\n\n\
                     ### Task 1: Decide the approach\n**State:** completed\n\n\
                     #### Task 1.1: Pick the library\n**State:** completed\n\n\
                     #### Task 1.2: Sketch the API\n**State:** completed\n\n\
                     ### Task 2: Build it\n**State:** pending\n**Prior:** 1.1\n",
                ),
                ("runtime/results/plan.1.1.md", "## Result\n\nChose the vendored parser.\n"),
            ],
            "plan.rhei.md",
            "plan.2",
            &memory_machine(),
            false,
        );

        assert!(
            history.contains(
                "- Task plan.1: Decide the approach \u{2014} completed \u{2014} (no result) \
                 \u{2014} 2 subtasks: 2 completed\n"
            ),
            "the fold counts the prior it did not replace; got:\n{history}"
        );
        assert!(
            history.contains(
                "- Task plan.1.1: Pick the library \u{2014} completed \u{2014} Chose the \
                 vendored parser.\n"
            ),
            "a prior is read by name and keeps its line; got:\n{history}"
        );
        // The prior's sibling was never named, so the fold speaks for it alone.
        assert!(!history.contains("- Task plan.1.2"), "got:\n{history}");
    }

    /// §FS-rhei-memory.4.3 step 5: an open parent nobody holds is admitted by a
    /// terminal descendant alone, and then the count is the whole trailing
    /// column — which is how an off-path subtree is accounted for once its
    /// finished children no longer have lines of their own.
    #[test]
    fn an_open_parent_with_no_assignee_reports_its_progress_alone() {
        let mut plan = String::from(
            "# Rhei: Open Parent\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
             ### Task 1: The open parent\n**State:** pending\n\n",
        );
        for index in 1..=5 {
            let state = if index <= 2 { "completed" } else { "pending" };
            plan.push_str(&format!("#### Task 1.{index}: Step {index}\n**State:** {state}\n\n"));
        }
        plan.push_str("### Task 2: The reader\n**State:** pending\n");
        let history = fold_history(&[("plan.rhei.md", plan.as_str())], "plan.2");

        assert_eq!(
            history,
            "\n## Plan History\n\n### In Flight\n\n\
             - Task plan.1: The open parent [pending] \u{2014} 2 of 5 subtasks finished\n"
        );
    }

    /// §FS-rhei-memory.3.2: the breakdown names each descendant's state with the
    /// machine's own name and orders the buckets as the machine declares them,
    /// so a custom terminal shows under its own name — and it buckets *every*
    /// descendant, so a parent finished over children left pending still adds up.
    #[test]
    fn the_breakdown_names_a_custom_terminal_in_the_machines_order() {
        let (_, history) = fold_sections(
            &[(
                "plan.rhei.md",
                "# Rhei: Custom Terminals\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
                 ### Task 1: The release\n**State:** completed\n\n\
                 #### Task 1.1: Finished first\n**State:** completed\n\n\
                 #### Task 1.2: Shipped\n**State:** shipped\n\n\
                 #### Task 1.3: Dropped\n**State:** cancelled\n\n\
                 #### Task 1.4: Never started\n**State:** pending\n\n\
                 ### Task 2: The reader\n**State:** pending\n",
            )],
            "plan.rhei.md",
            "plan.2",
            &custom_terminal_machine(),
            true,
        );

        // Declaration order is pending, shipped, completed, cancelled — not the
        // alphabet and not plan order, either of which would read differently.
        assert!(
            history.contains(
                "- Task plan.1: The release \u{2014} completed \u{2014} (no result) \u{2014} \
                 4 subtasks: 1 pending, 1 shipped, 1 completed, 1 cancelled\n"
            ),
            "got:\n{history}"
        );
    }

    /// §FS-rhei-memory.4.3 step 1 and §FS-rhei-memory.1.1: the eviction this
    /// change exists to stop. Forty results owed to one parent used to fill the
    /// 40-line cap and drop every decision the plan had actually made; folded,
    /// nine entries do not reach the cap at all.
    #[test]
    fn the_cap_no_longer_evicts_a_root_decision_for_a_subtask() {
        let mut plan = String::from("# Rhei: Eviction\n\n---\nstructure:\n  maxLevels: 3\n---\n\n\
             ## Tasks\n\n");
        let mut ledger = String::new();
        for index in 1..=8 {
            plan.push_str(&format!("### Task d{index}: Decision {index}\n**State:** completed\n\n"));
            ledger.push_str(&format!("plan.d{index} pending@completed\n"));
        }
        plan.push_str("### Task big: Sweep every dependency\n**State:** completed\n\n");
        for index in 1..=40 {
            plan.push_str(&format!(
                "#### Task big.s{index:02}: Check dependency {index:02}\n**State:** completed\n\n"
            ));
            ledger.push_str(&format!("plan.big.s{index:02} pending@completed\n"));
        }
        ledger.push_str("plan.big pending@completed\n");
        plan.push_str("### Task later: Write the report\n**State:** pending\n");
        let history = fold_history(
            &[
                ("plan.rhei.md", plan.as_str()),
                ("runtime/state-transitions.log", ledger.as_str()),
            ],
            "plan.later",
        );

        for index in 1..=8 {
            assert!(
                history.contains(&format!("- Task plan.d{index}: Decision {index} \u{2014}")),
                "root decision d{index} is memory the reader is owed; got:\n{history}"
            );
        }
        assert!(
            history.contains(
                "- Task plan.big: Sweep every dependency \u{2014} completed \u{2014} \
                 (no result) \u{2014} 40 subtasks: 40 completed\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("- Task plan.big.s"), "got:\n{history}");
        // Nine entries against a cap of forty: the cap stops binding here.
        assert!(!history.contains("earlier tasks not shown"), "got:\n{history}");
        assert_eq!(history.lines().filter(|line| line.starts_with("- Task ")).count(), 9);
    }

    /// §FS-rhei-memory.4.2 step 2: a sibling that stands for a subtree says how
    /// large it is, after the marker that says it waits — the set, the cap and
    /// the overflow line are unchanged.
    #[test]
    fn a_sibling_with_children_says_how_many() {
        let (position, _) = fold_sections(
            &[(
                "plan.rhei.md",
                "# Rhei: Siblings\n\n---\nstructure:\n  maxLevels: 4\n---\n\n## Tasks\n\n\
                 ### Task 1: The parent\n**State:** pending\n\n\
                 #### Task 1.1: The reader\n**State:** pending\n**Provides:** findings\n\n\
                 #### Task 1.2: A leaf beside it\n**State:** pending\n\n\
                 #### Task 1.3: A subtree that waits\n**State:** pending\n\
                 **Consumes:** 1.1:findings\n\n\
                 ##### Task 1.3.1: Its own child\n**State:** pending\n\n\
                 ##### Task 1.3.2: And another\n**State:** pending\n",
            )],
            "plan.rhei.md",
            "plan.1.1",
            &memory_machine(),
            true,
        );

        assert!(
            position.contains(
                "\n### Siblings\n\n\
                 - Task plan.1.2: A leaf beside it [pending]\n\
                 - Task plan.1.3: A subtree that waits [pending] \u{2014} waits on this task \
                 \u{2014} 2 subtasks\n"
            ),
            "got:\n{position}"
        );
    }
