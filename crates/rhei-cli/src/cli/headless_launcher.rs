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
    // §REQ-test-isolation.7: the marker is read through the one reader of rhei's own variables.
    inherited_rhei_var_os(HEADLESS_CHILD_ENV).is_some()
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
    // Hold the complete shared owner set before any parent launch effects. §FS-rhei-recover.4
    forced_boundary("launcher-before-guards")?;
    // §FS-rhei-errors.6
    let _guards = rhei_core::root_access::for_input(input).map_err(|err| root_access_report(input, err))?;
    forced_boundary("launcher-guarded")?;
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
            help = format!(
                "set HOME or XDG_STATE_HOME, or run in the foreground with `{}`",
                help_command_with("run", "--no-tui")
            ),
            "a detached run needs a state directory to publish its id into, and neither \
             XDG_STATE_HOME nor HOME is set"
        ));
    }

    let log_path = run_console_log_path(&workspace_root);
    let mut child = spawn_detached_run(&log_path, options)?;
    let pid = child.id();

    match await_child_ready(&mut child, pid, &workspace_root) {
        Ok(outcome) => announce_launch(
            &outcome,
            options.json(),
            options.announces_dashboard(),
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        )
        .map_err(|error| {
            miette!(
                help = format!(
                    "the run started regardless: `rhei attach {}` reaches it",
                    shell_word(&workspace_root)
                ),
                "could not write the launch report: {error}"
            )
        }),
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

/// Hand the operator the id of a child the handshake found, on `out`, with
/// any warning about it on `err`. The id resolves before its first byte is
/// printed, or the warning says why it will not.
// §FS-rhei-run-headless.1.1
fn announce_launch(
    outcome: &LaunchOutcome,
    json: bool,
    announce_dashboard: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    let (LaunchOutcome::Running(descriptor) | LaunchOutcome::FinishedEarly(descriptor)) = outcome;
    // The child's entry may not have landed yet; publish it before printing. §FS-rhei-run-headless.1.1
    let unregistered = publish_registry_entry_once(descriptor).err();
    let reported = match outcome {
        LaunchOutcome::Running(_) => report_launched(descriptor, json, announce_dashboard, out),
        LaunchOutcome::FinishedEarly(_) => report_finished_early(descriptor, json, out, err),
    };
    // A failed report does not swallow the warning. §FS-rhei-run-headless.2
    let warned = unregistered.map_or(Ok(()), |why| writeln!(err, "{}", unregistered_run_warning(descriptor, &why)));
    reported.and(warned)
}

fn report_launched(
    descriptor: &RunDescriptor,
    json: bool,
    announce_dashboard: bool,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        return writeln!(out, "{}", descriptor_json(descriptor));
    }
    writeln!(out, "Run {} started headless (pid {}).", descriptor.id, descriptor.pid)?;
    writeln!(out, "  attach:  rhei attach {}", descriptor.id)?;
    writeln!(out, "  stop:    rhei stop {}", descriptor.id)?;
    if let Some(log) = &descriptor.log {
        writeln!(out, "  log:     {}", log.display())?;
    }
    // Withheld under `--no-dashboard`: the control server is up because an
    // attached surface needs it, but nobody asked to be sent to a browser.
    // §FS-rhei-run-headless.4
    if announce_dashboard {
        if let Some(url) = &descriptor.control_url {
            writeln!(out, "  browser: {url}")?;
        }
    }
    Ok(())
}

/// A run short enough to finish inside the handshake window. It started, it
/// worked, and it ended — the id still resolves, so say so rather than call a
/// completed plan a failed launch. §FS-rhei-run-headless.1.1
fn report_finished_early(
    descriptor: &RunDescriptor,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        // The record still carries the id, so the CI shape of §FS-rhei-run-headless.5.3 keeps
        // working; the prose about it belongs on stderr.
        writeln!(out, "{}", descriptor_json(descriptor))?;
        return writeln!(err, "Run {} finished before the launcher returned.", descriptor.id);
    }
    writeln!(out, "Run {} finished before the launcher returned.", descriptor.id)?;
    match descriptor.exit_code {
        Some(code) => writeln!(out, "  It exited {code}.")?,
        None => writeln!(out, "  It recorded no exit status.")?,
    }
    writeln!(out, "  attach:  rhei attach {}", descriptor.id)?;
    if let Some(log) = &descriptor.log {
        writeln!(out, "  log:     {}", log.display())?;
    }
    Ok(())
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
