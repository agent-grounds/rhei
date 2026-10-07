    // ---- A task's own tooling in the effective set: §FS-rhei-task-tooling.3 ----
    // §FS-rhei-task-tooling.2 §FS-rhei-task-tooling.4

    fn task_entries(entries: &[(&str, bool)]) -> Vec<TaskToolingEntry> {
        entries
            .iter()
            .map(|(id, optional)| TaskToolingEntry { id: id.to_string(), optional: *optional })
            .collect()
    }

    fn task_mcp(entries: &[(&str, bool)]) -> TaskTooling {
        TaskTooling { mcp_servers: task_entries(entries), skills: Vec::new() }
    }

    /// Settings whose registry holds every id in `mcp` with a command naming
    /// it, and whose defaults are `defaults`.
    fn tooling_settings(defaults: Vec<StateMcpEntry>, mcp: &[&str]) -> RheiSettings {
        let registry = mcp
            .iter()
            .map(|id| {
                let command = Some(vec![format!("registry-{id}")]);
                (id.to_string(), McpServerProfile { command, ..Default::default() })
            })
            .collect();
        settings_with(Some(defaults), registry)
    }

    fn ids<E: TaskAddedEntry>(entries: &[E]) -> Vec<&str> {
        entries.iter().map(|entry| entry.id()).collect()
    }

    fn mcp_command(tooling: &ResolvedTooling, id: &str) -> String {
        let entry = tooling.mcp_servers.iter().find(|entry| entry.id == id).expect("entry");
        let definition = entry.definition.as_ref().expect("definition");
        definition.command.as_deref().expect("command")[0].clone()
    }

    fn mcp_optional(tooling: &ResolvedTooling, id: &str) -> bool {
        tooling.mcp_servers.iter().find(|entry| entry.id == id).expect("entry").optional
    }

    const BARE_PENDING: &str = "  pending:\n    description: Work\n    agent: claude-code\n";

    #[test]
    fn a_task_adds_its_id_and_a_sibling_in_the_state_gets_none_of_it() {
        let machine = machine_with_tooling(BARE_PENDING);
        let settings = tooling_settings(Vec::new(), &["thunderbird-mail"]);

        let named = resolve_tooling(
            &machine,
            "pending",
            &task_mcp(&[("thunderbird-mail", false)]),
            &settings,
        );
        let sibling = resolve_tooling(&machine, "pending", &TaskTooling::default(), &settings);

        assert_eq!(ids(&named.mcp_servers), vec!["thunderbird-mail"]);
        assert_eq!(mcp_command(&named, "thunderbird-mail"), "registry-thunderbird-mail");
        assert!(sibling.mcp_servers.is_empty(), "{:?}", sibling.mcp_servers);
    }

    /// An id the state or the defaults already hold keeps their definition.
    #[test]
    fn a_shared_id_keeps_the_state_or_defaults_definition() {
        let machine = machine_with_tooling(
            "  pending:\n    description: Work\n    agent: claude-code\n    mcp_servers:\n\
             \x20     - id: grafana\n        command: [\"state-grafana\"]\n",
        );
        let inline_postgres = StateMcpEntry::Object(StateMcpEntryObject {
            id: "postgres".to_string(),
            command: Some(vec!["defaults-postgres".to_string()]),
            ..Default::default()
        });
        let settings = tooling_settings(vec![inline_postgres], &["grafana", "postgres"]);
        let task = task_mcp(&[("grafana", false), ("postgres", false)]);

        let tooling = resolve_tooling(&machine, "pending", &task, &settings);

        assert_eq!(ids(&tooling.mcp_servers), vec!["postgres", "grafana"]);
        assert_eq!(mcp_command(&tooling, "grafana"), "state-grafana");
        assert_eq!(mcp_command(&tooling, "postgres"), "defaults-postgres");
    }

    /// Required wins whichever side marks an entry required.
    #[test]
    fn a_shared_id_is_optional_only_when_both_sides_say_so() {
        let optional_state = machine_with_tooling(
            "  pending:\n    description: Work\n    agent: claude-code\n    mcp_servers:\n\
             \x20     - id: grafana\n        optional: true\n",
        );
        let required_state = machine_with_tooling(
            "  pending:\n    description: Work\n    agent: claude-code\n    mcp_servers: [grafana]\n",
        );
        let settings = tooling_settings(Vec::new(), &["grafana"]);
        let required_task = task_mcp(&[("grafana", false)]);
        let optional_task = task_mcp(&[("grafana", true)]);

        let tooling = resolve_tooling(&optional_state, "pending", &required_task, &settings);
        assert!(!mcp_optional(&tooling, "grafana"), "state optional + task required");
        let tooling = resolve_tooling(&required_state, "pending", &optional_task, &settings);
        assert!(!mcp_optional(&tooling, "grafana"), "state required + task optional");
        let tooling = resolve_tooling(&optional_state, "pending", &optional_task, &settings);
        assert!(mcp_optional(&tooling, "grafana"), "both optional");
    }

    #[test]
    fn a_state_empty_list_clears_the_defaults_but_not_the_task_entries() {
        let machine = machine_with_tooling(
            "  pending:\n    description: Work\n    agent: claude-code\n    mcp_servers: []\n",
        );
        let settings = tooling_settings(
            vec![StateMcpEntry::Id("postgres".to_string())],
            &["postgres", "grafana"],
        );

        let tooling =
            resolve_tooling(&machine, "pending", &task_mcp(&[("grafana", false)]), &settings);

        assert_eq!(ids(&tooling.mcp_servers), vec!["grafana"]);
    }

    const WITHHOLDING_PENDING: &str = "  pending:\n    description: Work\n    agent: claude-code\n\
        \x20   mcp_servers:\n      - id: postgres\n        optional: true\n\
        \x20   skills: [changelog-style]\n    withhold_task_tooling: true\n";

    /// Withholding drops the task's entries of both kinds, leaves what the
    /// state and the defaults supply exactly as without the task, and reports
    /// only ids the invocation did not get.
    #[test]
    fn withholding_drops_the_task_servers_and_reports_only_unsupplied_ids() {
        let machine = machine_with_tooling(WITHHOLDING_PENDING);
        let settings = tooling_settings(
            vec![StateMcpEntry::Id("linear".to_string())],
            &["linear", "postgres", "thunderbird-mail"],
        );
        let task = TaskTooling {
            mcp_servers: task_entries(&[("thunderbird-mail", false), ("postgres", false)]),
            skills: task_entries(&[("release-notes", false)]),
        };

        let withheld = resolve_tooling(&machine, "pending", &task, &settings);
        let without_task =
            resolve_tooling(&machine, "pending", &TaskTooling::default(), &settings);

        assert_eq!(ids(&withheld.mcp_servers), ids(&without_task.mcp_servers));
        assert_eq!(ids(&withheld.mcp_servers), vec!["linear", "postgres"]);
        assert!(mcp_optional(&withheld, "postgres"), "the task cannot make the state's required");
        assert_eq!(withheld.mcp_withheld, vec!["thunderbird-mail"]);
        assert_eq!(ids(&withheld.skills), vec!["changelog-style"]);
        assert_eq!(withheld.skills_withheld, vec!["release-notes"]);
    }

    #[test]
    fn withholding_drops_the_task_skills_and_reports_only_unsupplied_ids() {
        let machine = machine_with_tooling(WITHHOLDING_PENDING);
        let settings = tooling_settings(Vec::new(), &["postgres"]);
        let task = TaskTooling {
            mcp_servers: Vec::new(),
            skills: task_entries(&[("release-notes", true), ("changelog-style", false)]),
        };

        let withheld = resolve_tooling(&machine, "pending", &task, &settings);

        assert_eq!(ids(&withheld.skills), vec!["changelog-style"]);
        assert_eq!(withheld.skills_withheld, vec!["release-notes"]);
        assert!(withheld.mcp_withheld.is_empty(), "{:?}", withheld.mcp_withheld);
        assert_eq!(ids(&withheld.mcp_servers), vec!["postgres"]);
    }
