// Launch options pin the exclusive Claude Code MCP selection.
// §FS-rhei-agents.1.1.2 §FS-rhei-states.7.3 §FS-rhei-agents.2.2

mod mcp_surface {
    use super::super::*;
    use serde_json::json;

    fn profile(id: &str, entry: serde_json::Value) -> CustomAgentProfile {
        let mut agents = built_in_agents();
        let mut fields = agents
            .iter()
            .map(|(id, p)| (id.clone(), built_in_agent_fields(p)))
            .collect::<BTreeMap<_, _>>();
        agents.insert(id.to_string(), serde_json::from_value(entry.clone()).expect("profile"));
        fields.insert(id.to_string(), raw_object_keys(&entry));
        apply_agent_family_inheritance(&mut agents, &mut fields);
        agents.remove(id).expect("resolved profile")
    }

    fn resolved(id: &str, profile: CustomAgentProfile, mode: Option<&str>) -> ResolvedAgent {
        ResolvedAgent {
            agent: AgentConfig::from(id),
            profile,
            mode: mode.map(str::to_string),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(5),
            autonomous_args: Vec::new(),
        }
    }

    fn command(
        agent: &ResolvedAgent,
        tooling: &ResolvedTooling,
        root: &Path,
        snapshot: &[String],
    ) -> Vec<String> {
        build_agent_command(
            agent,
            "fixture prompt",
            root,
            None,
            None,
            "task-1",
            "work",
            1,
            1,
            tooling,
            root,
            snapshot,
        )
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
    }

    fn strict(argv: &[String]) -> usize {
        argv.iter().position(|arg| arg == "--strict-mcp-config").unwrap_or_else(|| {
            panic!("missing --strict-mcp-config in Claude Code option region: {argv:?}")
        })
    }

    fn declared(optional: bool) -> ResolvedTooling {
        ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "declared".into(),
                optional,
                definition: Some(McpServerProfile {
                    url: Some("https://fixture.invalid".into()),
                    ..Default::default()
                }),
            }],
            skills: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn mcp_surface_strict_follows_resolved_family_across_modes_and_transports() {
        let cases = [
            ("cld", json!({"family":"claude-code", "command":["wrapper"]}), true),
            (
                "cld",
                json!({"family":"claude-code", "command":["wrapper"], "stdin_prompt":false}),
                true,
            ),
            (
                "cld",
                json!({"family":"claude-code", "command":["wrapper"], "intervene_stdin":true}),
                true,
            ),
            ("claude-code", json!({"family":"codex", "command":["wrapper"]}), false),
            ("other", json!({"command":["wrapper"], "stdin_prompt":true}), false),
        ];
        let dir = tempfile::tempdir().expect("fixture root");
        let mut missing = Vec::new();
        for (id, entry, expects_strict) in cases {
            for mode in [None, Some("yolo")] {
                let agent = resolved(id, profile(id, entry.clone()), mode);
                let argv = command(&agent, &ResolvedTooling::default(), dir.path(), &[]);
                let has_strict = argv.iter().any(|arg| arg == "--strict-mcp-config");
                if has_strict != expects_strict {
                    missing.push(format!("id={id} family={} mode={mode:?} stdin={} intervention={}: missing --strict-mcp-config; argv={argv:?}", agent.family(), agent.profile.stdin_prompt, agent.profile.intervene_stdin));
                }
                if expects_strict && has_strict && agent.profile.stdin_prompt {
                    assert!(strict(&argv) < argv.len() - 1);
                    assert_eq!(argv.last().map(String::as_str), Some("--"));
                }
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    #[test]
    fn mcp_surface_wholesale_same_id_replacement_gets_strict_without_transport_inheritance() {
        let agent = resolved(
            "claude-code",
            profile("claude-code", json!({"command":["wrapper"], "stdin_prompt":true})),
            None,
        );
        assert_eq!(agent.family(), "claude-code");
        assert!(agent.profile.mcp_config_flag.is_none(), "wholesale replacement stays wholesale");
        let dir = tempfile::tempdir().expect("root");
        let argv = command(&agent, &ResolvedTooling::default(), dir.path(), &[]);
        assert!(!dir.path().join("tmp").exists());
        assert_eq!(argv.last().map(String::as_str), Some("--"));
        assert!(strict(&argv) < argv.len() - 1);
    }

    #[test]
    fn mcp_surface_explicit_attachment_overrides_cannot_disable_strict() {
        let dir = tempfile::tempdir().expect("root");
        let mut missing = Vec::new();
        for override_fields in [
            json!({"mcp_flag":"--attach", "mcp_config_flag":null}),
            json!({"mcp_config_flag":"--config"}),
            json!({"mcp_flag":null, "mcp_config_flag":null}),
        ] {
            let mut entry = json!({"family":"claude-code", "command":["wrapper"]});
            entry
                .as_object_mut()
                .expect("entry")
                .extend(override_fields.as_object().expect("overrides").clone());
            let agent = resolved("cld", profile("cld", entry), None);
            let gate = gate_tooling_for_agent(&agent, &declared(true));
            assert!(gate.required.is_empty());
            let argv = command(&agent, &gate.tooling, dir.path(), &[]);
            if agent.profile.mcp_flag.is_some() {
                assert!(argv.windows(2).any(|pair| pair == ["--attach", "declared"]));
                assert!(!argv.iter().any(|arg| arg == "--mcp-config"));
            } else if agent.profile.mcp_config_flag.is_some() {
                assert!(argv.iter().any(|arg| arg == "--config"));
            } else {
                assert_eq!(gate.warnings.len(), 1);
                assert!(gate.tooling.mcp_servers[0].definition.is_none());
                assert_eq!(gate_tooling_for_agent(&agent, &declared(false)).required.len(), 1);
            }
            if !argv.iter().any(|arg| arg == "--strict-mcp-config") {
                missing.push(format!(
                    "explicit {override_fields}: missing --strict-mcp-config; argv={argv:?}"
                ));
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    #[test]
    fn mcp_surface_optional_drop_to_empty_retains_strict_and_no_config() {
        let agent = resolved(
            "claude-code",
            built_in_agents().remove("claude-code").expect("builtin"),
            None,
        );
        let mut tooling = declared(true);
        tooling.mcp_servers[0].definition = None;
        let gate = gate_tooling_for_agent(&agent, &tooling);
        assert!(gate.required.is_empty());
        assert_eq!(gate.warnings.len(), 1);
        assert_eq!(gate.tooling.mcp_servers_csv(), "");
        let dir = tempfile::tempdir().expect("root");
        let argv = command(&agent, &gate.tooling, dir.path(), &[]);
        assert!(!argv.iter().any(|arg| arg == "--mcp-config"));
        assert!(!dir.path().join("tmp").exists());
        assert!(strict(&argv) < argv.len() - 1);
    }

    #[test]
    fn mcp_surface_snapshot_strict_config_skill_separator_order() {
        let agent = resolved(
            "claude-code",
            built_in_agents().remove("claude-code").expect("builtin"),
            Some("yolo"),
        );
        let mut tooling = declared(false);
        tooling.skills.push(ResolvedSkillEntry {
            id: "review".into(),
            optional: false,
            definition: Some(SkillProfile { path: "fixture-skill".into(), description: None }),
        });
        let dir = tempfile::tempdir().expect("root");
        let argv = command(&agent, &tooling, dir.path(), &["--resume".into(), "snapshot".into()]);
        let config = argv.iter().position(|arg| arg == "--mcp-config").expect("config attached");
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&argv[config + 1]).expect("config file"))
                .expect("JSON");
        assert_eq!(
            json["mcpServers"].as_object().expect("servers").keys().collect::<Vec<_>>(),
            vec!["declared"]
        );
        let skill = argv.iter().position(|arg| arg == "--skill").expect("skill attached");
        assert_eq!(argv.last().map(String::as_str), Some("--"));
        let strict = strict(&argv);
        let snapshot = argv.iter().position(|arg| arg == "snapshot").expect("snapshot argument");
        assert!(
            snapshot < strict && strict < config && config < skill && skill < argv.len() - 1,
            "tooling order: {argv:?}"
        );
    }

    #[test]
    fn mcp_surface_config_write_warning_retains_strict_without_native_fallback() {
        let agent = resolved(
            "claude-code",
            built_in_agents().remove("claude-code").expect("builtin"),
            None,
        );
        let dir = tempfile::tempdir().expect("root");
        fs::write(dir.path().join("tmp"), "blocks config directory").expect("block config write");
        let argv = command(&agent, &declared(false), dir.path(), &[]);
        assert!(
            !argv.iter().any(|arg| arg == "--mcp-config"),
            "existing warning path omits failed attachment"
        );
        assert!(strict(&argv) < argv.len() - 1);
    }

    #[test]
    fn mcp_surface_other_families_keep_existing_arguments() {
        let dir = tempfile::tempdir().expect("root");
        for family in ["codex", "gemini", "cursor", "kilocode", "pi"] {
            let agent = resolved(
                "wrapper",
                profile("wrapper", json!({"family":family, "command":["recorder"]})),
                None,
            );
            let argv = command(&agent, &ResolvedTooling::default(), dir.path(), &[]);
            assert!(!argv.iter().any(|arg| arg == "--strict-mcp-config"), "{family}: {argv:?}");
            let expected = match family {
                "codex" => vec!["--json", "--"],
                "gemini" => vec!["--prompt", "fixture prompt"],
                "cursor" => vec!["--print", "fixture prompt"],
                "kilocode" => vec!["--auto", "fixture prompt"],
                "pi" => vec!["--mode", "json", "-p", "fixture prompt"],
                _ => unreachable!(),
            };
            assert_eq!(argv, expected, "{family} keeps its existing argv");
        }
    }
}
