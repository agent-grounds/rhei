    use super::*;
    use clap::CommandFactory;

    /// The top-level help is a hand-maintained `help_template` string, so a
    /// newly added subcommand is invisible in `rhei --help` until someone
    /// remembers to list it. `init` shipped that way. Fail loudly instead.
    #[test]
    fn every_subcommand_is_listed_in_the_top_level_help() {
        let command = cli_command();
        let help = command.clone().render_help().to_string();
        // Match a listing entry (`  <name>  <description>`), not a bare
        // substring: `list` and `run` also occur inside descriptions.
        let listed = |name: &str| {
            help.lines().any(|line| {
                line.starts_with("  ")
                    && line.trim_start().strip_prefix(name).is_some_and(|rest| {
                        rest.starts_with(char::is_whitespace) || rest.is_empty()
                    })
            })
        };
        let missing: Vec<&str> =
            command.get_subcommands().map(|sub| sub.get_name()).filter(|n| !listed(n)).collect();
        assert!(
            missing.is_empty(),
            "subcommands missing from the `help_template` in cli_declarations.rs: {missing:?}"
        );
    }

    #[test]
    fn parses_validate_command_with_input() {
        let cli = Cli::try_parse_from(["rhei", "validate", "docs/markdown-plan-compiler.md"])
            .expect("cli should parse");

        assert!(cli.state_machine.is_none());
        match cli.command {
            Commands::Validate { watch, input, .. } => {
                assert!(!watch);
                assert_eq!(input, Some(PathBuf::from("docs/markdown-plan-compiler.md")));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_validate_watch_command_with_input() {
        let cli =
            Cli::try_parse_from(["rhei", "validate", "--watch", "docs/markdown-plan-compiler.md"])
                .expect("cli should parse");

        assert!(cli.state_machine.is_none());
        match cli.command {
            Commands::Validate { watch, input, .. } => {
                assert!(watch);
                assert_eq!(input, Some(PathBuf::from("docs/markdown-plan-compiler.md")));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_json_pretty() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "json",
            "--pretty",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { input, format, pretty, no_color, no_metadata, no_content, .. } => {
                assert_eq!(input, Some(PathBuf::from("docs/markdown-plan-compiler.md")));
                assert_eq!(format, RenderFormat::Json);
                assert!(pretty);
                assert!(!no_color);
                assert!(!no_metadata);
                assert!(!no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_github_toggles() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "github",
            "--no-metadata",
            "--no-content",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { format, no_metadata, no_content, .. } => {
                assert_eq!(format, RenderFormat::Github);
                assert!(no_metadata);
                assert!(no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_progress_no_color() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "progress",
            "--no-color",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { format, no_color, .. } => {
                assert_eq!(format, RenderFormat::Progress);
                assert!(no_color);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_viz_command() {
        let cli = Cli::try_parse_from(["rhei", "viz", "plan.rhei.md", "-o", "out.html", "--open"])
            .expect("cli should parse");
        match cli.command {
            Commands::Viz { input, output, open, .. } => {
                assert_eq!(input, Some(PathBuf::from("plan.rhei.md")));
                assert_eq!(output, Some(PathBuf::from("out.html")));
                assert!(open);
            }
            other => panic!("expected viz command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["rhei", "viz", "workspace"]).expect("cli should parse");
        match cli.command {
            Commands::Viz { input, output, open, .. } => {
                assert_eq!(input, Some(PathBuf::from("workspace")));
                assert!(output.is_none());
                assert!(!open);
            }
            other => panic!("expected viz command, got {other:?}"),
        }
    }

    #[test]
    fn parses_states_command() {
        let cli = Cli::try_parse_from(["rhei", "states"]).expect("cli should parse");
        match cli.command {
            Commands::States { json, .. } => assert!(!json),
            other => panic!("expected states command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["rhei", "states", "--json"]).expect("cli should parse");
        match cli.command {
            Commands::States { json, .. } => assert!(json),
            other => panic!("expected states command, got {other:?}"),
        }
    }

    #[test]
    fn render_state_machine_text_includes_states_and_transitions() {
        let yaml = r#"
name: demo
version: 1
models:
  - gpt-5
  - claude-sonnet
states:
  draft:
    description: planning
    instructions: Wait until author promotes task.
    personality: Ask one sharp planning question first.
    initial: true
    visits: 3
    all_models:
      - gpt-5
      - claude-sonnet
  done:
    description: finished
    model: gpt-5
    final: true
transitions:
  - from: draft
    to: done
    on_enter: cli:record_done
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_text(&machine);

        assert!(rendered.contains("State machine: demo"));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("draft"));
        assert!(rendered.contains("Visits: 3"));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("Personality: Ask one sharp planning question first."));
        assert!(rendered.contains("Wait until author promotes task."));
        assert!(rendered.contains("done [final]"));
        assert!(rendered.contains("Model: gpt-5"));
        assert!(rendered.contains("draft -> done (on_enter=cli:record_done)"));
    }

    #[test]
    fn render_state_machine_json_includes_state_personality() {
        let yaml = r#"
name: demo
version: 1
models:
  - gpt-5
states:
  draft:
    description: planning
    personality: Focus on planning risks.
    visits: 2
    all_models:
      - gpt-5
    initial: true
  done:
    description: done
    final: true
transitions:
  - from: draft
    to: done
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_json(&machine).expect("render JSON");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("parse JSON");

        assert_eq!(json["name"], "demo");
        assert_eq!(json["models"], serde_json::json!(["gpt-5"]));
        assert_eq!(json["states"][0]["personality"], "Focus on planning risks.");
        assert_eq!(json["states"][0]["visits"], 2);
        assert_eq!(json["states"][0]["all_models"], serde_json::json!(["gpt-5"]));
    }

    /// The machine that reproduces the ticket: one poll waiting on a person,
    /// one waiting on CI. §FS-rhei-states.2.5
    fn poll_waiting_on_machine() -> rhei_validator::StateMachine {
        let yaml = r#"
name: approvals
version: 1
states:
  plan-approval:
    description: Wait for the author to answer.
    program: "./check-reply.sh"
    poll:
      interval: 10m
      max_attempts: 60
      waiting_on: author
    initial: true
  ci-watch:
    description: Watch CI.
    program: "./check-ci.sh"
    poll:
      interval: 2m
      max_attempts: 30
  done:
    description: finished
    final: true
transitions:
  - from: plan-approval
    to: plan-approval
  - from: plan-approval
    to: ci-watch
  - from: ci-watch
    to: ci-watch
  - from: ci-watch
    to: done
"#;
        rhei_validator::StateMachine::from_yaml_str(yaml).expect("load")
    }

    /// The `Poll:` line carries the label, and only for the state that
    /// declares one — a CI watch prints exactly what it printed before.
    /// §FS-rhei-states-cmd.4
    #[test]
    fn render_state_machine_text_names_the_person_a_poll_waits_on() {
        let rendered = render_state_machine_text(&poll_waiting_on_machine());

        assert!(
            rendered.contains("Poll: interval=10m, max_attempts=60, waiting_on=author"),
            "missing person-waiting poll line in:\n{rendered}"
        );
        assert!(
            rendered.contains("Poll: interval=2m, max_attempts=30\n"),
            "machine-backoff poll line changed in:\n{rendered}"
        );
    }

    /// Presence is the meaning, so the field is omitted rather than `null` on a
    /// poll that waits on a machine. §FS-rhei-states-cmd.5
    #[test]
    fn render_state_machine_json_carries_waiting_on_only_where_declared() {
        let rendered = render_state_machine_json(&poll_waiting_on_machine()).expect("render JSON");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("parse JSON");
        let state = |name: &str| {
            json["states"]
                .as_array()
                .expect("states array")
                .iter()
                .find(|s| s["name"] == name)
                .expect("state present")
                .clone()
        };

        assert_eq!(state("plan-approval")["poll"]["waiting_on"], "author");
        assert_eq!(
            state("ci-watch")["poll"],
            serde_json::json!({ "interval": "2m", "max_attempts": 30 })
        );
    }

    #[test]
    fn parses_run_command_with_separated_flag_groups() {
        let cli = Cli::try_parse_from([
            "rhei",
            "run",
            "plan.rhei.md",
            "--dry-run",
            "--no-callbacks",
            "--continue-on-error",
            "--parallel",
            "4",
            "--prices",
            "prices.json",
            "--no-agent",
            "--agent",
            "codex",
            "--model",
            "o3",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Run { input, standalone, agent, program, snapshot, .. } => {
                assert_eq!(input, Some(PathBuf::from("plan.rhei.md")));
                assert!(standalone.dry_run);
                assert!(standalone.no_callbacks);
                assert!(standalone.continue_on_error);
                assert_eq!(standalone.parallel, 4);
                assert_eq!(standalone.prices, Some(PathBuf::from("prices.json")));
                assert!(agent.no_agent);
                assert_eq!(agent.agent.as_deref(), Some("codex"));
                assert_eq!(agent.model.as_deref(), Some("o3"));
                assert!(!program.no_program);
                assert_eq!(program.program_timeout.as_deref(), None);
                assert!(snapshot.from_snapshot.is_none());
                assert!(!snapshot.override_inherit);
                assert!(snapshot.snapshot_task.is_none());
                assert!(snapshot.snapshot_target.is_none());
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_run_command_with_snapshot_flags() {
        let cli = Cli::try_parse_from([
            "rhei",
            "run",
            "plan.rhei.md",
            "--from-snapshot",
            "1.2.3:implementation:pending@2:claude-code-anthropic-claude-opus-4-7/g3",
            "--override-inherit",
            "--task",
            "1.2.3",
            "--target",
            "claude-code-anthropic-claude-opus-4-7",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Run { snapshot, .. } => {
                assert_eq!(
                    snapshot.from_snapshot.as_deref(),
                    Some("1.2.3:implementation:pending@2:claude-code-anthropic-claude-opus-4-7/g3")
                );
                assert!(snapshot.override_inherit);
                assert_eq!(snapshot.snapshot_task.as_deref(), Some("1.2.3"));
                assert_eq!(
                    snapshot.snapshot_target.as_deref(),
                    Some("claude-code-anthropic-claude-opus-4-7")
                );
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_rejects_override_inherit_without_from_snapshot() {
        let err = Cli::try_parse_from(["rhei", "run", "plan.rhei.md", "--override-inherit"])
            .expect_err("clap should reject --override-inherit without --from-snapshot");
        let msg = err.to_string();
        assert!(
            msg.contains("--from-snapshot") || msg.contains("requires"),
            "unexpected clap error: {msg}"
        );
    }

    /// Dashboard startup follows the TUI/default and explicit override contract.
    /// §FS-rhei-run-tui.1.6 §FS-rhei-run
    #[test]
    fn dashboard_policy_follows_tui_default_and_explicit_flags() {
        let mut opts = default_run_options();

        assert!(opts.dashboard_enabled(true));
        assert!(!opts.dashboard_enabled(false));

        opts.standalone.no_dashboard = true;
        assert!(!opts.dashboard_enabled(true));
        assert!(!opts.dashboard_enabled(false));

        opts.standalone.no_dashboard = false;
        opts.standalone.dashboard = true;
        assert!(opts.dashboard_enabled(true));
        assert!(opts.dashboard_enabled(false));
    }

    /// Dry runs are planning-only and must not start live dashboard side effects.
    /// §FS-rhei-run.2
    #[test]
    fn dry_run_frontend_never_starts_dashboard() {
        let mut opts = default_run_options();
        opts.standalone.dry_run = true;
        opts.standalone.dashboard = true;

        let machines = ExecutionMachines {
            set: rhei_validator::MachineSet::single(
                rhei_validator::StateMachine::builtin_default(),
            ),
            default_callbacks: CallbackPaths {
                plan_path: PathBuf::from("missing-plan.rhei.md"),
                state_machine_path: None,
                working_dir: PathBuf::from("."),
            },
            per_rhei_callbacks: BTreeMap::new(),
            state_machine_override: None,
        };
        let frontend = start_run_frontend(
            Path::new("."),
            Path::new("missing-plan.rhei.md"),
            &machines,
            &opts,
            1,
            0,
            &RunShutdown::default(),
            &RunIdentity::new(),
        );

        assert!(frontend.dashboard.is_none());
    }

    #[test]
    fn run_help_separates_standalone_and_agent_flags() {
        let mut command = Cli::command();
        let run = command.find_subcommand_mut("run").expect("run subcommand should exist");
        let mut buffer = Vec::new();
        run.write_long_help(&mut buffer).expect("help should render");
        let help = String::from_utf8(buffer).expect("help should be UTF-8");

        assert!(help.contains("Standalone Execution:"));
        assert!(help.contains("--dry-run"));
        assert!(help.contains("--parallel"));
        // §FS-rhei-run.2.1 §FS-rhei-cost-accounting.5.1: custom pricing is a
        // public run option, not an accounting-only internal input.
        assert!(help.contains("--prices <PATH>"));
        assert!(help.contains("Agent Execution:"));
        assert!(help.contains("--no-agent"));
        assert!(help.contains("--agent <AGENT>"));
        assert!(help.contains("--model <MODEL>"));
        assert!(help.contains("Program Execution:"));
        assert!(help.contains("--no-program"));
        assert!(help.contains("--program-timeout <DURATION>"));
        // §FS-rhei-run.2.3: Snapshots flag group.
        assert!(help.contains("Snapshots:"));
        assert!(help.contains("--from-snapshot <REF>"));
        assert!(help.contains("--override-inherit"));
        assert!(help.contains("--task <TASK_ID>"));
        assert!(help.contains("--target <SLUG>"));
    }

    #[test]
    fn parses_version_command() {
        let cli = Cli::try_parse_from(["rhei", "version"]).expect("cli should parse");

        match cli.command {
            Commands::Version => {}
            other => panic!("expected version command, got {other:?}"),
        }
    }

    #[test]
    fn parses_completions_command() {
        let cli = Cli::try_parse_from(["rhei", "completions", "fish"]).expect("cli should parse");

        match cli.command {
            Commands::Completions { shell, install, system, output, dry_run, .. } => {
                assert_eq!(shell, Some(CompletionShell::Fish));
                assert!(!install);
                assert!(!system);
                assert!(output.is_none());
                assert!(!dry_run);
            }
            other => panic!("expected completions command, got {other:?}"),
        }

        let cli =
            Cli::try_parse_from(["rhei", "completions", "powershell"]).expect("cli should parse");
        match cli.command {
            Commands::Completions { shell, .. } => {
                assert_eq!(shell, Some(CompletionShell::PowerShell))
            }
            other => panic!("expected completions command, got {other:?}"),
        }
    }

    /// §FS-rhei-completions.2
    #[test]
    fn parses_completions_without_shell() {
        let cli = Cli::try_parse_from(["rhei", "completions"]).expect("cli should parse");
        match cli.command {
            Commands::Completions { shell, .. } => assert_eq!(shell, None),
            other => panic!("expected completions command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["rhei", "completions", "--install", "--dry-run"])
            .expect("cli should parse");
        match cli.command {
            Commands::Completions { shell, install, dry_run, .. } => {
                assert_eq!(shell, None);
                assert!(install);
                assert!(dry_run);
            }
            other => panic!("expected completions command, got {other:?}"),
        }
    }

    /// §FS-rhei-completions.2
    #[test]
    fn detects_current_shell_from_shell_var() {
        let detected = |value: &str| detect_current_shell(Some(OsStr::new(value)));

        assert_eq!(detected("/bin/bash"), Some(CompletionShell::Bash));
        assert_eq!(detected("/usr/bin/zsh"), Some(CompletionShell::Zsh));
        assert_eq!(detected("fish"), Some(CompletionShell::Fish));
        assert_eq!(detected("/usr/local/bin/pwsh"), Some(CompletionShell::PowerShell));
        assert_eq!(detected("powershell"), Some(CompletionShell::PowerShell));
        assert_eq!(detected("/usr/bin/elvish"), Some(CompletionShell::Elvish));
        assert_eq!(detected("/bin/tcsh"), None);
        assert_eq!(detected(""), None);
        assert_eq!(detect_current_shell(None), None);
    }

    #[test]
    fn parses_completions_install_options() {
        let cli = Cli::try_parse_from([
            "rhei",
            "completions",
            "bash",
            "--install",
            "--system",
            "--dry-run",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Completions { shell, install, system, dry_run, .. } => {
                assert_eq!(shell, Some(CompletionShell::Bash));
                assert!(install);
                assert!(system);
                assert!(dry_run);
            }
            other => panic!("expected completions command, got {other:?}"),
        }
    }

    #[test]
    fn root_help_lists_completions_command() {
        let mut command = Cli::command();
        let mut buffer = Vec::new();
        command.write_long_help(&mut buffer).expect("help should render");
        let help = String::from_utf8(buffer).expect("help should be UTF-8");

        assert!(help.contains("Setup:"));
        assert!(help.contains("completions"));
        assert!(help.contains("Generate shell completion scripts"));
    }

    #[test]
    fn render_rhei_json_smoke() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Smoke

## Tasks

### Task 1: Alpha
**State:** pending
"#,
        )
        .expect("parse should succeed");

        let rendered = render_rhei(
            &rhei,
            BTreeSet::new(),
            false,
            Vec::new(),
            RenderFormat::Json,
            true,
            false,
            false,
            false,
        )
        .expect("render ok");

        assert!(rendered.contains("\"title\": \"Smoke\""));
        assert!(rendered.contains("\"tasks\""));
    }

    #[test]
    fn compose_agent_prompt_carries_domain_instructions_only() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Prompt Smoke

## Tasks

### Task demo: Verify prompt wiring
**State:** review

Write findings and transition the task.
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: prompt-smoke
version: 1
states:
  review:
    description: review
    instructions: Write findings to `{output.review-notes.path}`.
    initial: true
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}.md
  fix:
    description: fix
    final: true
transitions:
  - from: review
    to: fix
"#,
        )
        .expect("machine should parse");
        let task = &rhei.tasks[0];
        let context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: Path::new("/tmp/workspace"),
            checkout_root: Path::new("/tmp/workspace"),
            plan_path: Path::new("/tmp/workspace"),
            state_machine_path: Some(Path::new("/tmp/workspace/states.yaml")),
            plan_title: &rhei.title,
            task,
            state_name: "review",
            current_state_raw: "review",
            machine: &machine,
            metadata: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        };

        let prompt = compose_agent_prompt(&context).expect("prompt");

        // New Rhei Commands section replaces Workflow Notes.
        assert!(prompt.contains("## Rhei Commands"));
        assert!(prompt.contains("rhei-managed plan at `/tmp/workspace`"));
        assert!(prompt.contains("The active state machine is `/tmp/workspace/states.yaml`."));
        assert!(prompt.contains(
            "The `rhei run` process that spawned you is responsible for advancing the task"
        ));
        assert!(prompt.contains("Available transitions from `review`:"));

        // Completion is a property of the execution model, not the prompt — no
        // completion prose should appear.
        assert!(!prompt.contains("then stop"));
        assert!(!prompt.contains("create every required output artifact"));
        assert!(!prompt.contains("produce every required output artifact"));
        assert!(!prompt.contains("for caller context"));
        assert!(!prompt.contains("Workflow Notes"));
    }

    #[test]
    fn runtime_templates_use_resolved_model_provider_and_name() {
        let rhei = rhei_core::parse(
            "# Rhei: Prompt Smoke\n\n## Tasks\n\n### Task demo: Verify\n**State:** review\n\nDo work.\n",
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: prompt-smoke
version: 1
states:
  review:
    description: review
    instructions: "{model} {model.provider} {model.name}"
  done:
    description: done
    final: true
transitions:
  - from: review
    to: done
"#,
        )
        .expect("machine should parse");
        let context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: Path::new("/tmp/workspace"),
            checkout_root: Path::new("/tmp/workspace"),
            plan_path: Path::new("/tmp/workspace"),
            state_machine_path: None,
            plan_title: &rhei.title,
            task: &rhei.tasks[0],
            state_name: "review",
            current_state_raw: "review",
            machine: &machine,
            metadata: None,
            target: None,
            model: Some("impl-fast"),
            model_provider: Some("anthropic"),
            model_name: Some("claude-sonnet-4-6"),
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        };

        let rendered =
            resolve_runtime_template_text(state_instructions(&machine, "review").as_str(), &context);
        assert_eq!(rendered, "impl-fast anthropic claude-sonnet-4-6");
    }

    #[test]
    fn parse_diagnostic_includes_line_info_when_available() {
        let input = "first line\nbad line\nthird line";
        let err = rhei_core::parser::ParseError {
            message: "unexpected token".to_string(),
            line: Some(2),
            file: None,
        };

        let rendered = render_parse_diagnostic(Path::new("broken.md"), input, &err);

        assert!(rendered.contains("-- PARSE ERROR"));
        assert!(rendered.contains("broken.md"));
        assert!(rendered.contains("2| bad line"));
        assert!(rendered.contains("unexpected token"));
    }

    #[test]
    fn validation_failure_formatting_aggregates_multiple_errors() {
        let rendered = format_validation_errors(&[
            "Task 1 is missing mandatory **State:** metadata".to_string(),
            "Task 2 depends on missing Task 9".to_string(),
        ]);

        assert!(rendered.contains("I found 2 problems:"));
        assert!(rendered.contains("1. Task 1 is missing mandatory **State:** metadata"));
        assert!(rendered.contains("2. Task 2 depends on missing Task 9"));
    }

    #[test]
    fn path_matches_normalizes_paths() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/states.yaml"),
        );

        assert!(path_matches(Path::new("./docs/markdown-plan-compiler.md"), &watched));
        assert!(path_matches(Path::new("docs/states.yaml"), &watched));
        assert!(!path_matches(Path::new("docs/plan-language-spec.md"), &watched));
    }

    #[test]
    fn panta_watch_excludes_runtime_at_any_depth() {
        // A real tree, because a watch target is canonicalized: a path that
        // does not exist has no canonical form, and comparing one that has a
        // canonical form against one that does not answers nothing.
        let dir = tempfile::tempdir().expect("tmpdir");
        let project = dir.path();
        let matching = ["index.panta.md", "auth.rhei.md", "billing/tasks/invoice.md",
                        "runtime-notes.rhei.md"];
        for relative in matching {
            let path = project.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
            fs::write(&path, "").expect("fixture file");
        }
        let targets = panta_watch_targets(project);

        // Manifest, single-file rheis, and workspace task files revalidate.
        assert!(path_matches(&project.join("index.panta.md"), &targets));
        assert!(path_matches(&project.join("auth.rhei.md"), &targets));
        assert!(path_matches(&project.join("billing/tasks/invoice.md"), &targets));

        // The project `runtime/` tree (where viz writes dashboard.html) never does...
        assert!(!path_matches(&project.join("runtime/dashboard.html"), &targets));
        // ...nor a per-rhei `runtime/` tree nested under a workspace rhei.
        assert!(!path_matches(&project.join("billing/runtime/results/billing.1.md"), &targets));

        // A similarly-named sibling that is not a `runtime` directory still matches.
        assert!(path_matches(&project.join("runtime-notes.rhei.md"), &targets));
    }

    /// A skill compiled into the binary but missing from the `--skills` default
    /// reaches only users who read the flag docs and type its name;
    /// `rhei-template-writer` shipped that way. §FS-rhei-install-skills.2
    #[test]
    fn default_skills_covers_every_builtin() {
        let command = cli_command();
        let install = command
            .get_subcommands()
            .find(|sub| sub.get_name() == "install-skills")
            .expect("install-skills subcommand");
        let arg = install
            .get_arguments()
            .find(|arg| arg.get_id() == "skills")
            .expect("--skills argument");

        let mut defaults: Vec<String> = arg
            .get_default_values()
            .iter()
            .flat_map(|value| {
                value
                    .to_string_lossy()
                    .split(',')
                    .map(ToOwned::to_owned)
                    .collect::<Vec<String>>()
            })
            .collect();
        defaults.sort();

        assert_eq!(
            defaults,
            builtin_skill_names(),
            "the --skills default in cli_declarations.rs and the skills embedded from \
             crates/rhei-cli/skills/ have drifted apart"
        );
    }

    /// Build a machine whose `implement` state writes a handoff at `path` and
    /// whose `review` state requires it.
    fn handoff_machine(
        models: &str,
        implement_state: &str,
        path: &str,
    ) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(&format!(
            r#"
name: handoff
version: 1
{models}
states:
  implement:
    description: implement
    initial: true
{implement_state}
    outputs:
      - name: implementation
        kind: handoff
        path: {path}
  review:
    description: review
    instructions: Review it.
    handoff:
      inherit:
        - from: transition.previous
          required: true
  done:
    description: done
    final: true
transitions:
  - from: implement
    to: review
  - from: review
    to: done
"#
        ))
        .expect("machine should parse")
    }

    fn handoff_plan() -> rhei_core::ast::Rhei {
        rhei_core::parse(
            r#"# Rhei: Handoff

## Tasks

### Task 1: Ship it
**State:** review
"#,
        )
        .expect("plan should parse")
    }

    fn handoff_context<'a>(
        workspace: &'a Path,
        rhei: &'a rhei_core::ast::Rhei,
        machine: &'a rhei_validator::StateMachine,
        model: Option<&'a str>,
    ) -> RuntimeTemplateContext<'a> {
        RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: workspace,
            checkout_root: workspace,
            plan_path: workspace,
            state_machine_path: None,
            plan_title: &rhei.title,
            task: &rhei.tasks[0],
            state_name: "review",
            current_state_raw: "review",
            machine,
            metadata: None,
            target: None,
            model,
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        }
    }

    fn record_transition(workspace: &Path, line: &str) {
        let runtime = workspace.join("runtime");
        std::fs::create_dir_all(&runtime).expect("mkdir runtime");
        std::fs::write(runtime.join("state-transitions.log"), format!("{line}\n"))
            .expect("write ledger");
    }

    /// A handoff path that templates the execution identity belongs to the
    /// state that *wrote* it. Resolving `{model}` with the successor's model
    /// looks for a file the producer never wrote. §FS-rhei-states.3.2
    #[test]
    fn state_handoff_resolves_under_the_source_states_model() {
        let rhei = handoff_plan();
        let machine = handoff_machine(
            "models:\n  - producer-model\n  - consumer-model",
            "    model: producer-model",
            "runtime/handoffs/{task_id}/{state}/{model}/impl.md",
        );

        let workspace = tempfile::tempdir().expect("tmpdir");
        record_transition(workspace.path(), "1 implement@review");
        let artifact =
            workspace.path().join("runtime/handoffs/1/implement/producer-model/impl.md");
        std::fs::create_dir_all(artifact.parent().expect("parent")).expect("mkdir");
        std::fs::write(&artifact, "Parser rewritten; tests green.\n").expect("write handoff");

        // The successor runs on a different model than the producer did.
        let context =
            handoff_context(workspace.path(), &rhei, &machine, Some("consumer-model"));
        let prompt = compose_agent_prompt(&context).expect("prompt");

        assert!(prompt.contains("## Handoff from implement"), "{prompt}");
        assert!(prompt.contains("Parser rewritten; tests green."), "{prompt}");
    }

    /// `outputs:` is an existence contract, so an agent can satisfy it with an
    /// empty file. A required handoff must not accept that as context.
    /// §FS-rhei-states.3.2
    #[test]
    fn required_state_handoff_rejects_an_empty_artifact() {
        let rhei = handoff_plan();
        let machine = handoff_machine("", "", "runtime/handoffs/{task_id}/{state}/impl.md");

        let workspace = tempfile::tempdir().expect("tmpdir");
        record_transition(workspace.path(), "1 implement@review");
        let artifact = workspace.path().join("runtime/handoffs/1/implement/impl.md");
        std::fs::create_dir_all(artifact.parent().expect("parent")).expect("mkdir");
        std::fs::write(&artifact, "   \n\n").expect("write empty handoff");

        let context = handoff_context(workspace.path(), &rhei, &machine, None);
        let err = compose_agent_prompt(&context).expect_err("empty handoff should fail");
        let message = format!("{err:?}");

        assert!(message.contains("no handoff artifact with content"), "{message}");
        assert!(message.contains("impl.md"), "error should name what it looked for: {message}");
    }

    /// `transition.previous` reads the central ledger, which is where state
    /// history lives. §FS-rhei-complete.3.1
    #[test]
    fn state_handoff_reads_the_source_state_from_the_transition_ledger() {
        let rhei = handoff_plan();
        let machine = handoff_machine("", "", "runtime/handoffs/{task_id}/{state}/impl.md");

        let workspace = tempfile::tempdir().expect("tmpdir");
        let artifact = workspace.path().join("runtime/handoffs/1/implement/impl.md");
        std::fs::create_dir_all(artifact.parent().expect("parent")).expect("mkdir");
        std::fs::write(&artifact, "notes\n").expect("write handoff");

        // No ledger entry yet: nothing records how the task reached `review`.
        let context = handoff_context(workspace.path(), &rhei, &machine, None);
        let err = compose_agent_prompt(&context).expect_err("no recorded transition");
        // Match a fragment short enough to survive miette's line wrapping.
        assert!(
            format!("{err:?}").contains("transition into this state was recorded"),
            "{err:?}"
        );

        // The last entry naming `review` as its destination picks the source.
        let runtime = workspace.path().join("runtime");
        std::fs::write(
            runtime.join("state-transitions.log"),
            "1 pending@implement\n1 implement@review\n",
        )
        .expect("write ledger");
        let prompt = compose_agent_prompt(&context).expect("prompt");
        assert!(prompt.contains("## Handoff from implement"), "{prompt}");
        assert!(prompt.contains("notes"), "{prompt}");
    }

    /// The `## Result` section appears only where it applies: a state with no
    /// declared edge into a `final: true` state cannot finish the ticket, so
    /// naming the result path there would be noise.
    // §FS-rhei-agents.3
    #[test]
    fn compose_agent_prompt_omits_the_result_section_when_the_state_cannot_finish() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Mid Flight

## Tasks

### Task 1: Implement
**State:** implement
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: mid-flight
version: 1
states:
  implement:
    description: implement
    instructions: Implement it.
    initial: true
  review:
    description: review
  done:
    description: done
    final: true
transitions:
  - from: implement
    to: review
  - from: review
    to: done
"#,
        )
        .expect("machine should parse");

        let workspace = tempfile::tempdir().expect("tmpdir");
        let task = &rhei.tasks[0];
        let context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: workspace.path(),
            checkout_root: workspace.path(),
            plan_path: workspace.path(),
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name: "implement",
            current_state_raw: "implement",
            machine: &machine,
            metadata: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        };

        let prompt = compose_agent_prompt(&context).expect("prompt");
        assert!(!prompt.contains("## Result"), "{prompt}");
    }

    /// A wildcard escape hatch does not make a state one that can finish the
    /// ticket. Nearly every machine declares `* -> cancelled`, so counting it
    /// put the `## Result` section on the first state of every workflow — and an
    /// agent that writes a result three states early pre-satisfies the
    /// obligation at the real terminal edge with a stale message. The gate
    /// surfaces filter wildcards out of a gate's choices for the same reason.
    // §FS-rhei-agents.3 §FS-rhei-states.3.3
    #[test]
    fn compose_agent_prompt_omits_the_result_section_for_a_wildcard_terminal_edge() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Wildcard Escape

## Tasks

### Task 1: Implement
**State:** implement
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: wildcard-escape
version: 1
states:
  implement:
    description: implement
    instructions: Implement it.
    initial: true
  review:
    description: review
  done:
    description: done
    final: true
  cancelled:
    description: cancelled
    final: true
transitions:
  - from: implement
    to: review
  - from: review
    to: done
  - from: "*"
    to: cancelled
"#,
        )
        .expect("machine should parse");

        let workspace = tempfile::tempdir().expect("tmpdir");
        let task = &rhei.tasks[0];
        let mut context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: workspace.path(),
            checkout_root: workspace.path(),
            plan_path: workspace.path(),
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name: "implement",
            current_state_raw: "implement",
            machine: &machine,
            metadata: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        };

        let prompt = compose_agent_prompt(&context).expect("prompt");
        assert!(!prompt.contains("## Result"), "{prompt}");

        // The state that really can finish it still gets the section.
        context.state_name = "review";
        context.current_state_raw = "review";
        let prompt = compose_agent_prompt(&context).expect("prompt");
        assert!(prompt.contains("## Result"), "{prompt}");
        assert!(prompt.contains("`runtime/results/1.md`"), "{prompt}");
    }

    /// A fanned-out invocation is shown its own fragment: one shared path would
    /// let the last writer erase its siblings. §FS-rhei-agents.3 §FS-rhei-states.3.3
    #[test]
    fn compose_agent_prompt_names_the_per_invocation_result_fragment_under_fanout() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Fanout Prompt

## Tasks

### Task 1: Review
**State:** review
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: fanout-prompt
version: 1
models:
  - alpha
  - beta
states:
  review:
    description: review
    instructions: Review it.
    initial: true
    all_models:
      - alpha
      - beta
  done:
    description: done
    final: true
transitions:
  - from: review
    to: done
"#,
        )
        .expect("machine should parse");

        let workspace = tempfile::tempdir().expect("tmpdir");
        let task = &rhei.tasks[0];
        let context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: workspace.path(),
            checkout_root: workspace.path(),
            plan_path: workspace.path(),
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name: "review",
            current_state_raw: "review",
            machine: &machine,
            metadata: None,
            target: None,
            model: Some("alpha"),
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        };

        let prompt = compose_agent_prompt(&context).expect("prompt");
        // State and visit key the fragment too, so a later fanned-out state
        // cannot be answered with this one's account. §FS-rhei-states.3.3
        assert!(prompt.contains("`runtime/results/1/review/1/alpha.md`"), "{prompt}");
        assert!(!prompt.contains("`runtime/results/1.md`"), "{prompt}");
    }

    /// An export a prior task never wrote is skipped, not raised: enforcement
    /// belongs to a validator, and a missing file must not block the spawn.
    #[test]
    fn compose_agent_prompt_skips_an_unwritten_export() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Exports

## Tasks

### Task 1: Design the API
**State:** review
**Provides:** api-contract

### Task 2: Implement the client
**State:** review
**Prior:** Task 1
**Consumes:** 1:api-contract
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: exports
version: 1
states:
  review:
    description: review
    instructions: Implement it.
    initial: true
  done:
    description: done
    final: true
transitions:
  - from: review
    to: done
"#,
        )
        .expect("machine should parse");

        let workspace = tempfile::tempdir().expect("tmpdir");
        let task = &rhei.tasks[1];
        let context = RuntimeTemplateContext {
            task_roots: None,
            plan_tasks: None,
            workspace_root: workspace.path(),
            checkout_root: workspace.path(),
            plan_path: workspace.path(),
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name: "review",
            current_state_raw: "review",
            machine: &machine,
            metadata: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
            memory: None,
        };

        let prompt = compose_agent_prompt(&context).expect("prompt");

        assert!(!prompt.contains("## Consumed Exports"), "{prompt}");
    }
