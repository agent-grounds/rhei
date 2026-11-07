// The detached child of `rhei run --headless`: how it is started, what it is told,
// and what its console is. The launcher beside it decides whether to start one and
// reports what became of it.

// §AR-source-file-size.3 §FS-rhei-run-headless.1 §DA-detached-runs

/// Flags the launcher consumes rather than forwarding. `--headless` would make
/// the child detach again; `--json` and its companion describe *the launcher's*
/// output, and a JSON stream into a log file nobody reads is not what the
/// caller asked for — the detached run's machine-readable form is
/// `runtime/events.jsonl`.
// §FS-rhei-run-headless.1
const LAUNCHER_ONLY_FLAGS: [&str; 3] = ["--headless", "--json", "--json-agent-output"];

/// Open the console log the detached child writes on: emptied for the run that
/// is starting, and append-only from then on.
///
/// Append mode is not a detail. The launch lock covers launchers, not a *live*
/// run's writes, so where the pre-check is blind — two launches on member plans
/// of one root — this call empties a log a running child still holds open. That
/// child's next write then goes to the offset it had before, landing inside
/// whatever the new child wrote and cutting a rendered diagnostic mid-token.
/// `O_APPEND` sends every write to the end instead: the two consoles interleave
/// by lines, and no byte is ever overwritten.
///
/// `set_len` and not `OpenOptions::truncate`, which std rejects outright when
/// it is combined with `append`. The stderr `try_clone` shares this open file
/// description, so both of the child's streams inherit the append flag.
// §FS-rhei-run-headless.8
fn open_console_log(log_path: &Path) -> std::io::Result<fs::File> {
    let log = fs::OpenOptions::new().create(true).append(true).open(log_path)?;
    log.set_len(0)?;
    Ok(log)
}

/// Re-execute this binary's `rhei run` invocation, minus the launcher-only
/// flags, in a new session with its console redirected.
fn spawn_detached_run(log_path: &Path, options: &RunOptions) -> MietteResult<std::process::Child> {
    if !cfg!(unix) {
        return Err(miette!(
            help = format!("run it in the foreground instead: {}", help_command_with("run", "--no-tui")),
            "`--headless` needs a POSIX session to detach into and is not supported on this \
             platform yet"
        ));
    }

    let exe = std::env::current_exe().map_err(|err| {
        miette!(
            help = "a detached run re-executes this binary, so it must still be on disk",
            "could not locate the rhei binary to re-execute: {err}"
        )
    })?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create the runtime directory", err))?;
    }
    // Emptied for this run and appended to from then on, so a launcher that
    // empties it under a live run cannot cut that run's bytes.
    // §FS-rhei-run-headless.8
    let log = open_console_log(log_path)
        .map_err(|err| file_io_report(log_path, "failed to open the run console log", err))?;
    let stderr = log
        .try_clone()
        .map_err(|err| file_io_report(log_path, "failed to open the run console log", err))?;

    let mut command = std::process::Command::new(exe);
    command.args(child_arguments());
    command.env(HEADLESS_CHILD_ENV, headless_child_marker(options));
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::from(log));
    command.stderr(std::process::Stdio::from(stderr));
    detach_session(&mut command);

    command.spawn().map_err(|err| {
        miette!(
            help = "check that the rhei binary is still on disk and executable",
            "could not start the detached run: {err}"
        )
    })
}

/// Preserve launcher-only flags for diagnostics rendered by the detached
/// child, whose executable arguments intentionally omit them.
fn headless_child_marker(options: &RunOptions) -> String {
    let mut flags = vec!["--headless"];
    if options.standalone.json {
        flags.push("--json");
    }
    if options.standalone.json_agent_output {
        flags.push("--json-agent-output");
    }
    flags.join(",")
}

fn headless_launcher_flag_was_supplied(flag: &str) -> bool {
    // §REQ-test-isolation.7: the marker is read through the one reader of rhei's own variables.
    inherited_rhei_var(HEADLESS_CHILD_ENV)
        .is_some_and(|marker| marker.split(',').any(|candidate| candidate == flag))
}

/// Put the child in its own session, so the launching terminal's `SIGHUP`
/// cannot reach it.
/// `setsid` and not `Command::process_group`: the latter makes the child a
/// process-group leader, and `setsid` then fails with `EPERM`. `setsid` alone
/// creates both a new session and a new process group, which is what
/// detachment needs.
/// The child deliberately does **not** arm a parent-death signal. That backstop
/// belongs to supervised *work*; armed on the
/// supervisor it would kill every detached run the moment its launcher
/// returned.
// §FS-rhei-run-headless.1 §DA-supervised-process-groups §DA-detached-runs
#[cfg(unix)]
fn detach_session(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid` is async-signal-safe and the closure does nothing else.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(|err| std::io::Error::from_raw_os_error(err as i32))
        });
    }
}

#[cfg(not(unix))]
fn detach_session(_command: &mut std::process::Command) {}

/// This process's own arguments with the launcher-only flags removed, so the
/// child re-runs exactly what was asked for.
///
/// Filtering stops at `--`: everything after it is an operand, and a plan named
/// `--json` is a plan, not a flag. Dropping it there silently ran a different
/// plan than the one the operator typed.
// §FS-rhei-run-headless.1
fn child_arguments() -> Vec<std::ffi::OsString> {
    child_arguments_from(std::env::args_os().skip(1))
}

fn child_arguments_from(
    given: impl IntoIterator<Item = std::ffi::OsString>,
) -> Vec<std::ffi::OsString> {
    let separator = std::ffi::OsStr::new("--");
    let mut arguments = Vec::new();
    let mut past_separator = false;
    for argument in given {
        if past_separator {
            arguments.push(argument);
            continue;
        }
        if argument == separator {
            past_separator = true;
        } else if LAUNCHER_ONLY_FLAGS.iter().any(|flag| argument == std::ffi::OsStr::new(flag)) {
            continue;
        }
        arguments.push(argument);
    }
    arguments
}
