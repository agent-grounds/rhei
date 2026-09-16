    // The sections that orient one invocation: where it stands, what already
    // happened to it, and how to reach the rest of the project. The history
    // sections live next door. §FS-rhei-memory.3.1 §FS-rhei-memory.3.3

    /// A machine with one working state, one revisitable state, and two
    /// terminal ones — enough for every condition the memory sections branch on.
    fn memory_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"
name: memory-test
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {task_id}.
  review:
    description: Review
    instructions: Review Task {task_id}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: review }
  - { from: review, to: review }
  - { from: review, to: completed }
  - { from: "*", to: cancelled }
"#,
        )
        .expect("machine should parse")
    }

    /// `<label> 1`..`<label> n`, one per line — a body long enough to trip a cap.
    fn repeated_lines(label: &str, count: usize) -> String {
        let mut out = String::new();
        for n in 1..=count {
            out.push_str(label);
            out.push(' ');
            out.push_str(&n.to_string());
            out.push('\n');
        }
        out
    }

    /// Write a fixture tree and load it exactly as a command would.
    fn memory_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        for (relative, body) in files {
            write_under(dir.path(), relative, body);
        }
        dir
    }

    /// The render context every memory test composes from, with the fixture
    /// directory standing in for both the execution root and the checkout.
    #[allow(clippy::too_many_arguments)]
    fn memory_context<'a>(
        root: &'a Path,
        plan_path: &'a Path,
        loaded: &'a LoadedPlan,
        memory: &'a PromptMemory,
        machine: &'a rhei_validator::StateMachine,
        task: &'a rhei_core::ast::Task,
        state_name: &'a str,
    ) -> RuntimeTemplateContext<'a> {
        RuntimeTemplateContext {
            workspace_root: root,
            task_roots: Some(&loaded.task_roots),
            plan_tasks: Some(&loaded.rhei.tasks),
            checkout_root: root,
            plan_path,
            state_machine_path: None,
            plan_title: &loaded.rhei.title,
            task,
            state_name,
            current_state_raw: task.state.as_str(),
            machine,
            metadata: loaded.rhei.metadata.as_ref(),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("mock"),
            agent_mode: None,
            tooling: None,
            memory: Some(memory),
        }
    }

    /// The single-file rhei most memory tests run against: a parent with three
    /// children, a prior chain, an export, and a claimed sibling.
    const MEMORY_PLAN: &str = r#"# Rhei: Memory Plan

---
structure:
  maxLevels: 3
---

## Notes

Standing note from the plan writer.

## Tasks

### Task 1: Harden the parser
**State:** pending

The decomposition for the whole subtree lives here.
Acceptance: every child lands its own result.

#### Task 1.1: Implement
**State:** completed
**Provides:** findings

#### Task 1.2: Review round 1
**State:** completed
**Prior:** 1.1

#### Task 1.3: Fix round 1
**State:** review
**Prior:** 1.1, 1.2
**Consumes:** 1.1:findings

#### Task 1.4: Review round 2
**State:** pending
**Prior:** 1.3
**Assignee:** codex
"#;

    /// Load `MEMORY_PLAN` plus any extra fixture files, then hand back the
    /// pieces every test needs.
    fn memory_plan_dir(extra: &[(&str, &str)]) -> tempfile::TempDir {
        let mut files = vec![("plan.rhei.md", MEMORY_PLAN)];
        files.extend_from_slice(extra);
        memory_dir(&files)
    }

    /// §FS-rhei-memory.3.1: the chain names the Panta, the rhei, and every
    /// ancestor root-first, and ends in the bold line for this invocation.
    #[test]
    fn position_names_the_chain_down_to_this_invocation() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let position = render_position(&context);
        assert!(
            position.starts_with(
                "\n## Position\n\nPanta: Memory Plan \u{203a} rhei `plan`: Memory Plan \
                 \u{203a} Task plan.1: Harden the parser [pending]\n"
            ),
            "got:\n{position}"
        );
        assert!(
            position.contains(
                "\u{203a} **Task plan.1.3: Fix round 1 [review]** \u{2190} this invocation \
                 (visit 1)"
            ),
            "got:\n{position}"
        );
    }

    /// §FS-rhei-memory.3.1: a root task's chain is the Panta and the rhei
    /// alone, and it gets no sibling list and no parent body.
    #[test]
    fn a_root_task_has_no_siblings_and_no_parent() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1").expect("task 1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let position = render_position(&context);
        assert!(
            position.contains(
                "Panta: Memory Plan \u{203a} rhei `plan`: Memory Plan\n\u{203a} **Task plan.1:"
            ),
            "got:\n{position}"
        );
        assert!(!position.contains("### Siblings"), "got:\n{position}");
        assert!(!position.contains("### Parent"), "got:\n{position}");
    }

    /// §FS-rhei-memory.4.2: siblings render in plan order, and one that lists
    /// this task as a prior — or consumes one of its exports — says so.
    #[test]
    fn siblings_mark_the_ones_that_wait_on_this_task() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let position = render_position(&context);
        assert!(
            position.contains(
                "### Siblings\n\n\
                 - Task plan.1.1: Implement [completed]\n\
                 - Task plan.1.2: Review round 1 [completed]\n\
                 - Task plan.1.4: Review round 2 [pending] \u{2014} waits on this task\n"
            ),
            "got:\n{position}"
        );

        // The export consumer of 1.1 names the producer as a direct prior.
        let producer = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.1").expect("task 1.1");
        let producer_context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, producer, "review");
        let produced = render_position(&producer_context);
        assert!(
            produced.contains(
                "- Task plan.1.3: Fix round 1 [review] \u{2014} waits on this task\n"
            ),
            "got:\n{produced}"
        );
    }

    /// §FS-rhei-memory.3.1: the nearest ancestor's body is pasted in full and
    /// fenced; higher ancestors contribute one chain line and nothing more.
    #[test]
    fn the_parent_body_is_pasted_and_fenced() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let position = render_position(&context);
        assert!(
            position.contains("### Parent: Task plan.1: Harden the parser\n\n```markdown\n"),
            "got:\n{position}"
        );
        assert!(
            position.contains("Acceptance: every child lands its own result."),
            "got:\n{position}"
        );
    }

    /// §FS-rhei-memory.3.1: a bare rhei's own content sections are its Rhei
    /// Context, and its implicit Panta contributes no Project Context.
    #[test]
    fn a_bare_rhei_has_rhei_context_and_no_project_context() {
        let dir = memory_plan_dir(&[]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let position = render_position(&context);
        assert!(position.contains("### Rhei Context"), "got:\n{position}");
        assert!(position.contains("## Notes"), "got:\n{position}");
        assert!(position.contains("Standing note from the plan writer."), "got:\n{position}");
        assert!(!position.contains("### Project Context"), "got:\n{position}");
    }

    /// §FS-rhei-memory.4.2: `### Rhei Context` is verbatim — the pasted block
    /// is the bytes between the section heading and the next one, paragraph
    /// breaks, lists, and fences included.
    #[test]
    fn rhei_context_pastes_the_authored_block_byte_for_byte() {
        let authored = "First paragraph.\n\nSecond paragraph.\n\n- item one\n- item two\n\n\
                        ```sh\necho hi\n\necho bye\n```\n\nTail paragraph.";
        let plan = format!(
            "# Rhei: Verbatim\n\n## Notes\n\n{authored}\n\n## Tasks\n\n\
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
        // The body holds a triple-backtick run, so the fence is four.
        assert!(
            position.contains(&format!("````markdown\n## Notes\n\n{authored}\n````")),
            "got:\n{position}"
        );
    }

    /// A machine with two counted loops and a supervising state, so a
    /// `**State:**` line can carry the `-<visit>` suffix the engine writes.
    fn counted_loop_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"
name: counted-loops
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {task_id}.
  work:
    description: Work
    visits: 3
    instructions: Work on Task {task_id}.
  fix:
    description: Fix
    visits: 3
    instructions: Fix Task {task_id}.
  supervising:
    description: Supervise
    instructions: Supervise Task {task_id}.
  completed:
    description: Done
    final: true
transitions:
  - { from: pending, to: work }
  - { from: work, to: fix }
  - { from: fix, to: work }
  - { from: supervising, to: supervising }
  - { from: "*", to: completed }
  - { from: supervising, to: completed }
  - { from: fix, to: completed }
"#,
        )
        .expect("machine should parse")
    }

    /// A parent mid-supervision, a child mid-loop, a `bug` sibling mid-loop,
    /// and a finished `bug` for the history to summarize.
    const COUNTED_LOOP_PLAN: &str = r#"# Rhei: Loops

---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Parent
**State:** supervising-3

The decomposition for the whole subtree lives here.

#### Task 1.1: This one
**State:** work-3

#### Bug 1.2: Flaky teardown
**State:** fix-2
**Assignee:** codex
**Prior:** 1.1

#### Bug 1.3: Fixed already
**State:** completed
"#;

    /// One form for a state name across the whole prompt. A counted loop writes
    /// `work-3` into `**State:**`; the invocation's own state is already
    /// normalized, so a neighbour's raw value read as a different machine.
    // §FS-rhei-memory.4.5
    #[test]
    fn counted_loop_suffixes_are_normalized_in_every_section() {
        let dir = memory_dir(&[
            ("plan.rhei.md", COUNTED_LOOP_PLAN),
            ("runtime/state-transitions.log", "plan.1.3 fix@completed\n"),
            ("runtime/results/plan.1.3.md", "## Result\n\nTeardown no longer races.\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = counted_loop_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.1").expect("task 1.1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "work");

        let position = render_position(&context);
        assert!(
            position.contains("\u{203a} Task plan.1: Parent [supervising]\n"),
            "the chain normalizes an ancestor's state; got:\n{position}"
        );
        assert!(
            position.contains("- Bug plan.1.2: Flaky teardown [fix] \u{2014} waits on this task\n"),
            "a sibling's state is normalized; got:\n{position}"
        );

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Bug plan.1.3: Fixed already \u{2014} completed \u{2014} Teardown no longer \
                 races.\n"
            ),
            "got:\n{history}"
        );
        assert!(
            history.contains("- Bug plan.1.2: Flaky teardown [fix] \u{2014} codex\n"),
            "`### In Flight` normalizes too; got:\n{history}"
        );

        assert!(
            history.contains(
                "### Dependents\n\n- Bug plan.1.2: Flaky teardown [fix] \u{2014} prior\n"
            ),
            "`### Dependents` normalizes too; got:\n{history}"
        );

        // §FS-rhei-supervision.5.1: the pre-existing `## Child Tasks` map, which
        // printed the raw value beside the normalized one.
        let parent = find_task_by_id_str(&loaded.rhei.tasks, "plan.1").expect("task 1");
        let parent_context = memory_context(
            dir.path(),
            &plan_path,
            &loaded,
            &memory,
            &machine,
            parent,
            "supervising",
        );
        let prompt = compose_agent_prompt(&parent_context).expect("prompt");
        assert!(
            prompt.contains(
                "## Child Tasks\n\n\
                 - Task plan.1.1: This one [work]\n\
                 - Bug plan.1.2: Flaky teardown [fix]\n\
                 - Bug plan.1.3: Fixed already [completed]\n"
            ),
            "got:\n{prompt}"
        );
    }

    /// §FS-rhei-memory.3.1 §FS-rhei-memory.3.2: a node is named by its own kind,
    /// title-cased — the form `## Child Tasks` and `rhei list` already print.
    #[test]
    fn a_node_is_named_by_its_own_kind() {
        let dir = memory_dir(&[
            ("plan.rhei.md", COUNTED_LOOP_PLAN),
            ("runtime/state-transitions.log", "plan.1.3 fix@completed\n"),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = counted_loop_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.1").expect("task 1.1");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "work");

        let position = render_position(&context);
        assert!(position.contains("- Bug plan.1.2: Flaky teardown"), "got:\n{position}");
        assert!(
            position.contains("### Parent: Task plan.1: Parent\n"),
            "the parent heading carries its kind too; got:\n{position}"
        );
        assert!(
            position.contains("\u{203a} **Task plan.1.1: This one [work]**"),
            "got:\n{position}"
        );

        let history = render_plan_history(&context).expect("history");
        assert!(history.contains("- Bug plan.1.3: Fixed already \u{2014} completed"), "got:\n{history}");
    }
