    // ---- The spawn preflight an MCP server must pass: §FS-rhei-agents.6.2 ----

    /// A preflight every definition passes, for tests about the gate's other
    /// reasons.
    fn passing_mcp_preflight(_: &McpServerProfile) -> Result<(), String> {
        Ok(())
    }

    /// An executable file named `name` in `dir`, spelled as a `PATH` lookup on
    /// this platform finds it.
    fn mcp_preflight_executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(if cfg!(windows) { format!("{name}.cmd") } else { name.to_string() });
        fs::write(&path, "exit 0\n").expect("write executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        path
    }

    fn mcp_preflight_command(program: &str, path: Option<&Path>) -> McpServerProfile {
        let mut def =
            McpServerProfile { command: Some(vec![program.to_string()]), ..Default::default() };
        if let Some(path) = path {
            def.env.insert("PATH".to_string(), path.display().to_string());
        }
        def
    }

    fn mcp_preflight_url(url: &str) -> McpServerProfile {
        McpServerProfile {
            url: Some(url.to_string()),
            transport: Some("sse".to_string()),
            startup_timeout: Some("1s".to_string()),
            ..Default::default()
        }
    }

    /// A bare name is looked up on the entry's own `env.PATH`, and a miss is
    /// named in the reason.
    #[test]
    fn mcp_preflight_looks_up_a_bare_command_on_the_entrys_path() {
        let bin = tempfile::tempdir().expect("bin");
        let empty = tempfile::tempdir().expect("empty");
        let root = tempfile::tempdir().expect("root");
        mcp_preflight_executable(bin.path(), "fake-mcp");

        let found = mcp_preflight_command("fake-mcp", Some(bin.path()));
        assert_eq!(preflight_mcp_server(&found, root.path(), false), Ok(()));

        let missing = mcp_preflight_command("fake-mcp", Some(empty.path()));
        let reason = preflight_mcp_server(&missing, root.path(), false).expect_err("not on PATH");
        assert_eq!(reason, "command 'fake-mcp' is not on PATH");
    }

    /// A command with a separator is taken from the checkout root, where the
    /// agent starts its servers.
    #[test]
    fn mcp_preflight_resolves_a_relative_command_from_the_checkout_root() {
        let root = tempfile::tempdir().expect("root");
        fs::create_dir(root.path().join("bin")).expect("bin");
        let server = mcp_preflight_executable(&root.path().join("bin"), "server");
        let relative = format!("bin/{}", server.file_name().expect("name").to_string_lossy());

        let def = mcp_preflight_command(&relative, None);
        assert_eq!(preflight_mcp_server(&def, root.path(), false), Ok(()));

        let missing = mcp_preflight_command("bin/absent", None);
        let reason = preflight_mcp_server(&missing, root.path(), false).expect_err("missing");
        assert!(
            reason.starts_with("command 'bin/absent' is not an executable file in"),
            "{reason}"
        );
    }

    /// A file without an execute bit is not a server the agent can start.
    #[cfg(unix)]
    #[test]
    fn mcp_preflight_refuses_a_command_that_is_not_executable() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("server"), "exit 0\n").expect("write");
        let def = mcp_preflight_command("./server", None);
        assert!(preflight_mcp_server(&def, root.path(), false).is_err());
    }

    /// `working_directory` is checked, and resolved against, only for an agent
    /// that can carry it: an `mcp_config_flag` file cannot.
    /// §FS-rhei-mcp-config-file.2
    #[test]
    fn mcp_preflight_checks_working_directory_only_where_the_agent_carries_it() {
        let root = tempfile::tempdir().expect("root");
        let mut def = mcp_preflight_command("./server", None);
        def.working_directory = Some("absent".to_string());
        mcp_preflight_executable(root.path(), "server");
        if cfg!(windows) {
            def.command = Some(vec!["./server.cmd".to_string()]);
        }

        assert_eq!(preflight_mcp_server(&def, root.path(), false), Ok(()));
        let reason = preflight_mcp_server(&def, root.path(), true).expect_err("absent dir");
        assert!(reason.starts_with("working directory '"), "{reason}");

        fs::create_dir(root.path().join("absent")).expect("dir");
        let reason = preflight_mcp_server(&def, root.path(), true).expect_err("no server there");
        assert!(reason.contains("is not an executable file in"), "{reason}");
        mcp_preflight_executable(&root.path().join("absent"), "server");
        assert_eq!(preflight_mcp_server(&def, root.path(), true), Ok(()));
    }

    /// A url server passes when its host accepts a connection, and fails with
    /// the bound it waited when nothing listens.
    #[test]
    fn mcp_preflight_connects_to_a_url_servers_host() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let live = mcp_preflight_url(&format!("http://127.0.0.1:{port}/mcp"));
        assert_eq!(preflight_mcp_server(&live, Path::new("."), false), Ok(()));

        drop(listener);
        let closed = mcp_preflight_url(&format!("http://127.0.0.1:{port}/mcp"));
        let reason = preflight_mcp_server(&closed, Path::new("."), false).expect_err("closed");
        assert_eq!(reason, format!("127.0.0.1:{port} accepted no connection within 1s"));
    }

    #[test]
    fn mcp_url_host_port_defaults_the_port_by_scheme() {
        let host_port = |url: &str| mcp_url_host_port(url);
        assert_eq!(host_port("http://example.com/mcp"), Some(("example.com".into(), 80)));
        assert_eq!(host_port("wss://example.com"), Some(("example.com".into(), 443)));
        assert_eq!(
            host_port("HTTPS://user:pw@example.com:8443/x?y#z"),
            Some(("example.com".into(), 8443))
        );
        assert_eq!(host_port("http://[::1]:9000/sse"), Some(("::1".into(), 9000)));
        assert_eq!(host_port("http://[::1]/sse"), Some(("::1".into(), 80)));
        assert_eq!(host_port("tcp://example.com"), None);
        assert_eq!(host_port("http:///path"), None);
        assert_eq!(host_port("example.com:80"), None);
    }

    /// A required server that fails the preflight blocks the spawn with the
    /// preflight's reason; an optional one is dropped with it as a warning.
    /// §FS-rhei-agents.6.1
    #[test]
    fn gate_holds_mcp_definitions_to_the_preflight() {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile: built_in_agents().remove("claude-code").expect("claude-code"),
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: None,
            autonomous_args: Vec::new(),
        };
        let entry = |id: &str, optional: bool| ResolvedMcpEntry {
            id: id.to_string(),
            optional,
            definition: Some(mcp_preflight_command(id, None)),
        };
        let tooling = ResolvedTooling {
            mcp_servers: vec![entry("needed", false), entry("nice", true), entry("fine", false)],
            ..Default::default()
        };
        let preflight = |def: &McpServerProfile| match def.command.as_deref() {
            Some([program]) if program == "fine" => Ok(()),
            _ => Err("command is not on PATH".to_string()),
        };

        let gate = gate_tooling_for_agent(&resolved, &tooling, &preflight);
        let required: Vec<_> =
            gate.required.iter().map(|r| (r.id.as_str(), r.reason.as_str())).collect();
        assert_eq!(required, [("needed", "command is not on PATH")]);
        assert_eq!(
            gate.warnings,
            ["warning: optional mcp 'nice' unavailable (command is not on PATH); dropping"]
        );
        assert_eq!(gate.tooling.mcp_servers_csv(), "fine");
    }

    /// `startup_timeout` bounds the preflight's connection, so it must be a
    /// duration greater than zero, on a registry entry or an inline one.
    /// §FS-rhei-agents.1.1.4
    #[test]
    fn startup_timeout_must_be_a_positive_duration() {
        let settings: RheiSettings = serde_json::from_value(serde_json::json!({
            "mcp_servers": {
                "zero": { "url": "http://127.0.0.1:1", "transport": "sse", "startup_timeout": "0s" },
                "fine": { "command": ["fine"], "startup_timeout": "30s" }
            }
        }))
        .expect("settings");
        let errors = validate_intrinsic_settings(&settings);
        let timeouts: Vec<_> = errors.iter().filter(|e| e.contains("startup_timeout")).collect();
        assert_eq!(
            timeouts,
            ["mcp_servers.'zero' has startup_timeout '0s', which is not a duration greater than zero such as '10s'"]
        );

        let inline: Vec<StateMcpEntry> = serde_json::from_value(serde_json::json!([
            { "id": "words", "command": ["x"], "startup_timeout": "ten seconds" }
        ]))
        .expect("entries");
        let mut errors = Vec::new();
        validate_mcp_entries_known(
            "state 'work' mcp_servers",
            Some(&inline),
            &BTreeMap::new(),
            &mut errors,
        );
        assert_eq!(
            errors,
            ["state 'work' mcp_servers entry 'words' has startup_timeout 'ten seconds', which is not a duration greater than zero such as '10s'"]
        );
    }

    /// A skill passes when its path can be read; a missing one does not.
    #[test]
    fn skill_preflight_reads_the_path() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("SKILL.md");
        fs::write(&file, "# skill\n").expect("write");
        assert!(skill_path_readable(dir.path()));
        assert!(skill_path_readable(&file));
        assert!(!skill_path_readable(&dir.path().join("absent")));
    }
