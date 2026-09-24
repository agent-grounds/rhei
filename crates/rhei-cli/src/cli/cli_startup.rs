// Diagnostic setup and process entry before command dispatch. §AR-source-file-size.3

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
    let _ = miette::set_hook(Box::new(|diagnostic| {
        let help = diagnostic.help().map(|help| help.to_string()).unwrap_or_default();
        // A recovery command is a copyable unit even when it appears inside a
        // headless launcher's relayed diagnostic. §FS-rhei-migrate.5 §FS-rhei-errors.1.2
        let keeps_migration_action_whole = help.contains("rhei migrate export-priors ")
            || diagnostic.to_string().contains("rhei migrate export-priors ");
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .wrap_lines(!keeps_migration_action_whole)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .build(),
        )
    }));
}

/// Whether a parse failure is the one refusal that has a better answer than
/// "these two cannot be used together": `--until-idle` beside an explicit
/// `--tui`.
///
/// Matched on the rendered refusal rather than on the argument ids, because
/// clap exposes the pair it refused only in the message. `--headless` is
/// excluded by name: that pair is refused for a different reason and `--no-tui`
/// is no answer to it.
// §FS-rhei-run-tui.1.4
fn refuses_until_idle_beside_tui(err: &clap::Error) -> bool {
    if err.kind() != ErrorKind::ArgumentConflict {
        return false;
    }
    let rendered = err.render().to_string();
    rendered.contains("--until-idle") && !rendered.contains("--headless")
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

/// The run did the available work and returned because everything left is
/// deliberately waiting. Produced only when `--until-idle` is selected, and
/// allocated once here beside the other statuses this CLI owns: nothing else
/// in `3..=99` is allocated anywhere in it, and this is the one thing about
/// the option that cannot be changed after release without breaking every
/// caller keyed on it.
// §FS-rhei-run-json.5
const EXIT_IDLE: i32 = 3;

/// Whether the run that just finished returned idle.
///
/// The exit code is knowable only on the way out — after every guard has run
/// and the report is written — and a run that returned idle returns `Ok`,
/// because nothing about it failed. So it is recorded here and read on the
/// exit path, the same shape the interruption status already uses.
// §FS-rhei-run.3 §FS-rhei-run-json.5
static IDLE_RETURN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn request_idle_exit() {
    IDLE_RETURN.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn idle_exit_code() -> Option<i32> {
    IDLE_RETURN.load(std::sync::atomic::Ordering::SeqCst).then_some(EXIT_IDLE)
}

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
        // Declared on the flags, so it fires before any lock, descriptor,
        // journal or event log exists. What clap cannot say is the alternative
        // that works. §FS-rhei-run-tui.1.4
        Err(err) if refuses_until_idle_beside_tui(&err) => {
            let _ = err.print();
            eprintln!(
                "  tip: --until-idle implies line output; pass --no-tui, or drop --tui and let \
                 it choose."
            );
            std::process::exit(err.exit_code());
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
    // Below the signal, because an interrupt outranks every other outcome, and
    // above the plain success it is a split of. §FS-rhei-run.3
    if let Some(code) = idle_exit_code() {
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
