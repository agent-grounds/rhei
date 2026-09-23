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
    /// Claude Code accepts with nothing after it. §FS-rhei-agents.1.1.2
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
                "--",
            ],
            "the stream-json command line gains the separator and nothing else: {argv:?}"
        );
    }
}
