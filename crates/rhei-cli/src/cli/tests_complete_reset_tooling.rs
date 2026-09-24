    /// Whitespace-only is no result: an existence-only contract would let an
    /// empty file stand in for an answer. §FS-rhei-states.3.3
    #[test]
    fn a_result_file_counts_only_when_it_has_content() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = dir.path();
        let results = root.join("runtime").join("results");
        fs::create_dir_all(&results).expect("results dir");

        assert!(!task_result_is_present(root, "plan.1"), "no file at all");

        fs::write(results.join("plan.1.md"), "").expect("write empty");
        assert!(!task_result_is_present(root, "plan.1"), "an empty file is no result");

        fs::write(results.join("plan.1.md"), "\n \t\n").expect("write blank");
        assert!(!task_result_is_present(root, "plan.1"), "whitespace is no result");

        fs::write(results.join("plan.1.md"), "## Result\n\nDone.\n").expect("write result");
        assert!(task_result_is_present(root, "plan.1"), "content is a result");
    }

    /// The obligation is a property of the target state, satisfied by a message
    /// the caller carried or by a result already on disk — and by nothing else.
    // §FS-rhei-states.3.3 §FS-rhei-transition-cmd.3.2
    #[test]
    fn terminal_entry_requires_a_message_or_a_result_on_disk() {
        let yaml = "name: t\nversion: 1\nstates:\n  pending:\n    description: p\n  completed:\n    description: c\n    final: true\ntransitions:\n  - from: pending\n    to: completed\n";
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("machine");
        let dir = tempfile::tempdir().expect("tmpdir");
        let root = dir.path();

        let plan = root.join("plan.rhei.md");

        // A non-terminal target never asks.
        assert!(ensure_terminal_result_available(
            &machine, root, "plan.1", "pending", "pending", None, &plan
        )
        .is_ok());

        let err = ensure_terminal_result_available(
            &machine,
            root,
            "plan.1",
            "pending",
            "completed",
            None,
            &plan,
        )
        .expect_err("a terminal entry with nothing to say is refused");
        let rendered = format!("{err:?}");
        assert!(rendered.contains("runtime/results/plan.1.md"), "{rendered}");
        assert!(rendered.contains("--result"), "{rendered}");
        // The suggested commands name the plan, so they run from anywhere.
        // §FS-rhei-errors.2
        assert!(rendered.contains("plan.rhei.md"), "{rendered}");

        // A blank message is not a message.
        assert!(ensure_terminal_result_available(
            &machine,
            root,
            "plan.1",
            "pending",
            "completed",
            Some("   "),
            &plan
        )
        .is_err());

        assert!(ensure_terminal_result_available(
            &machine,
            root,
            "plan.1",
            "pending",
            "completed",
            Some("Shipped it."),
            &plan
        )
        .is_ok());

        let results = root.join("runtime").join("results");
        fs::create_dir_all(&results).expect("results dir");
        fs::write(results.join("plan.1.md"), "## Result\n\nThe worker wrote this.\n")
            .expect("write result");
        assert!(ensure_terminal_result_available(
            &machine, root, "plan.1", "pending", "completed", None, &plan
        )
        .is_ok());
    }

    #[test]
    fn paths_equivalent_falls_back_for_nonexistent_relative_paths() {
        assert!(paths_equivalent(
            Path::new("./docs/markdown-plan-compiler.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
        assert!(!paths_equivalent(
            Path::new("./docs/plan-language-spec.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
    }

    // Unix: the two spellings are made with a symlink, which an unprivileged
    // Windows process cannot create. §REQ-cross-platform.3
    #[cfg(unix)]
    #[test]
    fn paths_equivalent_canonicalizes_both_existing_paths() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tmpdir");
        let real = dir.path().join("real");
        let alias = dir.path().join("alias");
        fs::create_dir_all(&real).expect("real dir");
        fs::write(real.join("plan.rhei.md"), "# Rhei: Test\n").expect("plan");
        symlink(&real, &alias).expect("symlink");

        assert!(paths_equivalent(&alias.join("plan.rhei.md"), &real.join("plan.rhei.md")));
    }

    #[test]
    fn should_revalidate_filters_irrelevant_events() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/states.yaml"),
        );

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(should_revalidate(&event, &watched));

        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(!should_revalidate(&event, &watched));
    }

    #[test]
    fn should_revalidate_matches_workspace_task_files() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let workspace = dir.path().join("workspace");
        let tasks = workspace.join("tasks");
        fs::create_dir_all(&tasks).expect("workspace tasks dir");
        fs::write(workspace.join("index.rhei.md"), "# Rhei: Watch\n").expect("index");
        let task_path = tasks.join("alpha.md");
        fs::write(&task_path, "### Task 1: Alpha\n**State:** pending\n").expect("task");

        let watch_plan = validation_watch_plan(&workspace, None);
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![task_path],
            attrs: Default::default(),
        };

        assert!(should_revalidate(&event, &watch_plan.targets));
    }

    #[test]
    fn validation_watch_plan_does_not_require_a_valid_initial_plan() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let broken_plan = dir.path().join("broken.rhei.md");

        let watch_plan = validation_watch_plan(&broken_plan, None);

        assert!(path_matches(&broken_plan, &watch_plan.targets));
        assert!(watch_plan
            .roots
            .iter()
            .any(|root| root.path == canonical_watch_path(dir.path())
                && root.mode == RecursiveMode::NonRecursive));
    }

    #[test]
    fn watch_targets_match_created_relative_missing_plan() {
        fs::create_dir_all("target").expect("target dir");
        let current_thread = std::thread::current();
        // A test thread is named after its module path, and `:` is not a legal
        // character in a Windows filename. #91
        let thread_name: String = current_thread
            .name()
            .unwrap_or("unnamed")
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect();
        let rel_path = PathBuf::from(format!(
            "target/rhei-watch-missing-{}-{}.rhei.md",
            std::process::id(),
            thread_name
        ));
        let _ = fs::remove_file(&rel_path);

        let watch_plan = validation_watch_plan(&rel_path, None);
        fs::write(&rel_path, "# Rhei: Created Later\n\n## Tasks\n").expect("create plan");
        let created_path = rel_path.canonicalize().expect("canonical created plan");

        assert!(path_matches(&created_path, &watch_plan.targets));

        fs::remove_file(&rel_path).expect("cleanup created plan");
    }

    #[test]
    fn parses_complete_command_with_result() {
        let cli = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "3",
            "--result",
            "All tests pass",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Complete { input, task, result, no_callbacks, .. } => {
                assert_eq!(input, Some(PathBuf::from("plan.rhei.md")));
                assert_eq!(task.as_deref(), Some("3"));
                assert_eq!(result, "All tests pass");
                assert!(!no_callbacks);
            }
            other => panic!("expected complete command, got {other:?}"),
        }
    }

    /// §FS-rhei-complete.2.1: the positional doubles as the ticket id when it
    /// is id-shaped and names no existing path.
    #[test]
    fn complete_positional_splits_between_ticket_and_plan() {
        // Id-shaped, no such path: the positional is the ticket.
        let (input, task) =
            split_complete_ticket_target(Some(PathBuf::from("auth.1")), None).expect("ticket form");
        assert_eq!(input, None);
        assert_eq!(task, "auth.1");

        // --task present: the positional stays the plan path.
        let (input, task) =
            split_complete_ticket_target(Some(PathBuf::from("auth.1")), Some("3".to_string()))
                .expect("legacy form");
        assert_eq!(input, Some(PathBuf::from("auth.1")));
        assert_eq!(task, "3");

        // An existing path without --task asks for the ticket.
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = dir.path().join("plan.rhei.md");
        std::fs::write(&plan, "# Rhei: X\n").expect("write");
        let err = split_complete_ticket_target(Some(plan), None).expect_err("plan without ticket");
        assert!(err.to_string().contains("name the ticket"), "got: {err}");

        // Neither positional nor --task: the error shows the positional form.
        let err = split_complete_ticket_target(None, None).expect_err("no ticket at all");
        assert!(err.to_string().contains("rhei complete <ticket-id>"), "got: {err}");

        // Not id-shaped and not an existing path: reported as a missing plan.
        let err = split_complete_ticket_target(Some(PathBuf::from("missing/plan.rhei.md")), None)
            .expect_err("missing plan path");
        assert!(err.to_string().contains("does not exist"), "got: {err}");
    }

    #[test]
    fn parses_complete_command_requires_result() {
        // --result is mandatory; omitting it should fail.
        let err = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "build",
            "--no-callbacks",
        ]);
        assert!(err.is_err(), "complete without --result should fail");
    }

    #[test]
    fn parses_reset_command() {
        let cli = Cli::try_parse_from(["rhei", "reset", "workspace"]).expect("cli should parse");

        match cli.command {
            Commands::Reset { input, .. } => {
                assert_eq!(input, Some(PathBuf::from("workspace")));
            }
            other => panic!("expected reset command, got {other:?}"),
        }
    }

    #[test]
    fn find_completion_state_prefers_non_cancelled_terminal() {
        let yaml = r#"
name: test
version: 1
states:
  active: { description: "working" }
  completed: { description: "done", final: true }
  cancelled: { description: "nope", final: true }
transitions:
  - from: active
    to: cancelled
  - from: active
    to: completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let target = find_completion_state("active", &machine);
        assert_eq!(target.as_deref(), Some("completed"));
    }

    #[test]
    fn find_completion_state_does_not_fall_back_to_cancelled() {
        let yaml = r#"
name: test
version: 1
states:
  active: { description: "working" }
  cancelled: { description: "nope", final: true }
transitions:
  - from: active
    to: cancelled
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let target = find_completion_state("active", &machine);
        assert!(target.is_none(), "complete should not treat cancellation as success");
    }

    #[test]
    fn find_completion_state_returns_none_when_no_terminal_reachable() {
        let yaml = r#"
name: test
version: 1
states:
  draft: { description: "initial", initial: true }
  pending: { description: "ready" }
  completed: { description: "done", final: true }
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        // draft can only go to pending (non-terminal), not directly to completed
        let target = find_completion_state("draft", &machine);
        assert!(target.is_none());
    }

    #[test]
    fn complete_command_blocks_gating_state() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        fs::write(
            &plan,
            "# Rhei: Gate\n\n## Tasks\n\n### Task 1: Review\n**State:** human-review\n\nReview.\n",
        )
        .expect("write plan");
        fs::write(
            &states,
            "name: test\nversion: 1\nstates:\n  human-review:\n    description: review\n    gating: true\n  completed:\n    description: done\n    final: true\ntransitions:\n  - from: human-review\n    to: completed\n",
        )
        .expect("write states");

        let err = complete_command(&plan, &[], Some(&states), "1", "done", true)
            .expect_err("gating completion must fail");
        assert!(err.to_string().contains("gating state"), "{err}");
    }

    /// The gate block carries an optional result field, so the human who
    /// releases a gate says why on the move. Left blank on a terminal choice,
    /// the shared path refuses it and names the command that can carry the
    /// reason; a non-terminal choice is unaffected either way.
    // §FS-rhei-viz.5.1 §FS-rhei-run.3 §FS-rhei-states.3.3
    #[test]
    fn dashboard_gate_transition_refuses_a_terminal_choice_with_no_result() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        fs::write(
            &plan,
            "# Rhei: Gate\n\n## Tasks\n\n### Task 1: Review\n**State:** human-review\n\nReview.\n",
        )
        .expect("write plan");
        let yaml = "name: test\nversion: 1\nstates:\n  human-review:\n    description: review\n    gating: true\n  rework:\n    description: back to work\n  completed:\n    description: done\n    final: true\ntransitions:\n  - from: human-review\n    to: rework\n  - from: human-review\n    to: completed\n  - from: rework\n    to: completed\n";
        fs::write(&states, yaml).expect("write states");
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("machine");
        let callback_paths = resolve_callback_paths(Some(&states), &plan).expect("callbacks");

        // The dashboard passes ids from the loaded graph, which implicit Panta
        // qualifies with the rhei id derived from the plan file stem (`plan`).
        let err = transition_dashboard_gate(
            &plan,
            &machine,
            &callback_paths,
            "plan.1",
            "human-review",
            "completed",
            None,
            true,
        )
        .expect_err("a terminal gate choice carries no result");
        let rendered = format!("{err:?}");
        assert!(rendered.contains("without a result"), "{rendered}");
        assert!(rendered.contains("rhei transition"), "{rendered}");
        let untouched = fs::read_to_string(&plan).expect("read plan");
        assert!(untouched.contains("**State:** human-review"), "{untouched}");

        // A whitespace-only field is the same as no field at all.
        assert!(transition_dashboard_gate(
            &plan,
            &machine,
            &callback_paths,
            "plan.1",
            "human-review",
            "completed",
            Some("   "),
            true,
        )
        .is_err());

        let effective = transition_dashboard_gate(
            &plan,
            &machine,
            &callback_paths,
            "plan.1",
            "human-review",
            "rework",
            None,
            true,
        )
        .expect("a non-terminal gate choice is unaffected");

        assert_eq!(effective, "rework");
        let updated = fs::read_to_string(&plan).expect("read plan");
        assert!(updated.contains("**State:** rework"));
        let history =
            fs::read_to_string(dir.path().join("runtime/state-transitions.log")).expect("history");
        assert!(history.contains("plan.1 human-review@rework"));
    }

    /// The gate's own result field finishes the ticket: the message rides the
    /// transition, lands in the ticket's result file as a `## Result` entry, and
    /// the everyday `agent → human-gate → completed` shape works from the UI.
    // §FS-rhei-viz.5.1 §FS-rhei-states.3.3
    #[test]
    fn dashboard_gate_transition_carries_the_result_into_a_terminal_state() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        fs::write(
            &plan,
            "# Rhei: Gate\n\n## Tasks\n\n### Task 1: Review\n**State:** human-review\n\nReview.\n",
        )
        .expect("write plan");
        let yaml = "name: test\nversion: 1\nstates:\n  human-review:\n    description: review\n    gating: true\n  completed:\n    description: done\n    final: true\ntransitions:\n  - from: human-review\n    to: completed\n";
        fs::write(&states, yaml).expect("write states");
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("machine");
        let callback_paths = resolve_callback_paths(Some(&states), &plan).expect("callbacks");

        let effective = transition_dashboard_gate(
            &plan,
            &machine,
            &callback_paths,
            "plan.1",
            "human-review",
            "completed",
            Some("  Reviewed by hand; shipping it.  "),
            true,
        )
        .expect("a terminal gate choice with a result is applied");

        assert_eq!(effective, "completed");
        let updated = fs::read_to_string(&plan).expect("read plan");
        assert!(updated.contains("**State:** completed"), "{updated}");
        let result = fs::read_to_string(dir.path().join("runtime/results/plan.1.md"))
            .expect("result file");
        assert_eq!(result.trim(), "## Result\n\nReviewed by hand; shipping it.");
    }

    #[test]
    fn rewrite_task_completion_removes_assignee_and_appends_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"# Rhei: Test

## Tasks

### Task 1: Alpha
**State:** completed
**Assignee:** agent-1
Some work description.
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", "1", "runtime/results/1.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"), "assignee should be removed");
        assert!(
            content.contains("> **Result:** [1](runtime/results/1.md)"),
            "result link should be appended"
        );
        // State line should remain
        assert!(content.contains("**State:** completed"));
    }

    #[test]
    fn rewrite_task_completion_refreshes_existing_legacy_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"# Rhei: Test

## Tasks

### Task 1: Alpha
**State:** completed

> **Result:** [1](runtime/results/1.md)

Some work description.
"#,
        )
        .expect("write");

        // §FS-rhei-panta.6.3: completion owns this ticket's result link, so a
        // stale legacy link is replaced by the file this completion wrote.
        rewrite_task_completion(&path, "1", "plan.1", "runtime/results/plan.1.md", true)
            .expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(
            content.contains("> **Result:** [plan.1](runtime/results/plan.1.md)"),
            "legacy link should be refreshed: {content}"
        );
        assert!(
            !content.contains("> **Result:** [1](runtime/results/1.md)"),
            "stale legacy link should be gone: {content}"
        );
        assert_eq!(
            content.matches("> **Result:**").count(),
            1,
            "exactly one result block should remain: {content}"
        );
    }

    #[test]
    fn rewrite_task_completion_without_assignee_still_appends_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 1: Alpha
**State:** completed
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", "1", "runtime/results/1.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("> **Result:** [1](runtime/results/1.md)"));
    }

    #[test]
    fn rewrite_states_to_authored_restores_recorded_states() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Alpha
**State:** completed

#### Task 1.1: Detail
**State:** in-progress

### Task 2: Beta
**State:** review
"#;
        let authored: BTreeMap<String, String> = [
            ("1".to_string(), "pending".to_string()),
            ("1.1".to_string(), "pending".to_string()),
            ("2".to_string(), "pending".to_string()),
        ]
        .into_iter()
        .collect();
        let rewritten = rewrite_states_to_authored(raw, &authored).expect("rewrite states");

        assert_eq!(rewritten.matches("**State:** pending").count(), 3);
        assert!(!rewritten.contains("**State:** completed"));
        assert!(!rewritten.contains("**State:** in-progress"));
        assert!(!rewritten.contains("**State:** review"));
    }

    /// The §FS-rhei-reset.2.2 rule the supervised chain depends on: a task with
    /// no recorded authored state never moved, so its line is left verbatim
    /// rather than pulled to the machine's one `initial: true` state.
    #[test]
    fn rewrite_states_to_authored_leaves_tasks_that_never_moved() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Supervisor
**State:** supervising-3

#### Task 1.1: Implement
**State:** implement

#### Task 1.2: Review
**State:** review
"#;
        // Only the supervisor ever moved.
        let authored: BTreeMap<String, String> =
            [("1".to_string(), "supervising".to_string())].into_iter().collect();
        let rewritten = rewrite_states_to_authored(raw, &authored).expect("rewrite states");

        // The supervisor goes back to its authored state, losing the counted
        // visit suffix.
        assert!(rewritten.contains("### Task 1: Supervisor\n**State:** supervising\n"));
        // The children are untouched — this is the regression rhei#100 filed.
        assert!(rewritten.contains("#### Task 1.1: Implement\n**State:** implement"));
        assert!(rewritten.contains("#### Task 1.2: Review\n**State:** review"));
        assert!(!rewritten.contains("**State:** supervising-3"));
    }

    /// The ledger is append-only, so the FIRST `from` recorded for a task is
    /// the state it started in; later lines are later moves.
    // §FS-rhei-reset.2.2
    #[test]
    fn ledger_first_departures_keeps_the_earliest_departure_per_task() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        fs::create_dir_all(&runtime).expect("create runtime");
        fs::write(
            runtime.join("state-transitions.log"),
            "a.1 implement@completed\na.2 review@fix\na.1 completed@cancelled\n",
        )
        .expect("write ledger");

        let first = ledger_first_departures(dir.path());

        assert_eq!(first.get("a.1").map(String::as_str), Some("implement"));
        assert_eq!(first.get("a.2").map(String::as_str), Some("review"));
    }

    /// A ledger line reset cannot parse is skipped, never guessed at: a bad
    /// line must not become an authored state.
    #[test]
    fn ledger_first_departures_skips_lines_it_cannot_parse() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        fs::create_dir_all(&runtime).expect("create runtime");
        fs::write(
            runtime.join("state-transitions.log"),
            "\nonly-an-id\nb.1 no-at-sign\nb.2 @completed\nb.3 review@fix\n",
        )
        .expect("write ledger");

        let first = ledger_first_departures(dir.path());

        assert_eq!(first.len(), 1, "only the well-formed line counts: {first:?}");
        assert_eq!(first.get("b.3").map(String::as_str), Some("review"));
    }

    /// No ledger is no history, not an error — the case every never-run plan
    /// is in. §FS-rhei-reset.2.2
    #[test]
    fn ledger_first_departures_is_empty_without_a_ledger() {
        let dir = tempfile::tempdir().expect("tmpdir");
        assert!(ledger_first_departures(dir.path()).is_empty());
    }

    /// A plan with no `**State:**` line at all is the "wrong file" mistake and
    /// still errors; zero *rewrites* is now an ordinary outcome.
    #[test]
    fn rewrite_states_to_authored_rejects_a_plan_with_no_states() {
        let raw = "# Rhei: Reset\n\n## Tasks\n\nNo tasks here.\n";
        let authored: BTreeMap<String, String> = BTreeMap::new();
        assert!(rewrite_states_to_authored(raw, &authored).is_err());
    }

    #[test]
    fn rewrite_task_completion_inserts_result_link_before_child() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 2: Beta
**State:** completed

Body text.

#### Task 2.1: Sub
**State:** completed
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "2", "2", "runtime/results/2.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        let result_pos =
            content.find("> **Result:** [2](runtime/results/2.md)").expect("result present");
        let child_pos = content.find("#### Task 2.1").expect("child present");
        assert!(result_pos < child_pos, "result should appear before child");
    }

    #[test]
    fn rewrite_task_completion_matches_custom_node_kind() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Bug cache-key: Fix cache
**State:** completed
**Assignee:** codex

Body text.
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "cache-key", "cache-key", "runtime/results/cache-key.md", true)
            .expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"), "assignee should be removed");
        assert!(content.contains("> **Result:** [cache-key](runtime/results/cache-key.md)"));
    }

    #[test]
    fn rewrite_task_completion_ignores_task_shaped_heading_inside_code_fence() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 1: Parent
**State:** completed

```markdown
#### Task 1.1: Example
**State:** completed
```

#### Task 1.1: Real child
**State:** completed
**Assignee:** codex
Body.
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1.1", "1.1", "runtime/results/1.1.md", true)
            .expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("#### Task 1.1: Example\n**State:** completed\n```"));
        assert!(!content.contains("**Assignee:** codex"));
        assert!(content.contains("#### Task 1.1: Real child\n**State:** completed\nBody."));
        assert!(content.contains("> **Result:** [1.1](runtime/results/1.1.md)"));
    }

    #[test]
    fn write_task_assignee_rechecks_required_inputs() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("task.md");
        fs::write(&path, "### Task 1: Needs input\n**State:** fix\n").expect("write");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: input-recheck
version: 1
states:
  fix:
    description: Fix
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
transitions:
  - from: fix
    to: completed
"#,
        )
        .expect("load machine");
        let settings = load_merged_settings(dir.path()).expect("settings");
        let state_def = machine.states.get("fix").expect("fix state");

        let err = write_task_assignee(
            &path,
            "1",
            "plan.1",
            "fix",
            &machine,
            TaskAssigneeClaimContext {
                workspace_root: dir.path(),
                metadata: None,
                structure: None,
                state_def,
                settings: &settings,
            },
            "codex",
        )
        .expect_err("missing input should block claim");
        assert!(err.to_string().contains("Missing required input artifact: findings"));
        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"));
    }

    #[test]
    fn reset_rewrite_strips_assignees() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Alpha
**State:** completed
**Assignee:** codex

#### Task 1.1: Detail
**State:** completed
**Assignee:** claude-code
"#;
        let authored: BTreeMap<String, String> = [
            ("1".to_string(), "draft".to_string()),
            ("1.1".to_string(), "draft".to_string()),
        ]
        .into_iter()
        .collect();
        let rewritten = rewrite_states_to_authored(raw, &authored).expect("rewrite states");
        let rewritten = strip_assignee_lines(&rewritten);

        assert_eq!(rewritten.matches("**State:** draft").count(), 2);
        assert!(!rewritten.contains("**Assignee:**"));
    }

    #[test]
    fn clap_command_factory_builds() {
        Cli::command().debug_assert();
    }

    // ---- MCP / skills resolution ----

    fn machine_with_tooling(state_yaml: &str) -> rhei_validator::StateMachine {
        let yaml = format!(
            "name: tooling-test\nversion: 1\nstates:\n{state_yaml}\n  completed:\n    description: done\n    final: true\ntransitions:\n  - from: pending\n    to: completed\n"
        );
        rhei_validator::StateMachine::from_yaml_str(&yaml).expect("valid state machine")
    }

    fn settings_with(
        defaults_mcp: Option<Vec<StateMcpEntry>>,
        registry: BTreeMap<String, McpServerProfile>,
    ) -> RheiSettings {
        RheiSettings {
            project_settings_file: ProjectSettingsFile::default(),
            agent: None,
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults {
                model: None,
                agent: None,
                agent_mode: None,
                agent_timeout: None,
                program_timeout: None,
                attempts: None,
                mcp_servers: defaults_mcp,
                skills: None,
                ..Default::default()
            },
            agents: built_in_agents(),
            models: BTreeMap::new(),
            mcp_servers: registry,
            skills: BTreeMap::new(),
            snapshots: None,
            ..Default::default()
        }
    }

    #[test]
    fn resolve_tooling_unions_defaults_with_state_overrides_by_id() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: linear
        optional: true
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("linear".to_string(), McpServerProfile::default());
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings = settings_with(
            Some(vec![
                StateMcpEntry::Id("postgres".to_string()),
                StateMcpEntry::Id("linear".to_string()),
            ]),
            registry,
        );

        let tooling = resolve_tooling(&machine, "pending", &settings);
        // postgres from defaults stays first; linear from defaults is replaced
        // by the state-level entry that flips optional to true.
        let ids: Vec<&str> = tooling.mcp_servers.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["postgres", "linear"]);
        let linear = tooling.mcp_servers.iter().find(|e| e.id == "linear").unwrap();
        assert!(linear.optional, "state override should win");
        assert!(linear.definition.is_some(), "registry entry resolves");
    }

    #[test]
    fn resolve_tooling_empty_state_list_clears_defaults() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers: []
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings =
            settings_with(Some(vec![StateMcpEntry::Id("postgres".to_string())]), registry);

        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert!(tooling.mcp_servers.is_empty(), "explicit empty clears defaults");
    }

    #[test]
    fn resolve_tooling_omitted_state_inherits_defaults() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings =
            settings_with(Some(vec![StateMcpEntry::Id("postgres".to_string())]), registry);

        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        assert_eq!(tooling.mcp_servers[0].id, "postgres");
    }

    #[test]
    fn resolve_tooling_inline_definition_does_not_require_registry() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: adhoc
        command: ["mcp-adhoc", "--port", "8080"]
"#,
        );
        let settings = settings_with(None, BTreeMap::new());
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        let entry = &tooling.mcp_servers[0];
        assert_eq!(entry.id, "adhoc");
        assert!(entry.definition.is_some(), "inline definition resolves");
        assert_eq!(entry.definition.as_ref().unwrap().command.as_deref().unwrap()[0], "mcp-adhoc");
    }

    #[test]
    fn resolve_tooling_unknown_id_resolves_to_unavailable() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers: [missing]
"#,
        );
        let settings = settings_with(None, BTreeMap::new());
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        assert!(
            tooling.mcp_servers[0].definition.is_none(),
            "unknown id has no definition (Half B reports it as unavailable)"
        );
        assert!(!tooling.mcp_available("missing"));
    }

    #[test]
    fn env_id_segment_normalizes_id() {
        assert_eq!(env_id_segment("linear"), "LINEAR");
        assert_eq!(env_id_segment("ad-hoc"), "AD_HOC");
        assert_eq!(env_id_segment("foo bar"), "FOO_BAR");
        assert_eq!(env_id_segment("a.b.c"), "A_B_C");
    }

    #[test]
    fn format_tooling_log_line_marks_unavailable_optional_with_question_mark() {
        let entries = vec![
            ResolvedMcpEntry {
                id: "postgres".to_string(),
                optional: false,
                definition: Some(McpServerProfile::default()),
            },
            ResolvedMcpEntry { id: "grafana".to_string(), optional: true, definition: None },
        ];
        let line = format_tooling_log_line(&entries, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        assert_eq!(line.as_deref(), Some("postgres,grafana?"));
    }
