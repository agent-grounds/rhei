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

/// Program entry point.
///
/// Delegates to fallible command logic so tests can exercise it directly.
/// Wrap diagnostics at word boundaries but never *inside* a word.
///
/// miette's defaults offer a break opportunity at every hyphen and every `/`,
/// and split an overlong token outright. All three land mid-path on the
/// filesystem diagnostics this CLI prints constantly, and a path broken across
/// lines cannot be copied, clicked, or grepped. Treating only spaces as break
/// points keeps prose wrapping while a long path overflows the wrap column
/// intact, where the terminal soft-wraps it.
fn install_diagnostic_handler() {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .build(),
        )
    }));
}

/// True when `rhei` was invoked with no arguments at all.
///
/// Distinguishes the orientation case from a subcommand-level usage error;
/// see the call site in [`main`].
fn is_bare_invocation() -> bool {
    std::env::args_os().count() <= 1
}

/// Conventional status for a process that stopped because a pipe consumer
/// closed early: `128 + SIGPIPE`, the same value the shell reports for
/// `yes | head`.
const EXIT_BROKEN_PIPE: i32 = 141;

/// Leave quietly when there is no longer anywhere to print, instead of
/// surfacing an internal error.
///
/// Rust ignores `SIGPIPE` before `main`, so a closed stdout comes back as an
/// `EPIPE` write error and `println!` panics on it — `rhei list | head` exited
/// 101 with a stack trace. This intercepts exactly that panic and exits the way
/// a Unix filter killed by the signal does.
///
/// A terminal that goes away is the same situation with a different errno: a
/// `rhei run` whose window is closed writes `EIO` to the dead pty from then on,
/// and the end-of-run console summary panicked on it — then panicked *again*
/// from the report guard's own `println!` while unwinding, which is a double
/// panic and aborts. A run that ended is not a run that crashed.
///
/// Restoring `SIGPIPE` to `SIG_DFL` process-wide would be the shorter fix and
/// is the wrong one: this CLI writes to pipes it owns — a callback
/// subprocess's stdin, an agent's — and there the write returning `EPIPE` is
/// how a child that exited early gets *reported*. Under `SIG_DFL` those writes
/// killed `rhei` mid-diagnostic instead, so a transition that should have
/// failed with an explanation failed with empty stderr.
// §FS-rhei-usage.2 §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
fn install_quiet_broken_pipe_exit() {
    // Before any output can be lost, because the question cannot be asked
    // afterwards. §FS-rhei-run.3.2
    record_startup_terminals();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if is_lost_output_panic(info) {
            // `exit` runs no destructor: the shutdown guard never gets its
            // turn, so the hook is the last code that can end the groups.
            // §FS-rhei-run.3.2
            terminate_all_live_groups();
            // An interrupted run still names its signal: losing the terminal is
            // how the interruption arrived, not a second outcome.
            // §FS-rhei-run.3.2
            let code = interrupt_exit_code().unwrap_or(EXIT_BROKEN_PIPE);
            // The run really is ending, so its registry entry must go with it —
            // otherwise a reader that lost its pipe leaves a run listed as live
            // forever. §FS-rhei-run-headless.2
            finalize_run_descriptor(code);
            std::process::exit(code);
        }
        previous(info);
    }));
}

/// `EPIPE`, by `strerror`'s message and by errno. Both forms because the
/// message follows the locale, while the `(os error N)` suffix the standard
/// library appends does not — either one identifies the errno on its own.
const BROKEN_PIPE_MARKERS: [&str; 2] = ["Broken pipe", "(os error 32)"];

/// `EIO`, in the same two forms.
const IO_ERROR_MARKERS: [&str; 2] = ["Input/output error", "(os error 5)"];

/// Which of the process's own output streams a print failed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LostStream {
    Stdout,
    Stderr,
}

/// The stream a standard-library "failed printing to …" panic names, or `None`
/// for a panic that is about something else entirely.
fn printing_failure_stream(message: &str) -> Option<LostStream> {
    let rest = message.strip_prefix("failed printing to ")?;
    if rest.starts_with("stdout") {
        Some(LostStream::Stdout)
    } else if rest.starts_with("stderr") {
        Some(LostStream::Stderr)
    } else {
        None
    }
}

/// Whether stdout and stderr were terminals when the process started, asked
/// once and remembered.
///
/// It has to be once, and it has to be then. `isatty` on a pty whose master
/// has closed does not answer "yes, a terminal that has gone away" — the
/// hangup swaps the slave's file operations out and the `TCGETS` behind
/// `isatty` fails with `EIO` like every other ioctl on it, so the stream reads
/// as *not a terminal* from exactly the moment the guard below needs it to
/// read as one. Asked at startup the answer is the true one, and it cannot
/// change afterwards: a redirected stdout does not become a terminal, and a
/// terminal that goes away was still a terminal.
// §FS-rhei-run.3.2: a lost console ends the run quietly.
static STARTUP_TERMINALS: std::sync::OnceLock<(bool, bool)> = std::sync::OnceLock::new();

/// Ask the question while both streams are still whatever they are.
fn record_startup_terminals() -> (bool, bool) {
    *STARTUP_TERMINALS.get_or_init(|| {
        use std::io::IsTerminal as _;
        (std::io::stdout().is_terminal(), std::io::stderr().is_terminal())
    })
}

fn stream_is_terminal(stream: LostStream) -> bool {
    // `get_or_init` and not `get`: a panic on a path that never installed the
    // hook — a unit test, a library caller — still gets a real answer rather
    // than a default that silently changes the verdict.
    let (stdout, stderr) = record_startup_terminals();
    match stream {
        LostStream::Stdout => stdout,
        LostStream::Stderr => stderr,
    }
}

/// Whether a panic is the standard library's "failed printing to stdout" panic
/// for an output that no longer exists, rather than a real bug.
///
/// Matched on the payload text because that is all the standard library
/// exposes: the panic carries no typed error. The message must be that panic,
/// so one that merely mentions a broken pipe in some other context still
/// reports normally.
fn is_lost_output_panic(info: &std::panic::PanicHookInfo<'_>) -> bool {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    message.is_some_and(message_is_lost_output)
}

fn message_is_lost_output(message: &str) -> bool {
    lost_output_verdict(message, stream_is_terminal)
}

/// The decision itself, over the panic message and a way to ask whether the
/// stream it names is a terminal, so it can be tested — a `PanicHookInfo` is
/// not constructible outside a real panic, and a test cannot close the
/// harness's own stdout.
///
/// `EPIPE` always means the reader is gone. `EIO` means it only on a terminal,
/// where it is how a closed pty reports the session hanging up; on a redirected
/// stdout it is a real write failure — a full device, a dropped network mount —
/// and treating that as "the output is gone" would kill every in-flight agent
/// and exit `141` without a word about what actually went wrong.
///
/// `is_terminal` therefore answers for the stream as it was at startup, never
/// as it is now: see [`STARTUP_TERMINALS`] for why asking now inverts the
/// answer in the one case this exists for.
fn lost_output_verdict(message: &str, is_terminal: impl Fn(LostStream) -> bool) -> bool {
    let Some(stream) = printing_failure_stream(message) else {
        return false;
    };
    if BROKEN_PIPE_MARKERS.iter().any(|marker| message.contains(marker)) {
        return true;
    }
    IO_ERROR_MARKERS.iter().any(|marker| message.contains(marker)) && is_terminal(stream)
}

/// The stack the CLI gives itself, in bytes.
///
/// Windows reserves 1 MiB for a process's main thread where Linux and macOS
/// give 8, and this CLI needs more than 1: clap's command tree is built on the
/// stack and the plan parser descends recursively, so on Windows *every*
/// invocation overflowed — `rhei` with no arguments at all included, before it
/// had read a plan or a flag.
///
/// A thread asks for its stack in code, so this travels with the binary. A
/// linker flag in this repository's `.cargo/config.toml` would not: it is not
/// read when somebody runs `cargo install rhei-cli`, which is how the binary
/// this fixes actually reaches a Windows machine.
// §FS-rhei-distribution.1
const CLI_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Entry point for both installed binaries: run the CLI on a stack of a size
/// this program chose, rather than on whatever the platform handed `main`.
// §FS-rhei-distribution.1
pub fn run() {
    let spawned = std::thread::Builder::new()
        .name("rhei".to_string())
        .stack_size(CLI_STACK_BYTES)
        .spawn(run_on_cli_stack);
    match spawned {
        // A panic has already been reported by the hook that ran inside the
        // thread; joining only turns it back into the exit code the same panic
        // on the main thread would have produced.
        Ok(worker) => {
            if worker.join().is_err() {
                std::process::exit(101);
            }
        }
        // No thread to be had — a process at its limit, a sandbox that refuses
        // one. Run here on the platform's stack rather than not running.
        Err(_) => run_on_cli_stack(),
    }
}

fn run_on_cli_stack() {
    install_quiet_broken_pipe_exit();
    install_diagnostic_handler();
    // `clap_complete` removes `COMPLETE` from the environment before running
    // the completer, so what kind of run this is has to be recorded here or it
    // cannot be known later. §FS-rhei-templates.1.3

    // Empty and `0` are `clap_complete`'s own way of turning dynamic completion
    // off, so a run carrying either is an ordinary one. §FS-rhei-templates.1.3
    if std::env::var_os("COMPLETE")
        .is_some_and(|shell| !matches!(shell.to_string_lossy().as_ref(), "" | "0"))
    {
        mark_serving_shell_completion();
    }
    CompleteEnv::with_factory(cli_command).bin(invoked_bin_name()).complete();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // A bare `rhei` is a request for orientation, so answer it with the
        // root help on stdout and a success exit. Every *other* missing
        // subcommand — `rhei snapshot`, say — is a usage error about that
        // subcommand: let clap render its own contextual help to stderr with
        // the conventional exit code, or a script cannot tell the two apart.
        Err(err)
            if is_bare_invocation()
                && matches!(
                    err.kind(),
                    ErrorKind::MissingSubcommand
                        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                ) =>
        {
            let mut cmd = cli_command();
            if let Err(io_err) = cmd.print_help() {
                eprintln!("failed to write CLI help: {io_err}");
                std::process::exit(1);
            }
            println!();
            return;
        }
        Err(err) => err.exit(),
    };

    let json_mode = command_wants_json(&cli.command);

    if let Err(err) = dispatch(cli) {
        if json_mode {
            emit_json_error(&err);
        } else {
            eprintln!("{err:?}");
        }
        // A run that a signal ended reports the signal, not a generic failure:
        // whatever error it surfaced on the way out is a consequence of the
        // interruption. §FS-rhei-run.3.2
        let code = interrupt_exit_code().unwrap_or(1);
        finalize_run_descriptor(code);
        std::process::exit(code);
    }
    // Checked after `dispatch` so every guard has run and the report is
    // written: `128 + signal` is what a shell reports for a process the signal
    // killed, and `rhei run` was asked to stop by one. §FS-rhei-run.3.2
    if let Some(code) = interrupt_exit_code() {
        finalize_run_descriptor(code);
        std::process::exit(code);
    }
    // The exit code is only knowable here, which is why the descriptor's
    // terminal status is stamped from the exit path rather than from a guard
    // that cannot see it. §FS-rhei-run-headless.2
    finalize_run_descriptor(0);
}

/// Returns true when the invoked command's output format is JSON. In that
/// case, errors are rendered as a single-line JSON object on stderr instead
/// of the default miette text, so machine consumers don't have to parse two
/// shapes.
fn command_wants_json(command: &Commands) -> bool {
    match command {
        Commands::Next { json, .. } => *json,
        Commands::States { json, .. } => *json,
        Commands::Roster { json, .. } => *json,
        Commands::List { json, .. } => *json,
        Commands::Snapshot { command: SnapshotCommand::List { format, .. }, .. } => {
            matches!(format, SnapshotListFormat::Json)
        }
        Commands::Templates { json, .. } => *json,
        Commands::Cost { json, .. } => *json,
        Commands::Runs { json, .. } => *json,
        // `attach --json` streams records on stdout, so a failure must not
        // print miette prose beside them. §FS-rhei-run-json.1
        Commands::Attach { json, .. } => *json,
        Commands::Run { standalone, .. } => standalone.json,
        Commands::Render { format, .. } => matches!(format, RenderFormat::Json),
        _ => false,
    }
}

fn emit_json_error(err: &miette::Report) {
    // §FS-rhei-errors.5: machine consumers get the same next action as humans.
    let mut error = serde_json::json!({ "message": err.to_string() });
    if let Some(help) = err.help() {
        error["help"] = serde_json::Value::String(help.to_string());
    }
    let payload = serde_json::json!({ "error": error });
    let serialized = serde_json::to_string(&payload)
        .unwrap_or_else(|_| format!("{{\"error\":{{\"message\":{:?}}}}}", err.to_string()));
    eprintln!("{serialized}");
}

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
        Commands::Transition {
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
