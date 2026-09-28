    // How `rhei show` reads its positional, before any plan is loaded. Four
    // ways is the whole surface, and the one that matters is the first: an
    // id-shaped argument is the ticket, because `rhei show probe.7` is the
    // invocation the verb exists for.
    // §FS-rhei-show.5

    /// The four-way resolution, and the two refusals that have to name a form
    /// which works when pasted. §FS-rhei-show.5
    #[test]
    fn show_positional_splits_between_ticket_and_plan() {
        // Id-shaped, no such path: the positional is the ticket.
        let (input, task) =
            split_show_ticket_target(Some(PathBuf::from("probe.7")), None).expect("ticket form");
        assert_eq!(input, None);
        assert_eq!(task, "probe.7");

        // --task present: the positional stays the plan path.
        let (input, task) = split_show_ticket_target(
            Some(PathBuf::from("probe.rhei.md")),
            Some("probe.7".to_string()),
        )
        .expect("plan-and-task form");
        assert_eq!(input, Some(PathBuf::from("probe.rhei.md")));
        assert_eq!(task, "probe.7");

        // An existing path without --task asks for the ticket, both ways round.
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = dir.path().join("probe.rhei.md");
        std::fs::write(&plan, "# Rhei: Probe\n").expect("write");
        let err = split_show_ticket_target(Some(plan), None).expect_err("plan without ticket");
        let said = err.to_string();
        assert!(said.contains("name the ticket"), "got: {said}");
        assert!(said.contains("rhei show <ticket-id>"), "got: {said}");
        assert!(said.contains("--task <ticket-id>"), "got: {said}");

        // Neither positional nor --task: the error shows the positional form.
        let err = split_show_ticket_target(None, None).expect_err("no ticket at all");
        assert!(err.to_string().contains("rhei show <ticket-id>"), "got: {err}");
    }
