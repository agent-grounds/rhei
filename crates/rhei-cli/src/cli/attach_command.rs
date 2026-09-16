// `rhei attach`: connect a surface to a run this process did not start.
//
// A reader of files plus a client of the two boundaries the dashboard already
// has. It never drives the run, never transitions the plan on its own, and
// leaves nothing behind when it disconnects.

// §FS-rhei-run-headless.5

/// How often the surface re-reads the run's event log and the live agent logs.
/// Matched to the TUI's own redraw tick so a poll lands roughly per frame.
const ATTACH_POLL: Duration = Duration::from_millis(150);

/// Connect to a run. §FS-rhei-run-headless.5
pub(crate) fn attach_command(
    reference: Option<&str>,
    json: bool,
    since: u64,
    wait: bool,
) -> MietteResult<()> {
    let descriptor = resolve_run(reference)?;
    let events_path = resolve_workspace_relative(&descriptor, &descriptor.events);

    if json {
        require_event_log(&descriptor, &events_path)?;
        return stream_run_json(&descriptor, &events_path, since, wait);
    }

    // `--wait` without `--json` opens **no** surface: it is the quiet CI wait,
    // and a TUI it cannot start would fail a pipeline that only ever wanted an
    // exit code. It reads no records either, so a run that never managed to
    // write an event log is still one it can wait for.

    // §FS-rhei-run-headless.5.3 §FS-rhei-run-headless.8
    if wait {
        return wait_for_run_end(&descriptor);
    }

    // The three verdicts of §FS-rhei-run-headless.3, spelled out. `Unknown` is
    // not `Ended`: the run may be working right now, so the surface opens and
    // says what it could not confirm rather than reporting a live run dead.
    match descriptor.liveness() {
        Liveness::Ended | Liveness::Gone => {
            report_finished_run(&descriptor);
            Ok(())
        }
        Liveness::Live => {
            require_event_log(&descriptor, &events_path)?;
            attach_surface(&descriptor, &events_path, None)
        }
        Liveness::Unknown(reason) => {
            require_event_log(&descriptor, &events_path)?;
            attach_surface(&descriptor, &events_path, Some(reason))
        }
    }
}

/// Refuse a surface over an event log nothing is going to write.
///
/// Ahead of either surface, because both would otherwise wait forever on a file
/// that is not there. Only the paths that actually *read* records ask for this:
/// §5.3's quiet wait opens no surface and reads none, and a run that failed to
/// write its log (§8) must still be waitable.
// §FS-rhei-run-headless.8
fn require_event_log(descriptor: &RunDescriptor, events_path: &Path) -> MietteResult<()> {
    if descriptor.liveness().has_ended() || events_path.is_file() {
        return Ok(());
    }
    Err(miette!(
        help = "the run is live but is not publishing an event log; watch it in its own \
                terminal, or open the browser dashboard",
        "run {} has no event log at {} to follow",
        descriptor.id,
        events_path.display()
    ))
}

/// Wait out the run, then print exactly what attaching to an already finished
/// run prints and exit with the run's own code. The two print the same block on
/// purpose: whether the wait outlived the run or arrived after it is not a
/// difference a caller should have to parse.
// §FS-rhei-run-headless.5.3
fn wait_for_run_end(descriptor: &RunDescriptor) -> MietteResult<()> {
    poll_until_run_ends(descriptor)?;
    let final_state = settled_end(descriptor);
    report_finished_run(&final_state);
    exit_with_run_status(&final_state)
}

/// Poll until the run's own process is no longer live.
///
/// An undecided probe is **not** an end: reporting one as "has ended" is how a
/// healthy job fails its CI step. The wait keeps going, and gives up — loudly —
/// only once the grace of [`UndecidedWatch`] has run out.
// §FS-rhei-run-headless.5.3 §FS-rhei-run-headless.3
fn poll_until_run_ends(descriptor: &RunDescriptor) -> MietteResult<()> {
    let mut undecided = UndecidedWatch::default();
    loop {
        match descriptor.liveness() {
            Liveness::Ended | Liveness::Gone => return Ok(()),
            Liveness::Live => undecided.decided(),
            Liveness::Unknown(reason) => {
                if undecided.exhausted(&reason) {
                    return Err(undecided_run(descriptor, undecided.reason()));
                }
            }
        }
        std::thread::sleep(ATTACH_POLL);
    }
}

/// The run's descriptor once its exit status has landed on it.
///
/// A run stops being *live* before it has recorded anything: it lets go of the
/// run lock when the run command returns, and stamps its exit code one frame
/// later, from the process exit path that is the only place the code is
/// knowable. Reading between those two moments finds no status on a run that
/// ended perfectly well, and `--wait` then failed the CI step it exists to
/// pass. So the stamp gets the same bounded grace an undecided probe gets:
/// long enough for an ordinary exit path, short enough that a run killed
/// outright — which will never stamp anything — is still reported as one that
/// recorded nothing.
// §FS-rhei-run-headless.5.3
fn settled_end(descriptor: &RunDescriptor) -> RunDescriptor {
    let deadline = Instant::now() + UNDECIDED_GRACE;
    loop {
        let current = recorded_end(descriptor);
        if current.exit_code.is_some() || Instant::now() >= deadline {
            return current;
        }
        std::thread::sleep(ATTACH_POLL);
    }
}

/// The diagnostic for a run whose liveness never became decidable.
/// It is an error, not a result: the one thing that must never be said here is
/// that a run nobody could check has ended. §FS-rhei-run-headless.3
fn undecided_run(descriptor: &RunDescriptor, reason: &str) -> miette::Report {
    miette!(
        help = "fix what is in the way and ask again; the run itself is untouched",
        "could not tell whether run {} is still running: {reason}",
        descriptor.id
    )
}

/// The run's descriptor as it stands now — the run stamps its terminal status
/// on the way out, which may have happened while this process was waiting.
/// Re-read under this run's id, so a *successor* on the same workspace is
/// never mistaken for it.
fn recorded_end(descriptor: &RunDescriptor) -> RunDescriptor {
    read_descriptor(&run_descriptor_path(&descriptor.workspace))
        .filter(|current| current.id == descriptor.id)
        .unwrap_or_else(|| descriptor.clone())
}

/// Resolve a descriptor path that may have been recorded workspace-relative.
fn resolve_workspace_relative(descriptor: &RunDescriptor, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        descriptor.workspace.join(path)
    }
}

fn report_finished_run(descriptor: &RunDescriptor) {
    println!("Run {} has ended.", descriptor.id);
    match descriptor.exit_code {
        Some(0) => println!("  It exited 0."),
        Some(code) => println!("  It exited {code}."),
        None => println!("  It recorded no exit status."),
    }
    // What outlived it is what the operator came back for; a live surface over
    // a dead run would have nothing to show. §FS-rhei-run-headless.5.2
    for (label, relative) in
        [("Report", "runtime/run-report.md"), ("Dashboard", "runtime/dashboard.html")]
    {
        let path = descriptor.workspace.join(relative);
        if path.is_file() {
            println!("  {label}: {}", path.display());
        }
    }
}

/// Exit with the run's own recorded status, for `--wait`.
///
/// Only a recorded `0` is success. A run that recorded *nothing* did not end
/// on its own — a `SIGKILL`, an OOM kill, a machine that went away — and
/// reporting that as success made `--wait` tell every CI job built on §5.3
/// that a killed run had passed.
// §FS-rhei-run-headless.5.3
fn exit_with_run_status(descriptor: &RunDescriptor) -> MietteResult<()> {
    match recorded_end(descriptor).exit_code.or(descriptor.exit_code) {
        Some(0) => Ok(()),
        Some(code) => std::process::exit(code),
        None => Err(miette!(
            help = "read what it managed to write: runtime/run.log and runtime/run-report.md",
            "run {} recorded no exit status, so it did not end on its own",
            descriptor.id
        )),
    }
}

// ---------------------------------------------------------------------------
// JSON attachment
// ---------------------------------------------------------------------------

/// Replay and then follow a run's records on stdout. §FS-rhei-run-headless.5.3
fn stream_run_json(
    descriptor: &RunDescriptor,
    events_path: &Path,
    since: u64,
    wait: bool,
) -> MietteResult<()> {
    let mut reader = rhei_tui::EventLogReader::open(events_path);
    let mut ending = EndingWatch::default();
    loop {
        for record in reader.poll() {
            // A record with no `seq` is not a cursor point and so is never
            // what `--since` names. §FS-rhei-run-json.2
            if record.seq.is_some_and(|seq| seq <= since) {
                continue;
            }
            ending.note(&record.event);
            // The record's own timestamp, not the replay instant: rewriting it
            // made every replayed record claim to have happened just now.
            // §FS-rhei-run-json.2
            println!(
                "{}",
                rhei_tui::encode_event(
                    record.seq,
                    &record.event,
                    record.ts,
                    Some(&descriptor.workspace)
                )
            );
        }
        // Liveness is checked only after a drained poll, so the last records of
        // a run that ends mid-poll are still emitted.
        match ending.verdict(descriptor) {
            Ending::Ended => break,
            // A truncated stream means "the run was interrupted"
            // (§FS-rhei-run-json.2.1), so ending one at exit 0 with nothing on
            // stderr states an outcome that did not happen.
            Ending::Undecided(reason) => return Err(undecided_run(descriptor, &reason)),
            Ending::Following => {}
        }
        std::thread::sleep(ATTACH_POLL);
    }
    if wait {
        // `run_finished` ends the run *loop*, not the process: reading the
        // status off the last record reported a run that succeeded as one that
        // "did not end on its own". §FS-rhei-run-headless.5.3
        poll_until_run_ends(descriptor)?;
        return exit_with_run_status(&settled_end(descriptor));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Terminal attachment
// ---------------------------------------------------------------------------

/// Drive the run TUI from the run's files until the operator detaches or the
/// run ends.
///
/// `undecided` carries the reason a liveness probe could not answer, when one
/// could not: the surface opens anyway and says so, because refusing to draw a
/// run that is probably working is the worse of the two mistakes.
// §FS-rhei-run-headless.5 §FS-rhei-run-headless.5.1 §FS-rhei-run-headless.3
fn attach_surface(
    descriptor: &RunDescriptor,
    events_path: &Path,
    undecided: Option<String>,
) -> MietteResult<()> {
    use rhei_tui::EventSink as _;

    // An external signal must not kill this process with the terminal still in
    // raw mode. Installing the run's own handlers turns it into a flag the
    // loops below read, so the surface leaves cleanly. §FS-rhei-run-tui.1.8
    install_interrupt_handlers();

    let machines = attach_machines(descriptor)?;
    let plan_path = descriptor.plan.clone();
    let loader: rhei_tui::PlanLoader =
        Arc::new(move || load_plan_for_dashboard(&plan_path, &machines));

    let context = rhei_tui::TuiContext {
        workspace: descriptor.workspace.clone(),
        plan_loader: Some(loader),
        intervene: Some(Arc::new(ControlInterveneSink::new(descriptor.control_url.clone()))),
        gate: Some(Arc::new(ControlGateSink::new(descriptor.control_url.clone()))),
        // An attached surface has an operator in front of it whenever it is
        // still drawing; only a signal takes that away.
        stop_requested: Arc::new(interrupt_requested),
        // The whole difference: Ctrl+C detaches instead of signalling the run.
        // §FS-rhei-run-headless.5.1
        attached: true,
    };

    let parallel = descriptor.parallel.clamp(1, usize::from(u16::MAX)) as u16;
    let tui = rhei_tui::TuiSink::start(parallel, 0, context).map_err(|err| {
        miette!(
            help = "attaching needs an interactive terminal; follow the run's records \
                    instead with: rhei attach --json",
            "could not start the attached surface: {err}"
        )
    })?;

    tui.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Info,
        text: format!(
            "Attached to run {} (pid {}). Ctrl+C or q detaches; `rhei stop {}` stops the run.",
            descriptor.id, descriptor.pid, descriptor.id
        ),
    });
    if descriptor.control_url.is_none() {
        tui.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: "This run serves no control endpoint: intervene and gate release are \
                   unavailable from here."
                .to_string(),
        });
    }
    if let Some(reason) = undecided {
        tui.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: format!(
                "Could not confirm this run is still live ({reason}); attaching anyway."
            ),
        });
    }

    follow_run(descriptor, events_path, &tui);
    // Ends the render thread, or joins one that has already left because the
    // operator detached. Either way the terminal is restored before returning.
    tui.finish();
    Ok(())
}

/// What a follower should do next.
// §FS-rhei-run-json.2.1 §FS-rhei-run-headless.3
enum Ending {
    /// The run may still write more.
    Following,
    /// The run is over and its tail has been drained.
    Ended,
    /// Liveness stayed undecided past its grace, so whatever has been read may
    /// not be the whole run.
    Undecided(String),
}

/// When a follower may stop reading.
/// `run_finished` marks the end of the run *loop*, but the run still has a
/// closing diagnostic or two to write — the frozen dashboard's path, for one —
/// and those are written after it. Stopping on the record itself dropped them.
/// So a follower keeps reading for one more round, which is enough because the
/// run is already on its way out.
// §FS-rhei-run-json.2.1
#[derive(Default)]
struct EndingWatch {
    saw_finish: bool,
    drained_after_end: bool,
    undecided: UndecidedWatch,
}

impl EndingWatch {
    fn note(&mut self, event: &rhei_tui::RunEvent) {
        self.saw_finish |= matches!(event, rhei_tui::RunEvent::RunFinished { .. });
    }

    /// Whether there is anything left to wait for.
    ///
    /// A probe that could not answer keeps the follower reading: a run whose
    /// lock went unreadable is not a run that ended, and stopping there
    /// truncates a live run's stream. The grace bounds it so an outage that
    /// never clears is reported rather than followed forever.
    ///
    /// The extra drain is unconditional: a run whose *process* is gone has the
    /// same tail-loss window as one that only wrote `run_finished`, and one
    /// more poll costs a single tick.
    // §FS-rhei-run-headless.3
    fn verdict(&mut self, descriptor: &RunDescriptor) -> Ending {
        if !self.saw_finish {
            match descriptor.liveness() {
                Liveness::Ended | Liveness::Gone => {}
                Liveness::Live => {
                    self.undecided.decided();
                    return Ending::Following;
                }
                Liveness::Unknown(reason) => {
                    return if self.undecided.exhausted(&reason) {
                        Ending::Undecided(self.undecided.reason().to_string())
                    } else {
                        Ending::Following
                    };
                }
            }
        }
        if self.drained_after_end {
            return Ending::Ended;
        }
        self.drained_after_end = true;
        Ending::Following
    }
}

/// Pump the run's events into the surface until it is detached or the run ends.
fn follow_run(descriptor: &RunDescriptor, events_path: &Path, tui: &rhei_tui::TuiSink) {
    use rhei_tui::EventSink as _;

    let mut reader = rhei_tui::EventLogReader::open(events_path);
    let mut tailer = AgentLogTailer::default();
    let mut ending = EndingWatch::default();
    loop {
        for record in reader.poll() {
            let event = record.event;
            // Slot lifecycle decides which per-task logs are worth following.
            match &event {
                rhei_tui::RunEvent::SlotAssigned { task, slot, log_path, .. } => {
                    tailer.follow(&descriptor.workspace, task, *slot, log_path);
                }
                rhei_tui::RunEvent::SlotReleased { task, slot, .. } => {
                    // Drain before the release lands so the worker's last lines
                    // are drawn under the slot that produced them.
                    for output in tailer.release(task, *slot) {
                        tui.emit(output);
                    }
                }
                _ => {}
            }
            ending.note(&event);
            tui.emit(event);
        }
        for output in tailer.poll() {
            tui.emit(output);
        }

        if tui.screen_restored() || interrupt_requested() {
            // The operator detached, or a signal arrived. The run is untouched:
            // no signal was sent and nothing was written. §FS-rhei-run-headless.5.1
            return;
        }
        match ending.verdict(descriptor) {
            Ending::Ended => return,
            // There is an operator in front of this surface, so the bound on an
            // undecided run is the operator: say what could not be checked and
            // keep drawing until they detach. §FS-rhei-run-headless.3
            Ending::Undecided(reason) => tui.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Warn,
                text: format!("Still cannot confirm this run is live ({reason})."),
            }),
            Ending::Following => {}
        }
        std::thread::sleep(ATTACH_POLL);
    }
}

/// Resolve the state machines the attached plan is judged under, so the surface
/// renders the same machine the run is executing.
fn attach_machines(descriptor: &RunDescriptor) -> MietteResult<rhei_validator::MachineSet> {
    let loaded = load_plan(&descriptor.plan).map_err(|err| {
        miette!(
            help = format!(
                "the run's plan must still be readable to attach to it: {}",
                descriptor.plan.display()
            ),
            "could not load the plan of run {}: {err}",
            descriptor.id
        )
    })?;
    // The run's own `--state-machine`, not whatever the default resolves to
    // now: an attached surface that renders a different machine shows states
    // the run cannot be in. §FS-rhei-run-headless.5
    let resolved = resolve_state_machines_for_loaded_plan(
        &descriptor.plan,
        &loaded,
        descriptor.state_machine.as_deref(),
    )?;
    Ok(ExecutionMachines::build(&resolved, &descriptor.plan, &loaded)?.set)
}
