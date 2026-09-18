// `rhei run --headless`: re-execute `rhei run` in its own session and hand the
// operator its id.
//
// There is no daemon here and no second execution path. The child is an
// ordinary `rhei run` that happens to have been started with its console
// redirected and its session detached; it takes the same locks, drives the same
// loop, and writes the same report.

// §FS-rhei-run-headless.1 §DA-detached-runs

/// Environment marker telling a spawned `rhei run` that it is the detached
/// child, so it records itself as headless and stays alive for human gates.
/// Cleared again for every subprocess the run supervises, so an agent's own
/// `rhei run` is not mistaken for this one.
// §FS-rhei-run-headless.1.2
pub(crate) const HEADLESS_CHILD_ENV: &str = "RHEI_HEADLESS_CHILD";

/// Flags the launcher consumes rather than forwarding. `--headless` would make
/// the child detach again; `--json` and its companion describe *the launcher's*
/// output, and a JSON stream into a log file nobody reads is not what the
/// caller asked for — the detached run's machine-readable form is
/// `runtime/events.jsonl`.
// §FS-rhei-run-headless.1
const LAUNCHER_ONLY_FLAGS: [&str; 3] = ["--headless", "--json", "--json-agent-output"];

/// How long to wait for the child to report itself running before giving up on
/// the handshake. Generous: a large project's initial validation is real work.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const HANDSHAKE_POLL: Duration = Duration::from_millis(50);

/// The launcher's own lock, inside the workspace it is about to launch into.
/// Distinct from the run lock: it covers the launcher's non-atomic stretch —
/// pre-check, truncate `run.log`, spawn, handshake — which two simultaneous
/// launches would otherwise interleave, clobbering one another's console log
/// and each waiting out the full handshake on the other's child.
// §FS-rhei-run-headless.1.1
const HEADLESS_LAUNCH_LOCK: &str = "headless-launch.lock";

/// Whether this process is the detached child of a `--headless` launch.
pub(crate) fn is_headless_child() -> bool {
    std::env::var_os(HEADLESS_CHILD_ENV).is_some()
}

/// The one-live-run-per-rhei refusal, naming the run that is in the way.
///
/// Shared by the launcher's pre-check and the detached child's own fail-fast,
/// so an operator sees one diagnostic whichever of the two noticed first.
// §FS-rhei-run.2.6
pub(crate) fn run_lock_conflict(root: &Path) -> miette::Report {
    match read_descriptor(&run_descriptor_path(root)).filter(|run| !run.liveness().has_ended()) {
        Some(live) => miette!(
            help = format!(
                "watch it with `rhei attach {id}`, or stop it with `rhei stop {id}`",
                id = live.id
            ),
            "a run is already live on {}:\n  {}",
            root.display(),
            live.summary_line()
        ),
        None => miette!(
            help = "see what is live on this machine with: rhei runs",
            "a run is already live on {} and holds its .rhei/run.lock",
            root.display()
        ),
    }
}

/// Launch a detached run and report its id. §FS-rhei-run-headless.1
pub(crate) fn launch_headless_run(
    input: &Path,
    options: &RunOptions,
) -> MietteResult<()> {
    let workspace_root = execution_workspace_root(&normalize_workspace_input(input));
    // Held from here to the end of the handshake. §FS-rhei-run-headless.1.1
    let _launch_lock = acquire_launch_lock(&workspace_root)?;
    // Refuse before spawning anything when a live run already owns this
    // workspace: the child would fail on the lock a moment later, and this way
    // the diagnostic names the run that is in the way.

    // This is the one place an undecided probe may be read as "go ahead": the
    // child takes the real run lock a moment later and fails fast on it, so a
    // pre-check that guesses wrong costs a worse diagnostic and never a second
    // run. Written as a match so the choice is visible rather than hidden in a
    // helper.

    // §FS-rhei-run.2.6 §FS-rhei-run-headless.3 §FS-rhei-run-headless.1.1
    let occupied = read_descriptor(&run_descriptor_path(&workspace_root))
        .is_some_and(|run| match run.liveness() {
            Liveness::Live => true,
            Liveness::Ended | Liveness::Gone | Liveness::Unknown(_) => false,
        });
    if occupied {
        return Err(run_lock_conflict(&workspace_root));
    }

    if run_registry_dir().is_none() {
        return Err(miette!(
            help = "set HOME or XDG_STATE_HOME, or run in the foreground with `rhei run --no-tui`",
            "a detached run needs a state directory to publish its id into, and neither \
             XDG_STATE_HOME nor HOME is set"
        ));
    }

    let log_path = run_console_log_path(&workspace_root);
    let mut child = spawn_detached_run(&log_path, options)?;
    let pid = child.id();

    match await_child_ready(&mut child, pid, &workspace_root) {
        Ok(LaunchOutcome::Running(descriptor)) => {
            report_launched(&descriptor, options.json(), options.announces_dashboard());
            warn_if_unregistered(&descriptor);
            Ok(())
        }
        Ok(LaunchOutcome::FinishedEarly(descriptor)) => {
            report_finished_early(&descriptor, options.json());
            warn_if_unregistered(&descriptor);
            Ok(())
        }
        // A run that exited `0` did what it was asked; there is simply nothing
        // left to attach to. Reporting that as a startup failure would fail a
        // CI step for a plan that succeeded. §FS-rhei-run-headless.1.1
        Err(HandshakeFailure::Exited(status)) if status.success() => {
            eprintln!(
                "The run finished before it published a descriptor, so there is nothing to \
                 attach to.\n  its console is at {}",
                log_path.display()
            );
            Ok(())
        }
        Err(HandshakeFailure::Exited(status)) => Err(miette!(
            help = format!("the run's console is at {}", log_path.display()),
            "the run exited before it started ({}):\n{}",
            exit_status_text(&status),
            indent_block(&log_tail(&log_path, 20))
        )),
        Err(HandshakeFailure::TimedOut) => Err(miette!(
            help = format!(
                "it may still be starting: check `rhei runs`, or read {}",
                log_path.display()
            ),
            "the run (pid {pid}) did not report itself ready within {}s",
            HANDSHAKE_TIMEOUT.as_secs()
        )),
    }
}

/// Take the launcher's lock, or refuse without waiting. A second launcher on
/// this workspace is a mistake to report, not a queue to join.
// §FS-rhei-run-headless.1.1
fn acquire_launch_lock(workspace_root: &Path) -> MietteResult<HeldRunLock> {
    let rhei_dir = workspace_root.join(".rhei");
    fs::create_dir_all(&rhei_dir)
        .map_err(|err| file_io_report(&rhei_dir, "failed to create .rhei directory", err))?;
    let path = rhei_dir.join(HEADLESS_LAUNCH_LOCK);
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|err| file_io_report(&path, "failed to open the headless launch lock", err))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(HeldRunLock {
            file,
            workspace: workspace_root.to_path_buf(),
        }),
        // Another launcher holds it — on Unix and on Windows alike.
        // §FS-rhei-run-headless.1.1
        Err(err) if lock_is_contended(&err) => Err(concurrent_launch_report(workspace_root)),
        Err(err) => {
            Err(file_io_report(&path, "failed to inspect the headless launch lock", err))
        }
    }
}

/// The diagnostic for losing the launch race. It must not read like a run-lock
/// failure: nothing is wrong with the workspace, another launcher simply got
/// there first and its run is the one to talk about.
// §FS-rhei-run-headless.1.1
fn concurrent_launch_report(workspace_root: &Path) -> miette::Report {
    match read_descriptor(&run_descriptor_path(workspace_root)).filter(|run| !run.liveness().has_ended())
    {
        Some(live) => miette!(
            help = format!("watch it with `rhei attach {id}`", id = live.id),
            "another `rhei run --headless` is starting a run on {}:\n  {}",
            workspace_root.display(),
            live.summary_line()
        ),
        None => miette!(
            help = "wait for it to print its id, then see `rhei runs`",
            "another `rhei run --headless` is already starting a run on {}",
            workspace_root.display()
        ),
    }
}

/// Say so when the id the launcher just printed does not resolve.
///
/// The child warns about a registry it could not write, but that warning goes
/// into `runtime/run.log`, which nobody is reading yet. Without this the
/// operator is handed an id that `rhei attach` will not accept and no reason
/// why.
// §FS-rhei-run-headless.2
fn warn_if_unregistered(descriptor: &RunDescriptor) {
    let registered =
        run_registry_path(&descriptor.id).is_some_and(|entry| read_descriptor(&entry).is_some());
    if registered {
        return;
    }
    eprintln!(
        "warning: run {} has no registry entry, so its id will not resolve from another \
         directory.\n  reach it by path instead: rhei attach {}",
        descriptor.id,
        shell_quote(&descriptor.workspace.display().to_string())
    );
}

fn report_launched(descriptor: &RunDescriptor, json: bool, announce_dashboard: bool) {
    if json {
        println!("{}", descriptor_json(descriptor));
        return;
    }
    println!("Run {} started headless (pid {}).", descriptor.id, descriptor.pid);
    println!("  attach:  rhei attach {}", descriptor.id);
    println!("  stop:    rhei stop {}", descriptor.id);
    if let Some(log) = &descriptor.log {
        println!("  log:     {}", log.display());
    }
    // Withheld under `--no-dashboard`: the control server is up because an
    // attached surface needs it, but nobody asked to be sent to a browser.
    // §FS-rhei-run-headless.4
    if announce_dashboard {
        if let Some(url) = &descriptor.control_url {
            println!("  browser: {url}");
        }
    }
}

/// A run short enough to finish inside the handshake window. It started, it
/// worked, and it ended — the id still resolves, so say so rather than call a
/// completed plan a failed launch. §FS-rhei-run-headless.1.1
fn report_finished_early(descriptor: &RunDescriptor, json: bool) {
    if json {
        // The record still carries the id, so the CI shape of §5.3 keeps
        // working; the prose about it belongs on stderr.
        println!("{}", descriptor_json(descriptor));
        eprintln!("Run {} finished before the launcher returned.", descriptor.id);
        return;
    }
    println!("Run {} finished before the launcher returned.", descriptor.id);
    match descriptor.exit_code {
        Some(code) => println!("  It exited {code}."),
        None => println!("  It recorded no exit status."),
    }
    println!("  attach:  rhei attach {}", descriptor.id);
    if let Some(log) = &descriptor.log {
        println!("  log:     {}", log.display());
    }
}

fn descriptor_json(descriptor: &RunDescriptor) -> String {
    serde_json::to_string(descriptor).unwrap_or_else(|_| format!(r#"{{"id":"{}"}}"#, descriptor.id))
}

enum HandshakeFailure {
    /// The child exited before publishing — an invalid plan, a held lock, an
    /// unresolvable agent. Its own diagnostic is in the console log.
    Exited(std::process::ExitStatus),
    TimedOut,
}

/// How a launch ended, both ways being a launch that worked.
enum LaunchOutcome {
    Running(RunDescriptor),
    FinishedEarly(RunDescriptor),
}

/// `ExitStatus`'s own `Display` is already a sentence ("exit status: 0"), so
/// interpolating it after the word "status" said it twice.
fn exit_status_text(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit status {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    "no exit status".to_string()
}

/// Wait for the child to publish a descriptor naming its own pid, or to exit.
///
/// The **workspace** descriptor is the handshake, not the registry entry: the
/// launcher already knows the workspace, and a registry the launcher never
/// checked it could write turned an unwritable state directory into a
/// 30-second wait for a run that was working the whole time.
///
/// Matching on the pid is what makes this race-free without deleting anything:
/// a descriptor left by an earlier run in the same workspace names a different
/// process, so it can never be mistaken for this one's handshake.
// §FS-rhei-run-headless.1.1
fn await_child_ready(
    child: &mut std::process::Child,
    pid: u32,
    workspace_root: &Path,
) -> Result<LaunchOutcome, HandshakeFailure> {
    let descriptor_path = run_descriptor_path(workspace_root);
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        if let Some(outcome) = child_outcome(&descriptor_path, pid) {
            return Ok(outcome);
        }
        // Checked after the descriptor so a run that started and finished
        // inside one poll interval is still reported as started.
        if let Ok(Some(status)) = child.try_wait() {
            if let Some(outcome) = child_outcome(&descriptor_path, pid) {
                return Ok(outcome);
            }
            return Err(HandshakeFailure::Exited(status));
        }
        if Instant::now() >= deadline {
            return Err(HandshakeFailure::TimedOut);
        }
        std::thread::sleep(HANDSHAKE_POLL);
    }
}

/// What the workspace descriptor says about this child, if it is this child's.
/// A run that recorded a *failing* end is left to the exit path, which has the
/// console tail to explain it with.
fn child_outcome(descriptor_path: &Path, pid: u32) -> Option<LaunchOutcome> {
    let descriptor = read_descriptor(descriptor_path).filter(|run| run.pid == pid)?;
    match descriptor.status {
        RunStatus::Running => Some(LaunchOutcome::Running(descriptor)),
        RunStatus::Finished if descriptor.exit_code == Some(0) => {
            Some(LaunchOutcome::FinishedEarly(descriptor))
        }
        RunStatus::Finished | RunStatus::Failed => None,
    }
}

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
            help = "run it in the foreground instead: rhei run --no-tui <plan>",
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
    std::env::var(HEADLESS_CHILD_ENV)
        .ok()
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

/// The last `lines` non-empty lines of a console log, for a startup diagnostic.
fn log_tail(path: &Path, lines: usize) -> String {
    let Ok(contents) = fs::read_to_string(path) else {
        return format!("(no console output at {})", path.display());
    };
    let tail: Vec<&str> = contents.lines().filter(|line| !line.trim().is_empty()).collect();
    if tail.is_empty() {
        return format!("(the run wrote nothing to {})", path.display());
    }
    tail[tail.len().saturating_sub(lines)..].join("\n")
}

fn indent_block(text: &str) -> String {
    text.lines().map(|line| format!("  {line}")).collect::<Vec<_>>().join("\n")
}
