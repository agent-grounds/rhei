    // The owed clause of the retry paragraph: which artifacts a re-spawned
    // invocation is told it still owes, under which names and in which order,
    // and when the clause is not there at all. §FS-rhei-memory.3.3 §FS-rhei-memory.4.4

    /// `review` declares one output and has an edge into a `final: true`
    /// state — the shape of every agent state of a `grounded-ticket` machine,
    /// so the completion condition is that output plus the ticket's result.
    fn owed_machine(output_path: &str) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(&format!(
            r#"
name: retry-owed
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {{task_id}}.
  review:
    description: Review
    instructions: Review Task {{task_id}}.
    outputs:
      - name: issue
        path: {output_path}
  completed:
    description: Done
    final: true
transitions:
  - {{ from: pending, to: review }}
  - {{ from: review, to: completed }}
"#
        ))
        .expect("machine should parse")
    }

    /// The same state fanned out over two models, each invocation writing its
    /// own fragment and its own declared output.
    fn owed_fanout_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"
name: retry-owed-fanout
version: 1
models: [alpha, beta]
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {task_id}.
  review:
    description: Review
    instructions: Review Task {task_id}.
    all_models: [alpha, beta]
    outputs:
      - name: issue
        path: runtime/triage/{task_id}-{model}.issue.md
  completed:
    description: Done
    final: true
transitions:
  - { from: pending, to: review }
  - { from: review, to: completed }
"#,
        )
        .expect("machine should parse")
    }

    /// The spawn record of one attempt, as the engine leaves it on disk.
    /// §FS-rhei-agents.8.4
    fn write_retry_record(
        root: &Path,
        state: &str,
        suffix: Option<&str>,
        moves: u64,
        code: Option<i32>,
    ) {
        let suffix = suffix.map(|value| format!("-{value}")).unwrap_or_default();
        let log = root.join("runtime").join("logs").join(format!("task-plan.1.3-{state}.log"));
        let record = serde_json::json!({
            "task": "plan.1.3",
            "state": state,
            "moves": moves,
            "attempt": 1,
            "charged": 1,
            "attempt_charged": true,
            "kind": "agent",
            "worker": "mock",
            "log": log,
            "started": "2026-10-01T10:00:00Z",
            "ended": "2026-10-01T10:00:01Z",
            "duration": "1s",
            "code": code,
            "ending": "exited"
        });
        write_under(
            root,
            &format!("runtime/spawns/task-plan.1.3-{state}{suffix}.json"),
            &serde_json::to_string_pretty(&record).expect("serialize spawn record"),
        );
    }

    /// `name (`path`)` with the path spelled the way its author wrote it.
    ///
    /// Not rebuilt component by component: the clause may not re-separate a
    /// path, so an expectation that it does is an expectation against the point
    /// this arm cites. §FS-rhei-agents.4.1 §REQ-cross-platform
    fn owed_entry(name: &str, relative: &str) -> String {
        format!("{name} (`{relative}`)")
    }

    const OWES: &str = " It did not write what this visit still owes: ";

    /// The ticket verbatim (agent-grounds/rhei#376): the declared output is
    /// absent and the result is on disk, and the clause named the result.
    // §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
    #[test]
    fn the_owed_clause_names_a_missing_declared_output_and_not_the_result() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.3 pending@review\n"),
            ("runtime/results/plan.1.3.md", "attempt 1 wrote the result.\n"),
        ]);
        write_retry_record(dir.path(), "review", None, 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = owed_machine("runtime/triage/{task_id}.issue.md");
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let notice = render_retry_notice(&context, dir.path());
        assert!(
            notice.contains(&format!(
                "{OWES}{}.",
                owed_entry("issue", "runtime/triage/plan.1.3.issue.md")
            )),
            "got:\n{notice}"
        );
        assert!(
            !notice.contains("plan.1.3.md`"),
            "the result is on disk and `Result entries so far:` pastes it four lines above this \
             sentence; got:\n{notice}"
        );
    }

    /// The case the clause's old wording was written for, and the one the
    /// narrowed defence of §FS-rhei-memory.3.3 protects: nothing is declared,
    /// so the ticket's result is the whole completion condition.
    ///
    /// A recomputed list reaches it only if the terminal-edge question is asked
    /// the way `## Result` asks it. No transition has been selected when a
    /// prompt is composed, so a collector handed that absence verbatim drops
    /// the result and tells the retry nothing at all.
    // §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
    #[test]
    fn the_owed_clause_still_names_the_result_when_the_result_is_what_went_missing() {
        let dir =
            memory_plan_dir(&[("runtime/state-transitions.log", "plan.1.3 pending@review\n")]);
        write_retry_record(dir.path(), "review", None, 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let notice = render_retry_notice(&context, dir.path());
        assert!(
            notice.contains(&format!(
                "{OWES}{}.",
                owed_entry("result", "runtime/results/plan.1.3.md")
            )),
            "got:\n{notice}"
        );
    }

    /// Declared outputs first, in declaration order, and the ticket's result
    /// last — the order the missing-output warning already prints.
    // §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
    #[test]
    fn the_owed_clause_lists_every_unmet_artifact_in_declaration_order() {
        let dir =
            memory_plan_dir(&[("runtime/state-transitions.log", "plan.1.3 pending@review\n")]);
        write_retry_record(dir.path(), "review", None, 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = owed_machine("runtime/triage/{task_id}.issue.md");
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let notice = render_retry_notice(&context, dir.path());
        assert!(
            notice.contains(&format!(
                "{OWES}{}, {}.",
                owed_entry("issue", "runtime/triage/plan.1.3.issue.md"),
                owed_entry("result", "runtime/results/plan.1.3.md")
            )),
            "got:\n{notice}"
        );
    }

    /// Nothing is owed, so the paragraph claims nothing about any file. This is
    /// the whole paragraph, byte for byte: the retry is told that it is a
    /// retry, how the last attempt ended, and where its transcript is.
    ///
    /// The opening sentence is wrong here — the condition was met and no
    /// transition matched — and that is deliberately not this test's business
    /// (agent-grounds/rhei#376, out of scope by the ticket's own answer).
    // §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
    #[test]
    fn a_retry_that_owes_nothing_names_no_file() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.3 pending@review\n"),
            ("runtime/results/plan.1.3.md", "attempt 1 wrote the result.\n"),
            ("runtime/triage/plan.1.3.issue.md", "attempt 1 wrote the issue.\n"),
        ]);
        write_retry_record(dir.path(), "review", None, 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = owed_machine("runtime/triage/{task_id}.issue.md");
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let transcript = Path::new("runtime").join("logs").join("task-plan.1.3-review.log");
        assert_eq!(
            render_retry_notice(&context, dir.path()),
            format!(
                "\nRetrying this visit: attempt 2. The previous attempt exited 0 without meeting \
                 this state's completion condition. Its transcript is `{}`.\n",
                transcript.display()
            )
        );
    }

    /// A path that still carries a `{...}` template referenced a variable
    /// outside the namespace, which artifact resolution leaves verbatim by
    /// design. The clause says so where the warning says so, and in the same
    /// words, rather than presenting a template as a path that was checked.
    // §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
    #[test]
    fn an_unresolved_output_template_keeps_its_marker() {
        let dir =
            memory_plan_dir(&[("runtime/state-transitions.log", "plan.1.3 pending@review\n")]);
        write_retry_record(dir.path(), "review", None, 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = owed_machine("runtime/triage/{not_a_variable}.issue.md");
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        let notice = render_retry_notice(&context, dir.path());
        // The authored spelling, verbatim: the marker is about the brace the
        // template left, and the path around it is not re-separated to say so.
        // §FS-rhei-agents.4.1
        assert!(
            notice.contains(
                "issue (`runtime/triage/{not_a_variable}.issue.md`, unresolved template)"
            ),
            "got:\n{notice}"
        );
    }

    /// A fanned-out invocation is told about its own fragment and its own
    /// declared output, never a sibling identity's: naming `beta`'s file to
    /// `alpha`'s retry is the defect of this ticket in another direction.
    // §FS-rhei-memory.4.4 §FS-rhei-states.3.3
    #[test]
    fn a_fanned_out_invocation_is_told_about_its_own_fragment() {
        let dir =
            memory_plan_dir(&[("runtime/state-transitions.log", "plan.1.3 pending@review\n")]);
        // `beta` has written nothing either, and that is the point: a state-wide
        // union would print its paths into `alpha`'s retry.
        write_retry_record(dir.path(), "review", Some("alpha"), 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = owed_fanout_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let mut context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");
        context.model = Some("alpha");

        let notice = render_retry_notice(&context, dir.path());
        assert!(
            notice.contains(&owed_entry("issue", "runtime/triage/plan.1.3-alpha.issue.md")),
            "got:\n{notice}"
        );
        assert!(
            !notice.contains("beta"),
            "a sibling identity's files are not this invocation's business; got:\n{notice}"
        );
    }

    /// A record from an *earlier* visit is not a retry, and the new owed list
    /// must not turn one into a retry: re-entering a state is a fresh start.
    // §FS-rhei-memory.4.4
    #[test]
    fn a_record_from_an_earlier_visit_still_renders_nothing() {
        let dir = memory_plan_dir(&[(
            "runtime/state-transitions.log",
            "plan.1.3 pending@review\nplan.1.3 review@review\n",
        )]);
        // Two moves on the ledger, one on the record: the ticket left the state
        // and came back after that spawn. §FS-rhei-agents.8.4
        write_retry_record(dir.path(), "review", None, 1, Some(0));
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = owed_machine("runtime/triage/{task_id}.issue.md");
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "review");

        assert_eq!(render_retry_notice(&context, dir.path()), "");
    }
