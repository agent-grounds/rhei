// Unit tests for the shared error-guidance helpers. §FS-rhei-errors
// These decide what a user pastes, so edge cases are tested directly.

mod error_guidance_tests {
    use super::super::*;

    /// Quoting a word so a shell hands it back unchanged.
    ///
    /// Which quote a platform uses is `rhei_core::platform`'s question and is
    /// pinned there, both halves, on every platform. What is asked here is the
    /// decision this CLI depends on: bare when the shell would not touch it,
    /// quoted when it would.

    // §FS-rhei-errors.2
    mod shell_quote {
        use super::*;

        #[test]
        fn leaves_ordinary_words_bare() {
            for value in ["rhei", "instantiate", "--list-inputs", "a/b/c.md", "1.2.3", "a,b"] {
                assert_eq!(shell_quote(value), value, "{value} should not need quoting");
            }
        }

        #[test]
        fn quotes_what_a_shell_would_otherwise_read() {
            // The motivating case is the selector: unquoted, zsh fails with
            // `no matches found` before rhei is executed at all.
            for value in
                ["", "codex[yolo]:openai:gpt-5.5", "*.md", "a b", "~/plans", "it's", "one\ntwo"]
            {
                let quoted = shell_quote(value);
                assert_ne!(quoted, value, "{value:?} must not be printed bare");
                assert!(quoted.len() > value.len(), "{value:?} came back as {quoted:?}");
            }
        }

        #[test]
        fn quotes_a_leading_equals_where_the_shell_expands_it() {
            // zsh EQUALS expansion turns a leading `=word` into the path of
            // `word`, even though `=` is harmless elsewhere in a word. `cmd`
            // has no such expansion, so there the word stands as written.
            assert_eq!(shell_quote("=less"), if cfg!(windows) { "=less" } else { "'=less'" });
            assert_eq!(shell_quote("a=b"), "a=b");
        }
    }

    /// Rendering one argument, keeping a `KEY=` readable. §FS-rhei-errors.2
    mod shell_arg {
        use super::*;

        #[test]
        fn quotes_only_the_value_of_an_assignment() {
            // Built with the quoting the product would use, because what this
            // function decides is *where* the quotes go, not which they are.
            assert_eq!(
                shell_arg("agent=codex[yolo]:openai:gpt-5.5"),
                format!("agent={}", shell_quote("codex[yolo]:openai:gpt-5.5"))
            );
            assert_eq!(shell_arg("subject=<value>"), format!("subject={}", shell_quote("<value>")));
        }

        #[test]
        fn leaves_an_assignment_that_needs_no_quoting_alone() {
            assert_eq!(shell_arg("subject=docs"), "subject=docs");
        }

        #[test]
        fn quotes_a_flag_whole() {
            assert_eq!(shell_arg("--set"), "--set");
            assert_eq!(shell_arg("--output"), "--output");
        }

        #[test]
        fn treats_a_non_identifier_left_hand_side_as_a_plain_word() {
            // A path that happens to contain `=` is not an assignment.
            assert_eq!(shell_arg("/tmp/a=b/c"), "/tmp/a=b/c");
            assert_eq!(shell_arg("=x"), shell_quote("=x"));
        }

        #[test]
        fn round_trips_through_a_shell_unchanged() {
            // What the shell hands back is `key` + the unquoted value, which is
            // the original argument.
            let original = "agent=codex[yolo]:openai:gpt-5.5";
            let rendered = shell_arg(original);
            let quote = if cfg!(windows) { '"' } else { '\'' };
            let unquoted = rendered.replace(quote, "");
            assert_eq!(unquoted, original);
        }
    }

    #[test]
    fn shell_command_joins_quoted_parts() {
        assert_eq!(
            shell_command(["rhei", "instantiate", "guided", "agent=a[b]:c:d"]),
            format!("rhei instantiate guided agent={}", shell_quote("a[b]:c:d"))
        );
    }

    /// Suggesting the name the user probably meant. §FS-rhei-errors.1.3
    mod near_misses {
        use super::*;

        fn names(values: &[&str]) -> Vec<String> {
            values.iter().map(|value| value.to_string()).collect()
        }

        #[test]
        fn finds_a_single_character_typo() {
            let known = names(&["subject", "analysis_brief", "plan_title"]);
            assert_eq!(did_you_mean("subjekt", &known).as_deref(), Some("Did you mean 'subject'?"));
        }

        #[test]
        fn ignores_case() {
            let known = names(&["claude-code", "codex"]);
            assert_eq!(
                did_you_mean("Claude-Code", &known).as_deref(),
                Some("Did you mean 'claude-code'?")
            );
        }

        #[test]
        fn lists_the_candidates_when_nothing_is_close() {
            let known = names(&["subject", "brief"]);
            assert_eq!(
                did_you_mean("wildly-different-name", &known).as_deref(),
                Some("Valid values: subject, brief.")
            );
        }

        #[test]
        fn truncates_a_long_candidate_list() {
            let known = (0..20).map(|i| format!("input_number_{i}")).collect::<Vec<_>>();
            let hint = did_you_mean("zzzzzzzzzzzzzzzzzzzzzzz", &known).expect("a hint");
            assert!(hint.starts_with("Valid values include: input_number_0, "), "got: {hint}");
            assert!(hint.ends_with("(and 12 more)."), "got: {hint}");
        }

        #[test]
        fn drops_duplicates_so_a_merged_registry_lists_each_id_once() {
            // Built-ins are seeded into the settings registry, so the same id
            // can reach this twice; it must still be offered once.
            let known = names(&["claude-code", "codex", "claude-code", "codex"]);
            assert_eq!(
                did_you_mean("nothing-alike-at-all", &known).as_deref(),
                Some("Valid values: claude-code, codex.")
            );
        }

        #[test]
        fn declines_when_there_are_no_candidates() {
            assert_eq!(did_you_mean("anything", &[]), None);
        }

        #[test]
        fn declines_on_empty_input() {
            let known = names(&["subject"]);
            assert_eq!(nearest_match("", &known), None);
            assert_eq!(nearest_match("   ", &known), None);
        }

        #[test]
        fn breaks_ties_by_name_so_suggestions_are_stable() {
            let known = names(&["beta", "alpha"]);
            // Both are distance 5 from a 5-character input; the lower name wins.
            assert_eq!(nearest_match("gamma", &known).as_deref(), None);
            let known = names(&["bx", "ax"]);
            assert_eq!(nearest_match("cx", &known).as_deref(), Some("ax"));
        }
    }

    #[test]
    fn levenshtein_distance_counts_single_edits() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("abc", "ab"), 1);
        assert_eq!(levenshtein_distance("ab", "abc"), 1);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn levenshtein_distance_counts_characters_not_bytes() {
        // A multi-byte character is one edit, not one per byte.
        assert_eq!(levenshtein_distance("café", "cafe"), 1);
        assert_eq!(levenshtein_distance("naïve", ""), 5);
    }

    /// A `--agent` value that is really a selector. §FS-rhei-errors.1.2
    mod agent_flag_selector {
        use super::*;

        #[test]
        fn declines_for_a_bare_id() {
            assert_eq!(agent_flag_selector_help("claude-code", &[]), None);
        }

        #[test]
        fn rewrites_a_selector_into_the_flags_that_carry_it() {
            let help = agent_flag_selector_help("my-agent[yolo]:some-model", &[])
                .expect("a selector should be recognized");
            assert!(
                help.contains("--agent my-agent --agent-mode yolo --model some-model"),
                "got: {help}"
            );
        }

        #[test]
        fn handles_a_selector_without_a_mode() {
            let help =
                agent_flag_selector_help("my-agent:some-model", &[]).expect("a selector");
            assert!(help.contains("--agent my-agent --model some-model"), "got: {help}");
            assert!(!help.contains("--agent-mode"), "got: {help}");
        }

        #[test]
        fn also_names_a_typo_inside_the_selector() {
            let known = vec!["claude-code".to_string()];
            let help = agent_flag_selector_help("claude-codee:m", &known).expect("a selector");
            assert!(help.contains("--agent claude-codee --model m"), "got: {help}");
            assert!(help.contains("Did you mean 'claude-code'?"), "got: {help}");
        }

        #[test]
        fn stays_silent_about_an_id_that_is_defined() {
            let known = vec!["my-agent".to_string()];
            let help = agent_flag_selector_help("my-agent:m", &known).expect("a selector");
            assert!(!help.contains("Did you mean"), "got: {help}");
        }

        #[test]
        fn declines_when_the_value_is_not_a_parseable_selector() {
            assert_eq!(agent_flag_selector_help("a:b:c:d:e", &[]), None);
        }
    }

    #[test]
    fn unknown_agent_help_does_not_repeat_a_merged_registry() {
        let known = vec!["claude-code".to_string(), "codex".to_string(), "codex".to_string()];
        let help = unknown_agent_help(
            "totally-unrelated",
            &known,
            ProjectSettingsFile::default().relative_path(),
        );
        assert_eq!(help.matches("codex").count(), 1, "got: {help}");
        assert!(help.contains("agents.<id>"), "got: {help}");
    }

    /// A registry refusal names the key its entry is written under and the
    /// files it may be written in, at every site that makes one.
    /// §FS-rhei-errors.1.4
    mod settings_location {
        use super::*;

        /// The clause, keyed to whichever registry entry is missing.
        fn clause(key: &str, project_settings: &str) -> String {
            format!("`{key}` in {project_settings} or ~/.config/rhei/settings.json")
        }

        /// The refusal one target selector earns from `validate`.
        fn refusal(settings: &RheiSettings, selector: &str) -> String {
            let machine = super::super::machine_with_states(&format!(
                "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    target: {selector}\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n"
            ));
            let errs = validate_machine_settings_references(&machine, settings);
            errs.iter()
                .find(|e| e.contains(selector))
                .unwrap_or_else(|| panic!("'{selector}' must be refused: {errs:?}"))
                .clone()
        }

        /// The spawn-time agent help is where this wording already exists, so
        /// it is the wording every other refusal owes — one clause, not a
        /// second one beside it. Nothing here is new behaviour: it is the
        /// byte-for-byte guard on the site that must not move while the clause
        /// is factored out for the sites that lack it. §FS-rhei-errors.6
        #[test]
        fn the_spawn_time_agent_help_is_the_shared_wording() {
            for file in [ProjectSettingsFile::Current, ProjectSettingsFile::Deprecated] {
                let help = unknown_agent_help("nope", &[], file.relative_path());
                assert_eq!(
                    help,
                    format!("Define it under {}.", clause("agents.<id>", file.relative_path())),
                    "the spawn-time help is the clause every registry refusal shares"
                );
            }
        }

        /// An agent that declares no modes: the brackets are the mistake, and
        /// declaring the mode is the other way out, so both remedies are given.
        #[test]
        fn a_modeless_agent_is_offered_both_remedies() {
            let mut settings = super::super::default_settings();
            settings.agents.insert(
                "noop".to_string(),
                CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
            );
            let message = refusal(&settings, "noop[review]:openai:gpt");
            assert!(message.contains("it declares no modes"), "got: {message}");
            assert!(message.contains("drop the brackets"), "got: {message}");
            assert!(
                message.contains(&clause(
                    "agents.noop.modes",
                    ProjectSettingsFile::default().relative_path()
                )),
                "got: {message}"
            );
        }

        /// An agent that declares modes: the list says what exists, the clause
        /// says where to write what does not. This is the reported case.
        #[test]
        fn a_listed_mode_registry_still_says_where_to_add_one() {
            let message =
                refusal(&super::super::default_settings(), "codex[review]:openai:gpt");
            assert!(message.contains("known modes: yolo"), "got: {message}");
            assert!(
                message.contains(&clause(
                    "agents.codex.modes",
                    ProjectSettingsFile::default().relative_path()
                )),
                "got: {message}"
            );
        }

        /// No agents at all: the agent is what is missing, so the entry is the
        /// key, and the one message that already named a file gains the other.
        #[test]
        fn an_empty_registry_names_the_entry_and_both_files() {
            let message = refusal(&RheiSettings::default(), "noop[review]:openai:gpt");
            assert!(message.contains("no agents are configured"), "got: {message}");
            assert!(
                message.contains(
                    &clause("agents.<id>", ProjectSettingsFile::default().relative_path())
                ),
                "got: {message}"
            );
        }
    }

    /// Every diagnostic carries a next action, guarded here rather than left
    /// to review because coverage is a property of the whole crate rather than
    /// of any one call site. §FS-rhei-errors.6
    #[test]
    fn every_miette_site_carries_help() {
        super::diagnostic_guidance_guard::check_sources(&["miette"]);
    }

    /// Filesystem remedies derived from the error kind. §FS-rhei-errors.6
    mod io_help {
        use super::*;
        use std::io::ErrorKind;

        #[test]
        fn a_missing_file_in_an_existing_directory_is_a_spelling_check() {
            // Tests run with the crate root as the working directory, so `src`
            // is a directory that really exists.
            let help = io_error_help(&PathBuf::from("src/absent.md"), ErrorKind::NotFound);
            assert!(help.contains("Check the spelling"), "got: {help}");
            assert!(help.contains("ls src"), "got: {help}");
        }

        #[test]
        fn a_missing_directory_is_an_mkdir() {
            let help =
                io_error_help(&PathBuf::from("no/such/dir/absent.md"), ErrorKind::NotFound);
            assert!(help.contains("mkdir -p"), "got: {help}");
        }

        #[test]
        fn a_bare_filename_has_no_directory_to_suggest() {
            let help = io_error_help(&PathBuf::from("absent.md"), ErrorKind::NotFound);
            assert!(help.contains("Check the spelling"), "got: {help}");
            assert!(!help.contains("ls "), "got: {help}");
        }

        #[test]
        fn permission_denied_points_at_the_path_itself() {
            let help = io_error_help(&PathBuf::from("/etc/shadow"), ErrorKind::PermissionDenied);
            assert!(help.contains("ls -ld /etc/shadow"), "got: {help}");
        }

        #[test]
        fn a_path_needing_quotes_is_quoted() {
            let help =
                io_error_help(&PathBuf::from("/tmp/a b/plan.md"), ErrorKind::PermissionDenied);
            assert!(help.contains(&shell_quote("/tmp/a b/plan.md")), "got: {help}");
        }
    }

    /// What a spawn failure says when the operating system refused the size of
    /// the command line rhei composed, and what it says when it did not.
    ///
    /// The evidence arm is the platform's — errno on Linux and macOS, a
    /// measurement on Windows — so each case below asks for the one its own
    /// platform can answer. The decision they surround is the same everywhere.

    // §FS-rhei-errors.7 §FS-rhei-errors.7.1
    mod spawn_failure_guidance {
        use super::*;

        /// Larger than any platform's cap, so the measured arm fires on Windows
        /// wherever the errno arm fires elsewhere.
        const OVERSIZED: usize = 2 * 1024 * 1024;

        /// Whether the prompt reached the command line, which the caller reads
        /// off the composed argument vector. §FS-rhei-errors.7.2
        const IN_ARGV: bool = true;
        const OFF_ARGV: bool = false;

        /// The failure this platform reports for a command line it will not
        /// take: `E2BIG` on Linux and macOS, and on Windows something
        /// indistinct that only the measurement explains.
        fn too_large_for_this_platform() -> std::io::Error {
            if cfg!(windows) {
                std::io::Error::from(std::io::ErrorKind::Other)
            } else {
                std::io::Error::from_raw_os_error(7)
            }
        }

        /// The help's list of stdin agents is the built-ins that declare
        /// `stdin_prompt`, so it cannot fall out of step with them again:
        /// agent-grounds/rhei#659 left `pi` off it. §FS-rhei-errors.7
        #[test]
        fn the_oversized_prompt_help_names_every_builtin_that_reads_stdin() {
            let help = oversized_prompt_help().to_string();
            for id in ["claude-code", "codex", "pi"] {
                assert!(help.contains(&format!("'{id}'")), "the help names '{id}': {help}");
            }
            let agents = built_in_agents();
            for (id, profile) in &agents {
                let quoted = format!("'{id}'");
                assert_eq!(
                    help.contains(&quoted),
                    profile.stdin_prompt,
                    "the help names {quoted} exactly when that built-in reads stdin: {help}"
                );
            }
        }

        #[test]
        fn an_oversized_prompt_is_named_as_the_cause() {
            let said = spawn_failure_guidance(
                &too_large_for_this_platform(),
                IN_ARGV,
                OVERSIZED,
                OVERSIZED,
            );

            assert!(
                said.detail.contains(&format!("{OVERSIZED} bytes")),
                "the size the user cannot see is what makes the failure actionable: {:?}",
                said.detail
            );
            assert!(
                said.help.contains("stdin"),
                "the remedy is prompt delivery, not PATH: {}",
                said.help
            );
            assert!(
                !said.help.contains("PATH"),
                "the binary ran a moment ago; PATH is the wrong next action: {}",
                said.help
            );
        }

        #[test]
        fn a_missing_binary_keeps_the_path_remedy_it_had() {
            let said = spawn_failure_guidance(
                &std::io::Error::from(std::io::ErrorKind::NotFound),
                IN_ARGV,
                OVERSIZED,
                OVERSIZED,
            );

            assert_eq!(said.detail, "", "nothing is added to a failure size does not explain");
            assert_eq!(said.help, spawn_not_started_help());
        }

        /// The same rule where the evidence is a measurement rather than an
        /// errno: a failure that names its own cause settles the question before
        /// the ruler comes out. §FS-rhei-errors.7.1
        #[test]
        fn a_failure_the_platform_names_is_not_overridden_by_the_measurement() {
            let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
            let said = spawn_failure_guidance(&denied, IN_ARGV, OVERSIZED, OVERSIZED);

            assert_eq!(said.detail, "", "a long command line denied nobody permission");
            assert_eq!(said.help, spawn_not_started_help());
        }

        /// The kernel counts the terminating NUL, so an argument of exactly the
        /// limit is already refused — and inside that one-byte window the reader
        /// used to get the `PATH` misdirection. §FS-rhei-errors.7.2
        #[test]
        fn a_prompt_of_exactly_the_limit_is_what_the_platform_refused() {
            let at_the_cap = COMMAND_LINE_LIMIT_BYTES;
            let too_large = too_large_for_this_platform();
            let said = spawn_failure_guidance(&too_large, IN_ARGV, at_the_cap, at_the_cap + 40);

            assert!(said.detail.contains("prompt"), "a prompt at the cap: {:?}", said.detail);
            assert!(said.help.contains("stdin"), "the remedy is delivery: {}", said.help);
        }

        /// A transport that puts no part of the prompt on the command line
        /// cannot have been stopped by the prompt's size, so the line's own
        /// measurement is reported and the remedy stays where it was.
        #[test]
        fn a_prompt_that_never_reached_argv_is_not_blamed_for_the_line() {
            let said = spawn_failure_guidance(
                &too_large_for_this_platform(),
                OFF_ARGV,
                OVERSIZED,
                OVERSIZED,
            );

            assert!(
                !said.detail.contains("prompt"),
                "the prompt is not on this line and cannot be its cause: {:?}",
                said.detail
            );
            assert!(
                said.detail.contains(&format!("{OVERSIZED} bytes")),
                "what was measured is still worth stating: {:?}",
                said.detail
            );
            assert_eq!(said.help, spawn_not_started_help());
        }

        /// The reviewer's case, and on a total cap the ordinary one: the line is
        /// over because of a fixed argument the profile carries, not because of
        /// the prompt. Blaming the prompt would print a byte count beside a
        /// larger limit and offer a remedy that shortens nothing.
        #[test]
        fn an_oversized_argument_that_is_not_the_prompt_leaves_the_prompt_alone() {
            let line = COMMAND_LINE_LIMIT_BYTES * 2;
            let said =
                spawn_failure_guidance(&too_large_for_this_platform(), IN_ARGV, 64, line);

            assert!(
                !said.detail.contains("prompt"),
                "a 64-byte prompt did not put this line over the cap: {:?}",
                said.detail
            );
            assert!(
                said.detail.contains(&format!("{line} bytes")),
                "the size that was actually over is the one to state: {:?}",
                said.detail
            );
            assert_eq!(
                said.help,
                spawn_not_started_help(),
                "moving this prompt to stdin would leave the line as long as it was"
            );
        }

        /// Windows has only the measurement to go on, so a command line it
        /// could have taken is not blamed on the prompt.
        #[test]
        fn a_command_line_within_the_limit_is_not_blamed_on_its_size() {
            let said = spawn_failure_guidance(
                &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                IN_ARGV,
                64,
                128,
            );

            assert_eq!(said.detail, "");
            assert_eq!(said.help, spawn_not_started_help());
        }

        /// The transport is read off the argument vector, not off one profile
        /// field: Claude Code's live-intervention arm sends the prompt down the
        /// stdin pipe without declaring `stdin_prompt`, and must not be told its
        /// prompt was too long for a line it is not on. §FS-rhei-errors.7.2
        #[test]
        fn the_stream_json_arm_is_read_as_keeping_its_prompt_off_the_line() {
            let mut profile = built_in_agents().remove("claude-code").expect("built-in profile");
            profile.intervene_stdin = true;
            profile.stdin_prompt = false;
            let resolved = ResolvedAgent {
                agent: AgentConfig::from("claude-code"),
                profile,
                mode: None,
                target: None,
                model: None,
                model_provider: None,
                model_name: None,
                timeout_secs: Some(60),
                autonomous_args: Vec::new(),
            };
            let runtime_dir = tempfile::tempdir().expect("tmpdir");
            let prompt = "x".repeat(OVERSIZED);
            let cmd = build_agent_command(
                &resolved,
                &prompt,
                Path::new("/tmp/workspace"),
                None,
                None,
                "task-1",
                "pending",
                1,
                1,
                &ResolvedTooling::default(),
                runtime_dir.path(),
                &[],
            );

            assert!(
                !prompt_reached_argv(&cmd, &prompt),
                "stream-json delivery keeps the prompt off argv even with stdin_prompt unset"
            );
            let said = spawn_failure_guidance(
                &too_large_for_this_platform(),
                prompt_reached_argv(&cmd, &prompt),
                prompt.len(),
                composed_command_line_bytes(&cmd),
            );
            assert!(
                !said.detail.contains("prompt"),
                "this prompt never reached the command line: {:?}",
                said.detail
            );
            assert_eq!(said.help, spawn_not_started_help());
        }
    }
}
