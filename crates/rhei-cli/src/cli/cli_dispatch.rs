/// Snapshot cache maintenance commands.
#[derive(Subcommand, Debug)]
enum SnapshotCommand {
    /// List cached snapshot generations
    List {
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Filter by task id
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Filter by snapshot name; use _state for auto-emitted snapshots
        #[arg(long, value_name = "SNAPSHOT")]
        name: Option<String>,
        /// Filter by emitting state
        #[arg(long, value_name = "STATE", add = ArgValueCompleter::new(complete_state_name))]
        state: Option<String>,
        /// Filter by emission origin
        #[arg(long, value_enum, default_value = "orchestrator")]
        produced_by: SnapshotProducedByFilter,
        /// Show only snapshots that no longer resolve in the current plan/state machine
        #[arg(long)]
        orphaned: bool,
        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: SnapshotListFormat,
    },
    /// Show one snapshot manifest and transcript preview
    Show {
        /// Snapshot reference
        #[arg(value_name = "REF")]
        reference: String,
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
    },
    /// Delete cached snapshot generations by policy
    Gc {
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Filter by task id
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Filter by snapshot name
        #[arg(long, value_name = "SNAPSHOT")]
        name: Option<String>,
        /// Delete only generations older than this duration (for example 7d or 4h)
        #[arg(long, value_name = "DURATION")]
        older_than: Option<String>,
        /// Keep the newest N generations per snapshot identity
        #[arg(long, value_name = "N")]
        keep_generations: Option<usize>,
        /// Include operator-produced generations in retention and deletion decisions
        #[arg(long)]
        include_operator: bool,
        /// Delete only snapshots that no longer resolve in the current plan/state machine
        #[arg(long)]
        orphaned: bool,
        /// Print what would be deleted without removing files
        #[arg(long)]
        dry_run: bool,
        /// Bypass the live-run interlock
        #[arg(long)]
        force: bool,
    },
    /// Continue interactively from a cached snapshot
    Continue {
        /// Snapshot reference
        #[arg(value_name = "REF")]
        reference: String,
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Select a target slug when the reference is ambiguous
        #[arg(long, value_name = "SLUG")]
        target: Option<String>,
        /// Continue from a specific generation
        #[arg(long, value_name = "N")]
        generation: Option<u64>,
        /// Do not capture the resulting operator transcript
        #[arg(long)]
        no_capture: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SnapshotProducedByFilter {
    Orchestrator,
    Operator,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SnapshotListFormat {
    Text,
    Json,
}

/// Output formats supported by the [`Render`](Commands::Render) subcommand.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum RenderFormat {
    Json,
    Github,
    Progress,
}

/// Supported AI coding agents for skill installation.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Agent {
    ClaudeCode,
    Cursor,
    Windsurf,
    Copilot,
    Kilocode,
    Pi,
    Codex,
    Antigravity,
    All,
}

/// Shells supported by the completion generator.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
    Elvish,
}

impl CompletionShell {
    fn as_str(self) -> &'static str {
        match self {
            CompletionShell::Bash => "bash",
            CompletionShell::Zsh => "zsh",
            CompletionShell::Fish => "fish",
            CompletionShell::PowerShell => "powershell",
            CompletionShell::Elvish => "elvish",
        }
    }
}

include!("cli_startup.rs");

/// Dispatch the parsed CLI command.
fn dispatch(cli: Cli) -> MietteResult<()> {
    // `--state-machine` is accepted both before the subcommand and on the
    // subcommands that read one; the subcommand copy wins when both appear.
    let before_subcommand = cli.state_machine;
    match cli.command {
        Commands::Init { dir, here, title, no_agents, force } => {
            init_command(dir.as_deref(), title.as_deref(), no_agents, force, here)
        }
        Commands::New { options } => new_command(&options),
        command @ Commands::Migrate { .. } => dispatch_target_command(command, before_subcommand),
        command @ Commands::Validate { .. } => dispatch_target_command(command, before_subcommand),
        Commands::Render { input, format, pretty, no_color, no_metadata, no_content, state_machine } => {
            let target = resolve_plan_target(input)?;
            render_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                format,
                pretty,
                no_color,
                no_metadata,
                no_content,
            )
        }
        Commands::States { input, rhei, json, state_machine } => {
            states_command(input, state_machine.or(before_subcommand).as_deref(), &rhei, json)
        }
        Commands::Roster { input, json } => roster_command(input, json),
        Commands::List {
            input,
            rhei,
            state,
            assignee,
            no_assignee,
            kind,
            has_prior,
            parent,
            root,
            contains,
            terminal,
            non_terminal,
            ready,
            blocked,
            limit,
            json,
            state_machine,
        } => {
            let target = resolve_plan_target(input)?;
            let rhei = target.scope_with(&rhei);
            list_command(
            target.path(),
            state_machine.or(before_subcommand).as_deref(),
            ListFilters {
                rhei,
                states: state,
                assignee,
                no_assignee,
                kind,
                has_prior,
                parent,
                root,
                contains,
                terminal,
                non_terminal,
                ready,
                blocked,
                limit,
            },
            json,
            )
        }
        Commands::Recover { execution_root } => recover_command(&execution_root),
        Commands::Transition {
            force,
            reason,
            input,
            task,
            from,
            to,
            result,
            supervisor,
            no_callbacks,
            state_machine,
        } => {
            let (input, task) = split_transition_ticket_target(input, task)?;
            let target = resolve_plan_target(input)?;
            if force {
                validate_force_options(reason.as_deref(), supervisor.as_deref(), no_callbacks)?;
                return force_transition_command(target.path(), &target.scope_with(&[]),
                    state_machine.or(before_subcommand).as_deref(), &task, &from, &to,
                    reason.as_deref().expect("reason validated"), result.as_deref());
            }
            if reason.is_some() {
                return Err(miette!("--reason is only valid with --force"));
            }
            transition_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                &task,
                &from,
                &to,
                result.as_deref(),
                supervisor.as_deref(),
                no_callbacks,
            )
        }
        command @ Commands::Run { .. } => dispatch_target_command(command, before_subcommand),
        // A member loads through its project like every other command, and the
        // rhei it named narrows which accounting roots are read rather than
        // being dropped here. §FS-rhei-panta.6.5
        Commands::Cost { input, rhei, task, json, by, run, since, until } => {
            let target = resolve_plan_target(input)?;
            let scope = target.scope_with(&rhei);
            cost_command(CostCommandOptions {
                input: target.path(),
                scope: &scope,
                task: task.as_deref(),
                json,
                by,
                run: run.as_deref(),
                since: since.as_deref(),
                until: until.as_deref(),
            })
        }
        Commands::Schema { name, list: _ } => accounting_schema_command(name.as_deref()),
        // `rhei summary` resolves its positional and its `--rhei` exactly as
        // `rhei cost` does, so the two narrow together. §FS-rhei-summary.1
        Commands::Summary { options } => {
            let target = resolve_plan_target(options.input.clone())?;
            let scope = target.scope_with(&options.rhei);
            summary_command(target.path(), &scope, before_subcommand.as_deref(), &options)
        }
        Commands::Report { input, task, state, full } => {
            report_command(&input, task.as_deref(), state.as_deref(), full)
        }
        Commands::Attach { run, json, since, wait } => {
            attach_command(run.as_deref(), json, since, wait)
        }
        Commands::Runs { json, all, since, until } => {
            runs_command(json, all, since.as_deref(), until.as_deref())
        }
        Commands::Stop { run, kill, wait } => stop_command(run.as_deref(), kill, wait),
        Commands::Intervene { plan, task, slot, message } => {
            intervene_command(&plan, &task, slot, &message)
        }
        Commands::Viz { input, output, open, state_machine } => {
            let target = resolve_plan_target(input)?;
            viz_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                output.as_deref(),
                open,
            )
        }
        Commands::Snapshot { command, state_machine } => snapshot_command(command, state_machine.or(before_subcommand).as_deref()),
        Commands::Templates { template, json, source } => {
            templates::templates_command(json, &source, template.as_deref())
        }
        Commands::Instantiate {
            template,
            mount,
            set_values,
            set_files,
            values,
            seam,
            pass,
            output,
            execute,
            dry_run,
            keep_on_error,
            list_inputs,
            input_args,
        } => templates::instantiate_command(
            template.as_deref(),
            &mount,
            &input_args,
            &instantiate_execute_args_from_env(),
            &set_values,
            &set_files,
            &values,
            &seam,
            &pass,
            output.as_deref(),
            execute,
            dry_run,
            keep_on_error,
            list_inputs,
        ),
        Commands::Next { input, task, json, no_callbacks, peek, rhei, state_machine } => {
            let target = resolve_plan_target(input)?;
            next_command(
                target.path(),
                state_machine.or(before_subcommand).as_deref(),
                task.as_deref(),
                json,
                no_callbacks,
                peek,
                &target.scope_with(&rhei),
            )
        }
        Commands::Complete {
            input,
            task,
            result,
            result_file,
            no_callbacks,
            state_machine,
        } => {
            // Result source failures take precedence over target and plan
            // resolution and cannot mutate either. §FS-rhei-complete.4
            let result = resolve_complete_result(result, result_file.as_deref())?;
            let (input, task) = split_complete_ticket_target(input, task)?;
            let target = resolve_plan_target(input)?;
            complete_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                &task,
                &result,
                no_callbacks,
            )
        }
        Commands::Release { input, task, all, rhei, dry_run, state_machine } => {
            let (input, task) = split_ticket_target(input, task)?;
            let target = resolve_plan_target(input)?;
            release_command(
                target.path(),
                state_machine.or(before_subcommand).as_deref(),
                task.as_deref(),
                all,
                &target.scope_with(&rhei),
                dry_run,
            )
        }
        Commands::Reset { input, rhei, dry_run, yes, state_machine } => {
            // §FS-rhei-panta.6: reset destroys runtime state, so it is the one
            // plan-taking command that never infers an omitted target.
            let Some(input) = input else {
                return Err(miette!(
help = "preview it first: rhei reset <plan-or-project> --dry-run",

                    "`rhei reset` rewrites in-scope ticket states and deletes runtime \
                     artifacts, so it never infers its target. Name the plan or project \
                     explicitly: `rhei reset <plan-or-project>`"
                ));
            };
            // Reset never *infers* a target, but an explicit member rhei still
            // loads through its project and narrows to itself. §FS-rhei-panta.6
            let target = resolve_plan_target(Some(input))?;
            reset_command(
                target.path(),
                state_machine.or(before_subcommand).as_deref(),
                &target.scope_with(&rhei),
                dry_run,
                yes,
            )
        }
        Commands::Version => {
            print_versions();
            Ok(())
        }
        Commands::InstallSkills { agent, local, link, uninstall, dry_run, skills } => {
            install_skills_command(agent, local, link, uninstall, dry_run, &skills)
        }
        Commands::Completions { shell, install, user: _, system, output, dry_run } => {
            completions_command(shell, install, system, output.as_deref(), dry_run)
        }
    }
}
