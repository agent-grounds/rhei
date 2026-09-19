// §FS-rhei-states.4.4: The composed prompt surface carried through the CLI.

    /// Write a `states.yaml` plus its sibling prompt files and load the machine
    /// the way the real commands do.
    fn machine_with_prompt_templates(
        state_body: &str,
        templates: &[(&str, &str)],
    ) -> (tempfile::TempDir, rhei_validator::StateMachine) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let states_path = dir.path().join("states.yaml");
        let yaml = format!(
            r#"
name: prompt-demo
version: 1
states:
  review:
    description: Review
    initial: true
{state_body}
  done:
    description: Done
    final: true
transitions:
  - from: review
    to: done
"#
        );
        fs::write(&states_path, yaml).expect("write states.yaml");
        let templates_dir = dir.path().join("prompt_templates");
        fs::create_dir_all(&templates_dir).expect("create prompt_templates");
        for (name, body) in templates {
            fs::write(templates_dir.join(name), body).expect("write prompt template");
        }
        let machine = rhei_validator::StateMachine::from_yaml_file(&states_path)
            .expect("machine should load with its prompt templates");
        (dir, machine)
    }

    const REVIEW_BODY: &str = r#"    prompt_template:
      name: artifact-review
      values:
        review_role: API reviewer
        findings_path: reports/findings.md
    instructions: Then summarize what you found.
"#;

    #[test]
    fn state_instructions_compose_template_then_inline_text() {
        let (_dir, machine) = machine_with_prompt_templates(
            REVIEW_BODY,
            &[("artifact-review.md", "You are a {review_role}.\nWrite to {findings_path}.\n")],
        );

        assert_eq!(
            state_instructions(&machine, "review"),
            "You are a API reviewer.\nWrite to reports/findings.md.\n\nThen summarize what you found.",
            "`rhei complete` rewrites see the same composed prompt the agent ran with"
        );
        assert_eq!(state_instructions(&machine, "done"), "");
        assert_eq!(state_instructions(&machine, "no-such-state"), "");
    }

    #[test]
    fn state_personality_is_inline_only_and_empty_becomes_none() {
        let (_dir, machine) = machine_with_prompt_templates(
            "    prompt_template: artifact-review\n    personality: Be terse.\n",
            &[("artifact-review.md", "Review it.\n")],
        );

        assert_eq!(state_personality(&machine, "review").as_deref(), Some("Be terse."));
        assert_eq!(state_personality(&machine, "done"), None);
        assert_eq!(state_personality(&machine, "no-such-state"), None);
    }

    #[test]
    fn render_state_machine_text_lists_templates_and_state_selection() {
        let (_dir, machine) = machine_with_prompt_templates(
            REVIEW_BODY,
            &[("artifact-review.md", "Review it.\n"), ("triage.md", "Triage it.\n")],
        );

        let rendered = render_state_machine_text(&machine);

        assert!(rendered.contains("Prompt templates: artifact-review, triage"), "{rendered}");
        assert!(rendered.contains("Prompt template: artifact-review"), "{rendered}");
    }

    #[test]
    fn render_state_machine_json_carries_templates_and_reference() {
        let (_dir, machine) = machine_with_prompt_templates(
            REVIEW_BODY,
            &[("artifact-review.md", "You are a {review_role}.\nWrite to {findings_path}.\n")],
        );

        let rendered = render_state_machine_json(&machine).expect("render JSON");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("parse JSON");

        assert_eq!(
            json["prompt_templates"]["artifact-review"]["instructions"],
            "You are a {review_role}.\nWrite to {findings_path}.\n"
        );
        let review = json["states"]
            .as_array()
            .expect("states array")
            .iter()
            .find(|state| state["name"] == "review")
            .expect("review state");
        assert_eq!(review["prompt_template"]["name"], "artifact-review");
        assert_eq!(review["prompt_template"]["values"]["review_role"], "API reviewer");
    }

    #[test]
    fn viz_machine_exposes_the_composed_instructions() {
        let (_dir, machine) = machine_with_prompt_templates(
            REVIEW_BODY,
            &[("artifact-review.md", "You are a {review_role}.\nWrite to {findings_path}.\n")],
        );

        let flattened = rhei_viz::flatten_machine(&machine);
        let review = flattened
            .states
            .iter()
            .find(|state| state.name == "review")
            .expect("review state");

        assert_eq!(
            review.instructions.as_deref(),
            Some(
                "You are a API reviewer.\nWrite to reports/findings.md.\n\nThen summarize what you found."
            ),
            "the inspector shows the prompt the agent actually receives"
        );
    }

    /// The watch plan stores canonical paths, and so do the events the watcher
    /// delivers — on macOS a temp dir is reached through a `/var` symlink, so
    /// deriving test paths from the raw handle would compare a path the runtime
    /// never sees.
    /// The temp directory in the spelling the watch plan uses, which is the
    /// CLI's canonical form — plain, not Windows' `\\?\` verbatim one.
    // §REQ-cross-platform.5
    fn canonical_tempdir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let root =
            rhei_core::platform::canonical_path(dir.path()).expect("canonicalize tmpdir");
        (dir, root)
    }

    #[test]
    fn watch_plan_covers_an_existing_prompt_templates_directory_recursively() {
        let (_guard, dir) = canonical_tempdir();
        let dir = dir.as_path();
        let plan = dir.join("plan.rhei.md");
        fs::write(&plan, "# Rhei: Watch\n").expect("plan");
        let states = dir.join("states.yaml");
        fs::write(&states, "name: x\nversion: 1\nstates: {}\ntransitions: []\n").expect("states");
        let templates_dir = dir.join("prompt_templates");
        fs::create_dir_all(&templates_dir).expect("prompt_templates");
        fs::write(templates_dir.join("review.md"), "Review it.\n").expect("template");

        let plan_out = validation_watch_plan(&plan, Some(&states));

        assert!(
            plan_out.roots.iter().any(|root| root.mode == RecursiveMode::Recursive
                && paths_equivalent(&root.path, &templates_dir)),
            "an existing prompt_templates/ is watched recursively: {:?}",
            plan_out.roots
        );
        assert!(
            should_revalidate(
                &Event {
                    kind: EventKind::Modify(notify::event::ModifyKind::Data(
                        notify::event::DataChange::Content,
                    )),
                    paths: vec![templates_dir.join("review.md")],
                    attrs: Default::default(),
                },
                &plan_out.targets,
            ),
            "editing a prompt file revalidates"
        );
    }

    #[test]
    fn watch_plan_still_matches_prompt_files_when_the_directory_is_absent() {
        let (_guard, dir) = canonical_tempdir();
        let plan = dir.join("plan.rhei.md");
        fs::write(&plan, "# Rhei: Watch\n").expect("plan");
        let states = dir.join("states.yaml");
        fs::write(&states, "name: x\nversion: 1\nstates: {}\ntransitions: []\n").expect("states");

        let plan_out = validation_watch_plan(&plan, Some(&states));

        // The directory does not exist yet, so there is nothing to watch
        // recursively; creating it must still be seen, because the watch plan
        // is rebuilt after every pass and will then pick the files up.
        assert!(
            should_revalidate(
                &Event {
                    kind: EventKind::Create(notify::event::CreateKind::Folder),
                    paths: vec![dir.join("prompt_templates")],
                    attrs: Default::default(),
                },
                &plan_out.targets,
            ),
            "creating prompt_templates/ revalidates and re-plans"
        );
    }

    /// A sibling output capture is inside a registered directory but is not a
    /// logical watch target and cannot begin a new pass. §FS-rhei-validate.5
    #[test]
    fn validate_watch_rejects_sibling_capture_path() {
        let (_guard, dir) = canonical_tempdir();
        let plan = dir.join("plan.rhei.md");
        let states = dir.join("states.yaml");
        let capture = dir.join("watch-stderr.txt");
        fs::write(&plan, "# Rhei: Watch\n").expect("plan");
        fs::write(&states, "name: watch\nversion: 1\nstates: {}\ntransitions: []\n")
            .expect("states");
        fs::write(&capture, "diagnostic output\n").expect("capture");
        let watched = canonical_watched_paths(&plan, &states);
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![capture],
            attrs: Default::default(),
        };

        assert!(!should_revalidate(&event, &watched));
    }
