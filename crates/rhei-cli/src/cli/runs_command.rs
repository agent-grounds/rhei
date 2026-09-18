// `rhei runs` and `rhei stop`: see what is live on this machine, and ask one of
// them to stop.
//
// Stopping is a signal, not a route. The loopback control server keeps the
// single inbound mutation boundary it was designed around, and stopping
// inherits the run's interruption contract whole instead of growing a second
// teardown.

// §FS-rhei-run.3.2 §FS-rhei-run-headless.6 §FS-rhei-run-headless.7
// §DA-detached-runs

/// How long a `--kill` waits between asking once and asking again. The run's
/// own handler escalates on the second signal, so this is literally the
/// operator asking twice — not a different mechanism. §FS-rhei-run.3.2
const KILL_ESCALATION_GRACE: Duration = Duration::from_secs(2);

/// How often `--wait` re-checks whether the run has actually gone.
const WAIT_POLL: Duration = Duration::from_millis(200);

/// List the runs that are live on this machine. §FS-rhei-run-headless.6
///
/// `--all` and the window flags turn the same command into a question about
/// history, answered next door; with none of them nothing here moves.
// §FS-rhei-run-headless.6.1
pub(crate) fn runs_command(
    json: bool,
    all: bool,
    since: Option<&str>,
    until: Option<&str>,
) -> MietteResult<()> {
    if let Some(query) = RunHistoryQuery::resolve(all, since, until)? {
        return runs_history_command(json, &query);
    }
    let sweep = sweep_run_registry();
    sweep.ensure_access()?;
    if json {
        let rendered = serde_json::to_string_pretty(&sweep.live).map_err(|err| {
            miette!(
                help = "read the run list as text instead: rhei runs",
                "could not render the run list as JSON: {err}"
            )
        })?;
        println!("{rendered}");
        // Stdout is the array and nothing else, so what could not be decided
        // is said on stderr rather than silently dropped.
        for entry in &sweep.undecided {
            eprintln!("warning: could not check {}: {}", entry.summary_line(), entry.reason);
        }
        return Ok(());
    }
    // An empty list is an answer, not a failure: nothing running is the normal
    // state of a machine. §FS-rhei-run-headless.6
    if sweep.live.is_empty() {
        println!("No runs are live on this machine.");
        report_undecided_runs(&sweep.undecided);
        println!("Start one with: rhei run --headless <plan>");
        return Ok(());
    }
    let live = sweep.live.len();
    println!("{live} live run{}:", if live == 1 { "" } else { "s" });
    for run in &sweep.live {
        println!("  {}", run.summary_line());
        println!("      started {}  {}", run.started_at, run.workspace.display());
        if let Some(url) = &run.control_url {
            // The **control** URL, not "the dashboard": under `--no-dashboard`
            // nothing may invite a browser, and the endpoints an attached
            // surface needs are up either way. §FS-rhei-run-headless.4
            println!("      control   {url}");
        }
    }
    report_undecided_runs(&sweep.undecided);
    println!();
    println!("Attach to one with: rhei attach <id>");
    Ok(())
}

/// Say what could not be checked, and why.
///
/// Silently keeping these and silently omitting them from the listing is the
/// same lie by a different route: the operator reads "no runs are live" for a
/// machine whose registry this process simply could not read.
// §FS-rhei-run-headless.3 §FS-rhei-run-headless.6
fn report_undecided_runs(entries: &[UndecidedRun]) {
    if entries.is_empty() {
        return;
    }
    println!();
    println!("{} entr{} could not be checked:", entries.len(), if entries.len() == 1 { "y" } else { "ies" });
    for entry in entries {
        println!("  {}", entry.summary_line());
        println!("      {}", entry.reason);
    }
    println!("  Kept: an unreadable file says nothing about the process it describes.");
}

/// Ask a run to stop. §FS-rhei-run-headless.7
pub(crate) fn stop_command(reference: Option<&str>, kill: bool, wait: bool) -> MietteResult<()> {
    let descriptor = resolve_run(reference)?;
    // Stop observes and signals a run only while its root is coherent. §FS-rhei-recover.4
    let _root_guards = rhei_core::root_access::for_input(&descriptor.workspace).map_err(|err| miette!("{err}"))?;
    // Stopping something that has already stopped is not an error: the
    // operator's intent — "make sure this is not running" — is satisfied.
    //
    // Only a *decided* end short-circuits, though. An entry this process could
    // not check is not an entry to report as ended: it still reaches the
    // platform's pre-signal authorization path.

    // §FS-rhei-run-headless.3 §FS-rhei-run-headless.7
    match descriptor.liveness() {
        Liveness::Ended | Liveness::Gone => {
            // A Linux ownership mismatch is an ended listing verdict; an
            // existing recorded process still needs signal authorization to refuse an unowned pid.
            // §FS-rhei-run-headless.7
            if !ended_run_has_live_recorded_process(&descriptor)? {
                println!("Run {} has already ended.", descriptor.id);
                report_recorded_result(&descriptor);
                return Ok(());
            }
        }
        Liveness::Live => {}
        Liveness::Unknown(reason) => {
            report_unknown_stop_liveness(&descriptor, &reason);
        }
    }

    confirm_signal_target(&descriptor)?;
    signal_run(&descriptor, "stop")?;
    println!("Asked run {} (pid {}) to stop.", descriptor.id, descriptor.pid);
    if kill {
        // The run is owed the grace its first signal opened before the second
        // one takes it away. §FS-rhei-run.3.2
        std::thread::sleep(KILL_ESCALATION_GRACE);
        // Undecided is not ended, so it does not skip the escalation the
        // operator asked for. §FS-rhei-run-headless.3
        if !descriptor.liveness().has_ended() {
            confirm_signal_target(&descriptor)?;
            signal_run(&descriptor, "escalate")?;
            println!("Asked again — in-flight work is being killed without its grace.");
        }
    }

    if wait {
        await_run_end(&descriptor)?;
        println!("Run {} has ended.", descriptor.id);
        // Re-read, and give the stamp time to land: a run is no longer live a
        // frame before it records its code. §FS-rhei-run-headless.5.3
        report_recorded_result(&settled_end(&descriptor));
    } else {
        println!("It is terminating its in-flight work; `rhei runs` shows when it is gone.");
    }
    Ok(())
}

/// Keep an alive but unowned Linux pid on the pre-signal authorization path.
/// A gone pid remains an already-ended run; a live pid may have been reused or
/// may belong to an unrelated process, and only the exact lock proof can tell.
// §FS-rhei-run-headless.7
#[cfg(target_os = "linux")]
fn ended_run_has_live_recorded_process(descriptor: &RunDescriptor) -> MietteResult<bool> {
    use rustix::process::{pidfd_open, Pid, PidfdFlags};

    if !matches!(descriptor.status, RunStatus::Running) {
        return Ok(false);
    }
    let Some(pid) = i32::try_from(descriptor.pid).ok().and_then(Pid::from_raw) else {
        return Ok(false);
    };
    pidfd_open_result_has_live_recorded_process(
        descriptor,
        pidfd_open(pid, PidfdFlags::empty()),
    )
}

#[cfg(target_os = "linux")]
fn pidfd_open_result_has_live_recorded_process<T>(
    descriptor: &RunDescriptor,
    result: rustix::io::Result<T>,
) -> MietteResult<bool> {
    match result {
        Ok(_) => Ok(true),
        Err(rustix::io::Errno::SRCH) => Ok(false),
        Err(err) => Err(miette!(
            help = "check whether this system supports pidfds and retry `rhei stop`",
            "refusing to stop run {} because pid {} could not be checked safely: {err}",
            descriptor.id,
            descriptor.pid
        )),
    }
}

#[cfg(not(target_os = "linux"))]
fn ended_run_has_live_recorded_process(_descriptor: &RunDescriptor) -> MietteResult<bool> {
    Ok(false)
}

#[cfg(target_os = "linux")]
fn report_unknown_stop_liveness(descriptor: &RunDescriptor, reason: &str) {
    eprintln!(
        "warning: could not confirm whether run {} is still running ({reason}); \
         checking exact run-lock ownership before signalling pid {}",
        descriptor.id, descriptor.pid
    );
}

#[cfg(not(target_os = "linux"))]
fn report_unknown_stop_liveness(descriptor: &RunDescriptor, reason: &str) {
    eprintln!(
        "warning: could not confirm whether run {} is still running ({reason}); \
         signalling pid {} anyway",
        descriptor.id, descriptor.pid
    );
}

/// Block until the run has actually gone, not merely until it said it would go.
///
/// An undecided probe keeps the wait going: returning on one reported a run as
/// ended while its process was still tearing down its in-flight work. The grace
/// of [`UndecidedWatch`] bounds it, and running out is an error rather than a
/// quiet "it has ended".
// §FS-rhei-run-headless.7 §FS-rhei-run-headless.3
fn await_run_end(descriptor: &RunDescriptor) -> MietteResult<()> {
    let mut undecided = UndecidedWatch::default();
    loop {
        match descriptor.liveness() {
            Liveness::Ended | Liveness::Gone => return Ok(()),
            Liveness::Live => undecided.decided(),
            Liveness::Unknown(reason) => {
                if undecided.exhausted(&reason) {
                    return Err(miette!(
                        help = "the signal was delivered; check the run's own workspace for \
                                what it recorded",
                        "asked run {} to stop, but could not confirm it ended: {}",
                        descriptor.id,
                        undecided.reason()
                    ));
                }
            }
        }
        std::thread::sleep(WAIT_POLL);
    }
}

fn report_recorded_result(descriptor: &RunDescriptor) {
    match descriptor.exit_code {
        Some(0) => println!("  It exited 0."),
        Some(code) => println!("  It exited {code}."),
        // A `SIGKILL`ed run never got to record one. Say so rather than
        // inventing a status the run did not report. §FS-rhei-run-headless.2
        None => println!("  It recorded no exit status."),
    }
    let report = descriptor.workspace.join("runtime/run-report.md");
    if report.is_file() {
        println!("  Report: {}", report.display());
    }
}

/// Re-read the workspace descriptor immediately before signalling, and refuse
/// unless it still names this run *and* this pid. Linux also refuses when this
/// last re-read is inconclusive; other platforms retain the previous best-
/// effort behavior because they have no stable ownership proof.
///
/// A registry entry is a memory of a pid, and pids are reused. Between
/// resolving the run and delivering the signal the process may have died and
/// its pid been handed to something else entirely — so the authoritative copy
/// gets the last word. On Linux, an unreadable descriptor is one more missing
/// part of the exact ownership proof and therefore refuses the signal.
// §FS-rhei-run-headless.7 §FS-rhei-run-headless.3
fn confirm_signal_target(descriptor: &RunDescriptor) -> MietteResult<()> {
    let path = run_descriptor_path(&descriptor.workspace);
    match read_descriptor_result(&path) {
        DescriptorRead::Loaded(current)
            if current.id == descriptor.id && current.pid == descriptor.pid =>
        {
            Ok(())
        }
        DescriptorRead::Loaded(current) => Err(miette!(
            help = "see what is live on this machine with: rhei runs",
            "run {} is no longer the run on {}: it now holds run {} (pid {})",
            descriptor.id,
            descriptor.workspace.display(),
            current.id,
            current.pid
        )),
        DescriptorRead::Missing => Err(miette!(
            help = "see what is live on this machine with: rhei runs",
            "run {} left no descriptor at {}, so there is nothing to confirm its pid against",
            descriptor.id,
            path.display()
        )),
        DescriptorRead::Unreadable(why) => confirm_unreadable_signal_target(descriptor, &path, &why),
    }
}

#[cfg(target_os = "linux")]
fn confirm_unreadable_signal_target(
    descriptor: &RunDescriptor,
    path: &Path,
    why: &str,
) -> MietteResult<()> {
    Err(miette!(
        help = "restore access to the workspace descriptor, then retry `rhei stop`",
        "refusing to signal run {} because {} could not be re-read: {why}",
        descriptor.id,
        path.display()
    ))
}

#[cfg(not(target_os = "linux"))]
fn confirm_unreadable_signal_target(
    descriptor: &RunDescriptor,
    path: &Path,
    why: &str,
) -> MietteResult<()> {
    eprintln!(
        "warning: could not re-read {} ({why}); signalling pid {} on the registry's \
         word alone",
        path.display(),
        descriptor.pid
    );
    Ok(())
}

/// Deliver `SIGINT` to the run, entering §FS-rhei-run.3.2 exactly as an
/// operator's Ctrl+C does. Linux first opens a stable process handle and proves
/// that exact identity owns this run's stamped lock; current-path contention is
/// listing evidence, never signal authorization. §FS-rhei-run-headless.7
#[cfg(target_os = "linux")]
fn signal_run(descriptor: &RunDescriptor, what: &str) -> MietteResult<()> {
    use rustix::process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags};

    let raw_pid = i32::try_from(descriptor.pid).ok();
    let pid = raw_pid.and_then(Pid::from_raw).ok_or_else(|| {
        miette!(
            help = "check `rhei runs` and the run's workspace descriptor",
            "refusing to {what} run {} because pid {} is not a valid process id",
            descriptor.id,
            descriptor.pid
        )
    })?;
    // Open first: if the process exits after ownership is proved, this handle
    // cannot silently retarget a reused numeric pid. §FS-rhei-run-headless.7
    let process = pidfd_open(pid, PidfdFlags::empty()).map_err(|err| {
        miette!(
            help = "the run may have ended already; check `rhei runs`",
            "refusing to {what} run {} because pid {} could not be opened safely: {err}",
            descriptor.id,
            descriptor.pid
        )
    })?;
    match probe_recorded_lock_owner_for_signal(descriptor, &process) {
        ProcessProbe::OwnsLock => {}
        ProcessProbe::DoesNotOwnLock => {
            return Err(miette!(
                help = "check `rhei runs`; the descriptor may be stale or the lock may belong to another process",
                "refusing to {what} run {} because pid {} does not own its recorded run lock",
                descriptor.id,
                descriptor.pid
            ));
        }
        ProcessProbe::Gone => {
            return Err(miette!(
                help = "check `rhei runs`; the recorded process may already have ended",
                "refusing to {what} run {} because pid {} ended before lock ownership could be confirmed",
                descriptor.id,
                descriptor.pid
            ));
        }
        ProcessProbe::Unknown(reason) => {
            return Err(miette!(
                help = "restore access to the recorded process and its lock, then retry `rhei stop`",
                "refusing to {what} run {} because pid {} lock ownership could not be confirmed: {reason}",
                descriptor.id,
                descriptor.pid
            ));
        }
    }
    pidfd_send_signal(&process, rustix::process::Signal::INT).map_err(|err| {
        miette!(
            help = "the run may have ended already; check `rhei runs`",
            "could not {what} run {} (pid {}): {err}",
            descriptor.id,
            descriptor.pid
        )
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn signal_run(descriptor: &RunDescriptor, what: &str) -> MietteResult<()> {
    let pid = Pid::from_raw(descriptor.pid as i32);
    signal::kill(pid, Signal::SIGINT).map_err(|err| {
        miette!(
            help = "the run may have ended already; check `rhei runs`",
            "could not {what} run {} (pid {}): {err}",
            descriptor.id,
            descriptor.pid
        )
    })
}

#[cfg(not(unix))]
fn signal_run(descriptor: &RunDescriptor, _what: &str) -> MietteResult<()> {
    Err(miette!(
        help = "stop the run from the terminal that is running it",
        "`rhei stop` needs POSIX signals and is not supported on this platform yet \
         (run {} is pid {})",
        descriptor.id,
        descriptor.pid
    ))
}
