// How a prompt reaches the agent, asserted on the argument vector rhei composes
// rather than on a description of it. agent-grounds/rhei#179: a supervisor brief
// past the platform's per-argument limit aborted the spawn before Claude Code
// started, because the built-in profile carried the whole prompt in one `argv`
// entry.

// These are the only assertions that tie the contract to the *built-in*
// profile. Do not weaken them into claims about a hand-written profile: that
// is a different agent, and the ticket is about this one.

// Reaching the built-in end to end would need a stub `claude` on `PATH`, which
// no portable fixture can put there. §REQ-cross-platform.4

mod agent_prompt_transport_tests {
    use super::super::*;

    /// Stands in for a composed prompt. Size is not what these assert — the
    /// transport is — so the shortest string that would be unmistakable in an
    /// argument vector does.
    const PROMPT: &str = "compose the pull request description";

    /// Everything rhei put on the command line, in order.
    fn composed(
        resolved: &ResolvedAgent,
        tooling: &ResolvedTooling,
        snapshot_args: &[String],
    ) -> Vec<String> {
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            resolved,
            PROMPT,
            Path::new("/tmp/workspace"),
            None,
            None,
            "task-1",
            "pending",
            1,
            1,
            tooling,
            runtime_dir.path(),
            snapshot_args,
        );
        command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    fn position(argv: &[String], needle: &str) -> usize {
        argv.iter()
            .position(|arg| arg == needle)
            .unwrap_or_else(|| panic!("expected {needle} in the composed command; got: {argv:?}"))
    }

    fn resolved(profile: CustomAgentProfile, id: &str, mode: Option<&str>) -> ResolvedAgent {
        ResolvedAgent {
            agent: AgentConfig::from(id),
            profile,
            mode: mode.map(str::to_string),
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        }
    }

    fn builtin(id: &str) -> CustomAgentProfile {
        built_in_agents().remove(id).unwrap_or_else(|| panic!("built-in {id} profile"))
    }

    /// The prompt is nowhere on the command line, and `-p` is still emitted —
    /// bare, because it is what puts Claude Code in print mode rather than what
    /// carries the prompt. §FS-rhei-agents.2 §FS-rhei-agents.1.1.2
    #[test]
    fn builtin_claude_code_carries_no_prompt_in_argv() {
        let argv = composed(
            &resolved(builtin("claude-code"), "claude-code", Some("yolo")),
            &ResolvedTooling::default(),
            &[],
        );

        assert!(
            !argv.iter().any(|arg| arg.contains(PROMPT)),
            "the prompt must travel on stdin, not in argv: {argv:?}"
        );
        assert!(
            argv.windows(2).any(|pair| pair == ["-p", "--model"]),
            "the print flag is emitted with no value, ahead of the model flag: {argv:?}"
        );
        assert_eq!(
            argv.last().map(String::as_str),
            Some("--"),
            "a stdin_prompt profile ends at the separator: {argv:?}"
        );
        assert_eq!(
            argv.windows(3)
                .filter(|span| *span == ["--output-format", "stream-json", "--verbose"])
                .count(),
            1,
            "the stream-json event stream is still requested exactly once: {argv:?}"
        );
    }

    /// Everything rhei has to say to the agent arrives before the separator.
    /// Past a `--` a flag is prompt text: Claude Code neither applies it nor
    /// complains, so a state that declared an MCP server would simply run
    /// without it. §FS-rhei-agents.2.2 §FS-rhei-snapshots.10.1
    #[test]
    fn builtin_claude_code_keeps_snapshot_and_tooling_flags_before_the_separator() {
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "linear".to_string(),
                optional: false,
                definition: Some(McpServerProfile {
                    command: Some(vec!["mcp-linear".to_string()]),
                    ..Default::default()
                }),
            }],
            skills: vec![ResolvedSkillEntry {
                id: "review".to_string(),
                optional: false,
                definition: Some(SkillProfile {
                    path: "/skills/review".to_string(),
                    description: None,
                }),
            }],
            ..Default::default()
        };
        let snapshot_args = vec!["--session-dir".to_string(), "sessions".to_string()];
        let claude = resolved(builtin("claude-code"), "claude-code", None);
        let argv = composed(&claude, &tooling, &snapshot_args);

        let separator = position(&argv, "--");
        let session_dir = position(&argv, "--session-dir");
        let mcp = position(&argv, "--mcp-config");
        let skill = position(&argv, "--skill");

        assert!(
            session_dir < mcp && session_dir < skill,
            "the snapshot strategy flags keep their slot ahead of the tooling flags: {argv:?}"
        );
        assert!(
            mcp < separator && skill < separator,
            "--mcp-config and --skill must precede the separator, or they are prompt text: {argv:?}"
        );
        assert_eq!(
            separator,
            argv.len() - 1,
            "the separator is the last argument rhei composes: {argv:?}"
        );
    }

    /// The rule is generic, not a special case for one built-in: a profile that
    /// declares both fields gets its flag emitted with no value rather than
    /// dropped without a word. §FS-rhei-agents.1.1.2
    #[test]
    fn a_custom_stdin_prompt_profile_emits_its_prompt_flag_with_no_value() {
        let profile = CustomAgentProfile {
            command: vec!["my-agent".to_string()],
            prompt_flag: Some("--print".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            ..Default::default()
        };
        let argv = composed(&resolved(profile, "my-agent", None), &ResolvedTooling::default(), &[]);

        assert_eq!(
            argv,
            vec!["--print", "--model", "claude-sonnet-4-6", "--"],
            "a declared prompt flag is emitted bare under stdin delivery: {argv:?}"
        );
    }

    /// Live intervention is reached by a different arm and keeps its own flags;
    /// what it gains is the trailing separator the profile now asks for, which
    /// Claude Code accepts with nothing after it. §FS-rhei-agents.1.1.2 The
    /// strict MCP option every Claude Code spawn carries sits ahead of it.
    /// §FS-rhei-agents.2.2
    #[test]
    fn builtin_claude_code_intervention_keeps_stream_json_and_ends_at_the_separator() {
        let mut profile = builtin("claude-code");
        profile.intervene_stdin = true;
        let argv =
            composed(&resolved(profile, "claude-code", None), &ResolvedTooling::default(), &[]);

        assert_eq!(
            argv,
            vec![
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--model",
                "claude-sonnet-4-6",
                "--strict-mcp-config",
                "--",
            ],
            "the stream-json command line gains strict MCP and the separator, nothing else: {argv:?}"
        );
    }

    /// The stream-json stdin transport follows the resolved family, so a
    /// wrapped Claude Code that holds stdin open is handed intervention
    /// messages in the dialect it is already speaking on the way out. There is
    /// no end-to-end case for this on purpose: an `intervene_stdin` profile
    /// holds the child's stdin open, and the suite's fixture prelude blocks on
    /// `sys.stdin.read()` until EOF. §FS-rhei-agents.1.1.2
    #[test]
    fn a_wrapped_claude_code_gets_the_stream_json_stdin_transport() {
        let mut wrapped = builtin("claude-code");
        wrapped.family = Some("claude-code".to_string());
        wrapped.command = vec!["wrapper".to_string()];
        wrapped.intervene_stdin = true;
        assert_eq!(
            agent_stdin_format(&resolved(wrapped.clone(), "cld", None)),
            AgentStdinFormat::ClaudeCodeStreamJson,
            "the family decides the transport, not the id the operator chose"
        );

        // The compatibility floor on both sides: no family is still the id,
        // and a family that is not Claude Code is still a plain line.
        let mut bare = wrapped.clone();
        bare.family = None;
        assert_eq!(
            agent_stdin_format(&resolved(bare, "cld", None)),
            AgentStdinFormat::PlainLine
        );
        let mut codex = wrapped;
        codex.family = Some("codex".to_string());
        assert_eq!(
            agent_stdin_format(&resolved(codex, "cld", None)),
            AgentStdinFormat::PlainLine
        );
    }

    #[test]
    fn supported_agents_keep_expected_prompt_transports() {
        let agents = built_in_agents();
        // `-p` stays declared and stops carrying the prompt: it is what puts
        // Claude Code in print mode. agent-grounds/rhei#179 §FS-rhei-agents.2
        let claude = agents.get("claude-code").expect("claude-code profile");
        assert_eq!(claude.prompt_flag.as_deref(), Some("-p"));
        assert!(claude.stdin_prompt);

        let codex = agents.get("codex").expect("codex profile");
        assert_eq!(codex.prompt_flag.as_deref(), None);
        assert!(codex.stdin_prompt);

        // The prompt moves to stdin under a bare `-p`, the print flag pi keeps.
        // agent-grounds/rhei#659 §FS-rhei-agents.2 §FS-rhei-agents.2.2.2
        let pi = agents.get("pi").expect("pi profile");
        assert_eq!(pi.prompt_flag.as_deref(), Some("-p"));
        assert!(pi.stdin_prompt);
    }

    /// A settings entry laid over the built-ins the way settings lay it: the
    /// fields the entry writes win, the rest are its family's.
    fn laid_over_built_ins(id: &str, entry: serde_json::Value) -> CustomAgentProfile {
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

    fn skill_tooling() -> ResolvedTooling {
        ResolvedTooling {
            skills: vec![ResolvedSkillEntry {
                id: "review".to_string(),
                optional: false,
                definition: Some(SkillProfile {
                    path: "/skills/review".to_string(),
                    description: None,
                }),
            }],
            ..Default::default()
        }
    }

    /// agent-grounds/rhei#659: the built-in `pi` carried the whole prompt as the
    /// value of `-p`, past Linux's per-argument cap. It now pipes it, under a
    /// bare `-p`, and ends with no separator, which pi refuses.
    /// §FS-rhei-agents.2 §FS-rhei-agents.2.2.2
    #[test]
    fn builtin_pi_pipes_its_prompt_under_a_bare_p_and_ends_with_no_separator() {
        let argv = composed(&resolved(builtin("pi"), "pi", None), &skill_tooling(), &[]);

        assert!(
            !argv.iter().any(|arg| arg.contains(PROMPT)),
            "the prompt must travel on stdin, not in argv: {argv:?}"
        );
        assert!(
            argv.windows(2).any(|pair| pair == ["-p", "--model"]),
            "-p is emitted with no value, directly ahead of the model flag: {argv:?}"
        );
        assert!(
            argv.windows(2).any(|pair| pair == ["--skill", "review"]),
            "the skill flag keeps its slot: {argv:?}"
        );
        assert!(!argv.iter().any(|arg| arg == "--"), "pi refuses a `--`: {argv:?}");
    }

    /// The family decides, so a wrapper of pi inherits the stdin transport and
    /// is spared the separator too. §FS-rhei-agents.2.2.2
    #[test]
    fn a_pi_family_wrapper_pipes_its_prompt_with_no_separator() {
        let wrapper = laid_over_built_ins("pi-wrap", serde_json::json!({"family": "pi", "command": ["wrapper"]}));
        let argv = composed(&resolved(wrapper, "pi-wrap", None), &ResolvedTooling::default(), &[]);

        assert!(!argv.iter().any(|arg| arg.contains(PROMPT)), "prompt on stdin: {argv:?}");
        assert!(!argv.iter().any(|arg| arg == "--"), "pi refuses a `--`: {argv:?}");
    }

    /// The help's own remedy: a same-id `agents.pi` entry that replaces the
    /// built-in wholesale and turns on `stdin_prompt`. Its family is still its
    /// own id, `pi`, so it gets no separator and starts. §FS-rhei-agents.2.2.2
    #[test]
    fn a_same_id_pi_entry_with_stdin_prompt_ends_with_no_separator() {
        let entry = CustomAgentProfile {
            command: vec!["pi".to_string()],
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            ..Default::default()
        };
        let argv = composed(&resolved(entry, "pi", None), &ResolvedTooling::default(), &[]);

        assert!(!argv.iter().any(|arg| arg.contains(PROMPT)), "prompt on stdin: {argv:?}");
        assert!(
            argv.windows(2).any(|pair| pair == ["-p", "--model"]),
            "-p is emitted with no value: {argv:?}"
        );
        assert!(!argv.iter().any(|arg| arg == "--"), "pi refuses a `--`: {argv:?}");
    }

    /// A written `"stdin_prompt": false` wins over the inherited `true`, so a
    /// pi-family entry can still carry the prompt as `-p <prompt>`, with no
    /// separator either way. §FS-rhei-agents.2.2.2 §FS-rhei-agents.1.1.2
    #[test]
    fn a_pi_family_entry_that_turns_stdin_off_carries_the_prompt_after_p() {
        let entry = laid_over_built_ins(
            "pi-argv",
            serde_json::json!({"family": "pi", "command": ["pi"], "stdin_prompt": false}),
        );
        let argv = composed(&resolved(entry, "pi-argv", None), &ResolvedTooling::default(), &[]);

        assert!(
            argv.windows(2).any(|pair| pair == ["-p", PROMPT]),
            "the prompt is the value of -p: {argv:?}"
        );
        assert!(!argv.iter().any(|arg| arg == "--"), "no separator: {argv:?}");
    }

    /// The exception is the `pi` family's alone: every other stdin profile still
    /// closes its command line with `--`. §FS-rhei-agents.2.2
    #[test]
    fn builtin_claude_code_and_codex_still_end_at_the_separator() {
        for id in ["claude-code", "codex"] {
            let argv = composed(&resolved(builtin(id), id, None), &skill_tooling(), &[]);
            assert_eq!(argv.last().map(String::as_str), Some("--"), "{id}: {argv:?}");
        }
    }

    /// What a stdin command line ends with: the separator and anything after it.
    fn ending(argv: &[String]) -> &[String] {
        let separator = argv.iter().position(|arg| arg == "--").unwrap_or(argv.len());
        &argv[separator..]
    }

    /// The built-in `codex` profile, and an entry that runs `codex exec` with
    /// the built-in's prompt transport but under another id.
    fn codex_exec_entry() -> CustomAgentProfile {
        CustomAgentProfile {
            command: vec!["codex".to_string(), "exec".to_string()],
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            ..Default::default()
        }
    }

    /// agent-grounds/rhei#670: `codex exec fork <id>` with no PROMPT forks and
    /// exits 0 without reading stdin, so a codex line closes its options with
    /// `--` and then names stdin as the prompt with `-`. §FS-rhei-agents.2.2.2
    #[test]
    #[ignore = "red until #670: the built-in codex still ends at a bare `--`; implement removes this"]
    fn builtin_codex_ends_a_stdin_line_with_the_separator_then_a_dash() {
        let argv = composed(&resolved(builtin("codex"), "codex", None), &skill_tooling(), &[]);

        assert!(!argv.iter().any(|arg| arg.contains(PROMPT)), "prompt on stdin: {argv:?}");
        assert_eq!(ending(&argv), ["--", "-"], "codex ends `-- -`: {argv:?}");
    }

    /// The family decides, so a codex wrapper and a same-id `agents.codex` entry
    /// end as the built-in does. §FS-rhei-agents.2.2.2 §FS-rhei-agents.1.1.2
    #[test]
    #[ignore = "red until #670: codex-family entries still end at a bare `--`; implement removes this"]
    fn a_codex_family_wrapper_and_a_same_id_codex_entry_end_with_the_separator_then_a_dash() {
        let wrapper =
            laid_over_built_ins("cdx", serde_json::json!({"family": "codex", "command": ["wrap"]}));
        let wrapped = composed(&resolved(wrapper, "cdx", None), &ResolvedTooling::default(), &[]);
        assert_eq!(ending(&wrapped), ["--", "-"], "a family: codex wrapper: {wrapped:?}");

        let same_id =
            composed(&resolved(codex_exec_entry(), "codex", None), &ResolvedTooling::default(), &[]);
        assert_eq!(ending(&same_id), ["--", "-"], "a same-id codex entry: {same_id:?}");
    }

    /// The ticket's own shape: codex's fork reached through the resume slot,
    /// with an MCP server attached. Everything rhei says to codex stays ahead
    /// of the separator, and the stdin marker is the last argument.
    /// §FS-rhei-agents.2.2.2 §FS-rhei-snapshots.10.1 §FS-rhei-snapshots.9.3.4
    #[test]
    #[ignore = "red until #670: a codex fork line still ends `fork <id> --`, so codex reads no prompt; implement removes this"]
    fn a_codex_fork_keeps_every_option_before_the_separator_and_ends_with_a_dash() {
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "linear".to_string(),
                optional: false,
                definition: Some(McpServerProfile {
                    command: Some(vec!["mcp-linear".to_string()]),
                    ..Default::default()
                }),
            }],
            ..Default::default()
        };
        let session = "01a1d2ce-0977-79d3-a8d0-e0dd240cac0d".to_string();
        let snapshot_args = vec!["fork".to_string(), session.clone()];
        let argv = composed(&resolved(builtin("codex"), "codex", None), &tooling, &snapshot_args);

        let separator = position(&argv, "--");
        let fork = position(&argv, "fork");
        assert_eq!(argv.get(fork + 1), Some(&session), "fork carries the session id: {argv:?}");
        assert!(fork + 1 < separator, "fork <id> precedes the separator: {argv:?}");
        let overrides: Vec<usize> =
            argv.iter().enumerate().filter(|(_, arg)| *arg == "-c").map(|(i, _)| i).collect();
        assert!(
            argv.iter().any(|arg| arg.starts_with("mcp_servers=")),
            "the MCP server is attached through a native override: {argv:?}"
        );
        assert!(
            overrides.iter().all(|&i| i + 1 < separator),
            "every -c override and its value precede the separator: {argv:?}"
        );
        assert_eq!(ending(&argv), ["--", "-"], "the stdin marker is the last argument: {argv:?}");
    }

    /// Only codex gains the `-`: pi keeps no separator, Claude Code and an entry
    /// with no family keep a bare `--`, and so does an entry that runs codex
    /// without declaring the family. §FS-rhei-agents.2.2.2
    #[test]
    fn every_family_but_codex_keeps_its_stdin_ending() {
        let pi = composed(&resolved(builtin("pi"), "pi", None), &skill_tooling(), &[]);
        assert!(ending(&pi).is_empty(), "pi ends with nothing: {pi:?}");
        assert_ne!(pi.last().map(String::as_str), Some("-"), "pi: {pi:?}");

        let claude =
            composed(&resolved(builtin("claude-code"), "claude-code", None), &skill_tooling(), &[]);
        assert_eq!(ending(&claude), ["--"], "claude-code: {claude:?}");

        let custom = CustomAgentProfile {
            command: vec!["my-agent".to_string()],
            stdin_prompt: true,
            ..Default::default()
        };
        let custom = composed(&resolved(custom, "my-agent", None), &skill_tooling(), &[]);
        assert_eq!(ending(&custom), ["--"], "a family-less custom entry: {custom:?}");

        let familyless_codex = composed(
            &resolved(codex_exec_entry(), "my-codex", None),
            &ResolvedTooling::default(),
            &[],
        );
        assert_eq!(
            ending(&familyless_codex),
            ["--"],
            "an entry that runs codex but declares no family: {familyless_codex:?}"
        );
    }
}
