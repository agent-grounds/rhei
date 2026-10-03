    // The `unresolved template` marker: decided once per entry from the authored relative path, and
    // read unchanged by the warning and the retry prompt (rhei#399). §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4

    /// The plan's task `plan.1.3`, which sits in `review`, loaded from a
    /// fixture that is kept alive beside it.
    fn marker_task() -> (tempfile::TempDir, LoadedPlan) {
        let dir = memory_plan_dir(&[]);
        let loaded = load_plan(&dir.path().join("plan.rhei.md")).expect("plan loads");
        (dir, loaded)
    }

    /// A root whose directory name holds a brace, the way a checkout under
    /// `co{br}` does. Nothing authored put it there.
    fn braced_root(dir: &Path) -> PathBuf {
        dir.join("co{br}").join("r")
    }

    /// The result's warning line, spelled the way the warning spells it, with
    /// or without the marker.
    fn result_warning(path: &Path, marked: bool) -> String {
        let shown = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if marked {
            format!("result ({}, unresolved template)", shown.display())
        } else {
            format!("result ({})", shown.display())
        }
    }

    /// A result path rhei checked is not a template because the root it is
    /// joined to holds a brace: neither the warning nor the flag the retry
    /// prompt reads may mark it.
    // §FS-rhei-agents.3.2.1
    #[test]
    fn a_braced_root_does_not_mark_the_missing_result() {
        let (dir, loaded) = marker_task();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let machine = owed_machine("runtime/triage/{task_id}.issue.md");
        let root = braced_root(dir.path());

        let entry = missing_terminal_result_entry(
            &root,
            &machine,
            task,
            Some("completed"),
            ResultInvocation::whole_task(),
        )
        .expect("the result is missing on a terminal edge");

        assert!(!entry.unresolved_template, "the authored `runtime/results/plan.1.3.md` has no brace");
        assert_eq!(
            entry.warning_entry(),
            result_warning(&root.join("runtime/results/plan.1.3.md"), false),
            "the warning prints the entry's one answer, not a second one asked of its root"
        );
    }

    /// A fan-out fragment's path carries the state name verbatim, so a braced
    /// state name is a brace in the authored path — the same answer a declared
    /// output whose template uses `{state}` gets — and both surfaces print it.
    // §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4 §FS-rhei-states.3.3
    #[test]
    fn a_braced_state_name_marks_the_missing_fragment_on_both_surfaces() {
        let (dir, loaded) = marker_task();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let machine = owed_machine("runtime/triage/{task_id}.issue.md");
        let invocation =
            ResultInvocation { state: "pub{lish}", visit_count: 1, identity: Some("mock-local-m1") };

        let entry = missing_terminal_result_entry(
            dir.path(),
            &machine,
            task,
            Some("completed"),
            invocation,
        )
        .expect("the fragment is missing on a terminal edge");

        assert!(
            entry.unresolved_template,
            "the retry prompt reads this flag, and `{}` holds a brace",
            result_relative_path("plan.1.3", invocation)
        );
        assert_eq!(
            entry.warning_entry(),
            result_warning(
                &dir.path().join(result_relative_path("plan.1.3", invocation)),
                true
            )
        );
    }

    /// A declared output already asks its marker of its authored relative path,
    /// which is also what its warning prints. That stays so under a braced
    /// root, and the result beside it is held to the same rule.
    // §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4
    #[test]
    fn a_declared_output_keeps_its_marker_and_the_result_beside_it_agrees() {
        let (dir, loaded) = marker_task();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.3").expect("task 1.3");
        let root = braced_root(dir.path());
        let invocation = (None, None, None, None, Some("mock"), None);

        for (template, marked) in [
            ("runtime/triage/{task_id}.issue.md", false),
            ("runtime/triage/{not_a_variable}.issue.md", true),
        ] {
            let machine = owed_machine(template);
            let owed = missing_required_outputs_for_invocation(
                &root,
                &machine,
                task,
                "review",
                1,
                Some("completed"),
                invocation,
            );
            let names: Vec<&str> = owed.iter().map(|entry| entry.name.as_str()).collect();
            assert_eq!(names, ["issue", "result"], "template {template}");

            let issue = &owed[0];
            let relative = template.replace("{task_id}", "plan.1.3");
            assert_eq!(issue.unresolved_template, marked, "template {template}");
            assert_eq!(
                issue.warning_entry(),
                if marked {
                    format!("issue ({relative}, unresolved template)")
                } else {
                    format!("issue ({relative})")
                }
            );

            let result = &owed[1];
            assert!(!result.unresolved_template, "template {template}");
            assert_eq!(
                result.warning_entry(),
                result_warning(&root.join("runtime/results/plan.1.3.md"), false),
                "template {template}"
            );
        }
    }
