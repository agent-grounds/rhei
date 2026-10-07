    // ---- Where a task's own tooling shows and is checked: §FS-rhei-task-tooling ----

    /// A `rhei new` invocation's options, parsed the way the binary parses them.
    fn new_options(args: &[&str]) -> NewOptions {
        let argv = ["rhei", "new", "Mail", "--under", "auth"].iter().chain(args).copied();
        match Cli::try_parse_from(argv).expect("cli should parse").command {
            Commands::New { options } => options,
            other => panic!("expected new command, got {other:?}"),
        }
    }

    /// An authored `false` is shown as authored in JSON and prints no line.
    // §FS-rhei-states-cmd.4 §FS-rhei-states-cmd.5
    #[test]
    fn states_show_an_authored_false_withhold_as_authored_and_print_no_line() {
        let machine = machine_with_tooling(
            "  pending:\n    description: Work\n    agent: claude-code\n    withhold_task_tooling: false\n",
        );
        let json: serde_json::Value =
            serde_json::from_str(&render_state_machine_json(&machine).expect("json"))
                .expect("states JSON");
        let state = |name: &str| {
            json["states"]
                .as_array()
                .expect("states array")
                .iter()
                .find(|state| state["name"] == name)
                .cloned()
                .expect("state")
        };
        assert_eq!(state("pending")["withhold_task_tooling"], serde_json::json!(false));
        assert!(state("completed").get("withhold_task_tooling").is_none());
        assert!(!render_state_machine_text(&machine).contains("Task tooling"));
    }

    fn tooling_agent(command: Vec<String>) -> ResolvedAgent {
        ResolvedAgent {
            agent: AgentConfig::from("fake"),
            profile: CustomAgentProfile {
                command,
                prompt_flag: Some("-p".to_string()),
                stdin_prompt: false,
                ..CustomAgentProfile::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(10),
            autonomous_args: Vec::new(),
        }
    }

    /// The agent log header the spawn writes for `tooling`.
    fn spawned_log_header(tooling: &ResolvedTooling) -> Vec<String> {
        let dir = tempfile::tempdir().expect("tmpdir");
        let log_path = dir.path().join("agent.log");
        spawn_and_wait_agent(
            &tooling_agent(write_fake_agent(dir.path())),
            &builtin_price_book(),
            "hello",
            dir.path(),
            dir.path(),
            None,
            dir.path(),
            None,
            "task-tooling",
            "review",
            1,
            tooling,
            &log_path,
            dir.path(),
            None,
            0,
            Arc::new(RecordingSink::default()),
            None,
            &spawn_plan_for_test(&log_path),
            None,
        )
        .expect("fake agent runs");
        let log = fs::read_to_string(&log_path).expect("read log");
        let header = log.split_once("\n===\n").map_or(log.as_str(), |(header, _)| header);
        header.lines().map(str::to_string).collect()
    }

    fn resolved_mcp(id: &str) -> ResolvedMcpEntry {
        let definition = Some(McpServerProfile::default());
        ResolvedMcpEntry { id: id.to_string(), optional: false, definition }
    }

    /// Withheld ids follow their kind's line in the same comma format, and the
    /// log stays at format `v1`. §FS-rhei-task-tooling.7
    #[test]
    fn the_log_header_lists_withheld_ids_after_their_kind() {
        let tooling = ResolvedTooling {
            mcp_servers: vec![resolved_mcp("postgres")],
            skills: Vec::new(),
            mcp_withheld: vec!["thunderbird-mail".to_string(), "grafana".to_string()],
            skills_withheld: vec!["release-notes".to_string()],
        };

        let header = spawned_log_header(&tooling);

        assert_eq!(header[0], "=== rhei agent log v1 ===");
        let tooling_lines: Vec<&str> = header
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("mcp_servers") || line.starts_with("skills"))
            .collect();
        assert_eq!(
            tooling_lines,
            vec![
                "mcp_servers: postgres",
                "mcp_servers_withheld: thunderbird-mail,grafana",
                "skills_withheld: release-notes",
            ]
        );
    }

    #[test]
    fn the_log_header_has_no_withheld_line_when_nothing_is_withheld() {
        let tooling = ResolvedTooling {
            mcp_servers: vec![resolved_mcp("postgres")],
            ..ResolvedTooling::default()
        };

        let header = spawned_log_header(&tooling);

        assert_eq!(header[0], "=== rhei agent log v1 ===");
        assert!(header.iter().any(|line| line == "mcp_servers: postgres"), "{header:?}");
        assert!(!header.iter().any(|line| line.contains("_withheld")), "{header:?}");
    }

    /// A required skill the task adds, whose bundle is missing, is unavailable
    /// and routed like a state's own. §FS-rhei-task-tooling.5
    #[test]
    fn a_required_task_skill_that_fails_is_unavailable_and_routed() {
        let machine = machine_with_tooling(BARE_PENDING);
        let mut settings = settings_with(None, BTreeMap::new());
        let missing = SkillProfile { path: "./no-such-skill-bundle".to_string(), description: None };
        settings.skills.insert("release-notes".to_string(), missing);
        let task = TaskTooling { mcp_servers: Vec::new(), skills: task_entries(&[("release-notes", false)]) };
        let mut agent = tooling_agent(vec!["fake-agent".to_string()]);
        agent.profile.skill_flag = Some("--skill".to_string());

        let tooling = resolve_tooling(&machine, "pending", &task, &settings);
        let gate = gate_tooling_for_agent(&agent, &tooling);
        let unavailable = unavailable_ids(&gate.required, ToolingKind::Skill);

        assert_eq!(unavailable, vec!["release-notes"]);
        let naming = serde_yaml::from_str::<serde_yaml::Value>("[release-notes]").unwrap();
        let other = serde_yaml::from_str::<serde_yaml::Value>("[changelog-style]").unwrap();
        assert!(tooling_trigger_matches(&serde_yaml::Value::Bool(true), &unavailable));
        assert!(tooling_trigger_matches(&naming, &unavailable));
        assert!(!tooling_trigger_matches(&other, &unavailable));
    }

    fn plan_with_task_tooling(metadata: &str) -> rhei_core::ast::Rhei {
        let input = format!("# Rhei: Tooling\n## Tasks\n\n### Task 1: Mail\n**State:** pending\n{metadata}");
        let mut rhei = rhei_core::parse(&input).expect("parse");
        rhei.tasks[0].id = parse_task_id("plan.1");
        rhei
    }

    /// The task and its field are the label of the existing unknown-id error.
    // §FS-rhei-task-tooling.6
    #[test]
    fn a_task_unknown_skill_is_named_with_its_task_and_field() {
        let rhei = plan_with_task_tooling("**Skills:** release-notse\n");
        let settings = settings_with(None, BTreeMap::new());

        let errors = validate_task_tooling_settings_references(&rhei, &settings);

        assert_eq!(
            errors,
            vec!["Task plan.1 **Skills:** references unknown skill 'release-notse'".to_string()]
        );
    }

    /// Validation reads the task, not the states it passes through, so a
    /// withholding state does not excuse an unknown id. §FS-rhei-task-tooling.6
    #[test]
    fn an_unknown_task_id_in_a_withholding_state_is_still_an_error() {
        let machine = machine_with_tooling(WITHHOLDING_PENDING);
        let rhei = plan_with_task_tooling("**MCP servers:** thunderbird-mial\n");
        let settings = tooling_settings(Vec::new(), &["postgres"]);

        let errors = validate_plan_settings_references(
            &rhei,
            &rhei_validator::MachineSet::single(machine),
            &settings,
        );

        let expected = "Task plan.1 **MCP servers:** references unknown mcp server 'thunderbird-mial'";
        assert!(errors.iter().any(|error| error == expected), "{errors:?}");
    }

    /// A description line opening with either field would author it.
    // §FS-rhei-new.3.4
    #[test]
    fn description_markers_include_the_task_tooling_fields() {
        assert!(PLAN_METADATA_MARKERS.contains(&"**MCP servers:**"));
        assert!(PLAN_METADATA_MARKERS.contains(&"**Skills:**"));
        assert!(structural_description_line("**MCP servers:** thunderbird-mail").is_some());
        assert!(structural_description_line("**Skills:** release-notes").is_some());
    }

    /// `rhei new` reads each flag value with the plan's own entry reader.
    // §FS-rhei-task-tooling.8
    #[test]
    fn new_reads_a_task_tooling_flag_with_the_plan_entry_reader() {
        let options = new_options(&["--mcp-server", "grafana (optional)", "--skill", "release-notes"]);
        let mcp = read_task_tooling_flag("--mcp-server", MCP_SERVERS_FIELD, &options.mcp_servers)
            .expect("entry");
        let skills = read_task_tooling_flag("--skill", SKILLS_FIELD, &options.skills).expect("entry");

        assert_eq!(mcp, task_entries(&[("grafana", true)]));
        assert_eq!(skills, task_entries(&[("release-notes", false)]));
    }

    /// The malformed value is refused before settings are read or a file is
    /// written: the target does not even exist. §FS-rhei-task-tooling.8
    #[test]
    fn new_refuses_a_malformed_task_tooling_value_as_an_argument() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let target = dir.path().join("absent");
        let options = new_options(&["--mcp-server", "grafana (optinal)"]);

        let refused = reject_task_tooling_flags(&options, &target).expect_err("malformed entry");

        let message = refused.to_string();
        assert!(message.contains("--mcp-server") && message.contains("grafana (optinal)"), "{message}");
        assert!(!target.exists(), "nothing may be written for a refused value");
    }

    const TOOLED_TASK: &str = "# Rhei: Tooling\n\n## Tasks\n\n### Task 1: Mail\n**State:** pending\n\
        **Model:** deep\n**MCP servers:** thunderbird-mail, grafana (optional)\n\
        **Skills:** release-notes\n\nBody\n";

    /// A claim lands before the execution override and the tooling.
    // §FS-rhei-plan-language.2
    #[test]
    fn a_claim_lands_before_model_and_the_tooling_lines() {
        let claimed = insert_task_assignee(TOOLED_TASK, "1", "codex").expect("claim");

        assert!(
            claimed.contains(
                "**State:** pending\n**Assignee:** codex\n**Model:** deep\n\
                 **MCP servers:** thunderbird-mail, grafana (optional)\n**Skills:** release-notes\n"
            ),
            "{claimed}"
        );
        rhei_core::parse(&claimed).expect("the claimed plan still parses");
    }

    /// `rhei complete` drops the claim and leaves the tooling as authored.
    // §FS-rhei-task-tooling.1
    #[test]
    fn completing_drops_the_claim_and_leaves_the_tooling_lines() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let claimed = insert_task_assignee(TOOLED_TASK, "1", "codex").expect("claim");
        fs::write(&plan, &claimed).expect("write plan");

        rewrite_task_completion(&plan, "1", "result", "result.md", false).expect("complete");

        assert_eq!(fs::read_to_string(&plan).expect("read plan"), TOOLED_TASK);
    }
