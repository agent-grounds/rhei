// How `rhei run` names the transcript of one agent invocation, and how anything
// that later wants to read one finds it.
//
// Four sides have to agree on the rule: the spawn that opens the file, the
// prompt that cites an earlier visit's, the reset that sweeps a ticket's
// runtime, and the engine's own account of a ticket it finished without
// spawning a worker. A second copy of the rule is how one of them drifts, so
// the rule lives here and only here.
//
// The name says which invocation, which entry and which attempt. *Which* entry
// is a fact about the ticket's movement history and *which* attempt one about
// its spawn records; neither is ever read back out of the directory listing.

// §AR-source-file-size.3 §FS-rhei-agents.8.1 §FS-rhei-memory.4.4 §FS-rhei-run.3

/// The number an invocation's log name carries, once it has been resolved.
///
/// Kept apart from the suffix it spells because two readers need the number
/// itself: the session a metric iteration binds names it as its `visit`, and the
/// upgrade fallback only applies to an entry number.
// §FS-rhei-agents.8.1 §FS-rhei-metrics.4
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogNumber {
    /// A counted state's `{visit_count}`.
    Visit(u64),
    /// An uncounted state's entry number.
    Entry(u64),
    /// A `poll:` state: its re-spawns are poll attempts, not entries.
    Unnumbered,
}

impl LogNumber {
    /// The number the name spells, where it spells one.
    fn value(self) -> Option<u64> {
        match self {
            LogNumber::Visit(n) | LogNumber::Entry(n) => Some(n),
            LogNumber::Unnumbered => None,
        }
    }

    /// The number a session is reported under: the one its name carries, 1
    /// where it carries none. §FS-rhei-metrics.4
    fn shown(self) -> u64 {
        self.value().unwrap_or(1).max(1)
    }
}

/// Which number names this invocation's log: a counted state's `{visit_count}`,
/// none for a `poll:` state, and otherwise the entry number, read from the
/// ledger. An `Err` is the diagnostic an unreadable ledger is, already worded as
/// the refusal a spawn prints.
// §FS-rhei-agents.8.1 §FS-rhei-transitions.4.3
fn resolve_log_number(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    visit_count: u64,
    task_root: &Path,
    runtime_dir: &Path,
    task_id: &str,
) -> Result<LogNumber, String> {
    if state_counts_visits(machine, state_name) {
        return Ok(LogNumber::Visit(visit_count));
    }
    if machine.states.get(state_name).is_some_and(|def| def.poll.is_some()) {
        return Ok(LogNumber::Unnumbered);
    }
    ticket_entry_number(task_root, runtime_dir, task_id, state_name).map(LogNumber::Entry)
}

/// How many times the ticket has arrived in `state_name`: its arrivals over the
/// two ledgers the move count reads, summed, plus one when it was placed there
/// initially — its first ledger line leaves the state, or it has none yet.
///
/// The movement parser is the ledger's own, so a forced metadata row and its
/// movement row are one arrival and a metadata row it does not know is none. A
/// missing ledger is no lines; one that cannot be read or parsed is an `Err`,
/// because guessing an entry number is how a log gets reused.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
fn ticket_entry_number(
    task_root: &Path,
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
) -> Result<u64, String> {
    let owning = task_root.join("runtime").join("state-transitions.log");
    let running = runtime_dir.join("state-transitions.log");
    let mut first_from: Option<String> = None;
    let mut arrivals = 0u64;
    for path in std::iter::once(&owning).chain((running != owning).then_some(&running)) {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(unreadable_ledger(path, &err.to_string())),
        };
        let movements = rhei_core::transition_history::parse(&raw)
            .map_err(|err| unreadable_ledger(path, &err.to_string()))?;
        for movement in movements.into_iter().filter(|movement| movement.task_id == task_id) {
            first_from.get_or_insert_with(|| movement.from.clone());
            arrivals += u64::from(movement.to == state_name);
        }
    }
    let placed_here = first_from.is_none_or(|from| from == state_name);
    Ok((arrivals + u64::from(placed_here)).max(1))
}

/// The refusal an unreadable ledger is: no entry number, so no name. §FS-rhei-agents.8.1
fn unreadable_ledger(path: &Path, err: &str) -> String {
    format!(
        "refusing to spawn: the transition ledger {} cannot be read, so the entry number that \
         names this log is unknown: {err}",
        path.display()
    )
}

/// The target slug or model an invocation's log name carries, if any.
// §FS-rhei-agents.8.1
fn agent_log_identity(target: Option<&ExecutionTarget>, model: Option<&str>) -> Option<String> {
    target
        .map(ExecutionTarget::slug)
        .or_else(|| model.map(str::to_string).filter(|value| !value.is_empty()))
}

fn resolved_agent_log_identity(resolved: &ResolvedAgent) -> Option<String> {
    agent_log_identity(resolved.target.as_ref(), resolved.model.as_deref())
}

/// The suffix a resolved agent's log carries for a given number. The spawn
/// sites plan from the identity and the resolved number instead.
#[cfg(test)]
fn resolved_agent_log_suffix(resolved: &ResolvedAgent, number: Option<u64>) -> Option<String> {
    agent_log_suffix(resolved.target.as_ref(), resolved.model.as_deref(), number)
}

/// The part of a log file name that follows `task-{task_id}-{state}`:
/// `[{identity}][-{n}]`, with an `n` of 1 left out.
///
/// Split from the resolved-agent form so prompt composition can name the log of
/// an *earlier* visit, where no `ResolvedAgent` for that visit exists — only
/// the identity this one carries, which is the identity that wrote it.
// §FS-rhei-agents.8.1 §FS-rhei-memory.4.4
fn agent_log_suffix(
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    number: Option<u64>,
) -> Option<String> {
    log_suffix(agent_log_identity(target, model).as_deref(), number)
}

/// The grammar's middle, `[{identity}][-{n}]`, from parts already resolved.
/// The identity comes before the number (`opus-2`). §FS-rhei-agents.8.1
fn log_suffix(identity: Option<&str>, number: Option<u64>) -> Option<String> {
    let identity = identity.filter(|value| !value.is_empty());
    let number = number.filter(|count| *count > 1).map(|count| count.to_string());
    match (identity, number) {
        (Some(identity), Some(number)) => Some(format!("{identity}-{number}")),
        (Some(identity), None) => Some(identity.to_string()),
        (None, Some(number)) => Some(number),
        (None, None) => None,
    }
}

/// The log file of one attempt at one entry of a state.
///
/// The first attempt keeps the entry's own `task-{id}-{state}{suffix}.log`
/// name; a re-spawn *within the same visit* appends `-attempt{n}` to it
/// (`…-2-attempt2.log`) instead of truncating the file that says why the
/// attempt before it did not finish. The entry number in `suffix` is what keeps
/// a ticket that leaves and returns from reusing the previous entry's name; the
/// attempt is what keeps a ticket that stalls in place from reusing its own.
// §FS-rhei-agents.8.1
fn agent_log_attempt_path(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
    attempt: u64,
) -> PathBuf {
    let suffix = suffix
        .filter(|value| !value.is_empty())
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let attempt = if attempt > 1 { format!("-attempt{attempt}") } else { String::new() };
    runtime_dir.join("logs").join(format!("task-{task_id}-{state_name}{suffix}{attempt}.log"))
}

/// The log of the *last thing that actually ran* for one invocation — whichever
/// attempt that was — and `None` when nothing did.
///
/// Read from the spawn record rather than by probing names: probing costs one
/// `exists()` per attempt ever made, and it names attempt files a spawn opened
/// and never wrote a line into. The unsuffixed name is still accepted when no
/// record answers, because a runtime written before records existed still has
/// transcripts worth citing, and citing a transcript is not a claim that a
/// worker ran — that claim has one source, and it is the record.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
fn latest_agent_log_path(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
) -> Option<PathBuf> {
    let record = spawn_record_path(runtime_dir, task_id, state_name, suffix);
    if let Some(record) = read_spawn_record(&record) {
        if record.log.exists() {
            return Some(record.log);
        }
    }
    let first = agent_log_attempt_path(runtime_dir, task_id, state_name, suffix, 1);
    first.exists().then_some(first)
}

/// Open a spawn's log for the first time, exclusively: a log is never
/// overwritten. A name already taken is the refusal of §FS-rhei-agents.8.1, said in its
/// own words even when another writer took the name since the plan checked it.
// §FS-rhei-agents.8.1 §FS-rhei-programs.5.1
fn create_log_exclusively(log_path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new().write(true).create_new(true).open(log_path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            unaccounted_log_message(log_path)
        } else {
            format!("failed to create log file '{}': {err}", log_path.display())
        }
    })
}

/// Move aside a session report already at the stem of a log just created
/// exclusively, and say where it went.
///
/// That report cannot belong to this log, which did not exist a moment ago,
/// but it may be the only account left of the session it was rendered from, so
/// it is renamed to `<stem>.orphaned-<unix-ts>.md` untouched rather than
/// overwritten by this session's report (V-D03). Best-effort: a report that
/// cannot be moved is left for `rhei report` to overwrite, as it always has.
// §FS-rhei-session-reports.1
fn rename_orphan_session_report(log_path: &Path) -> Option<String> {
    let runtime_dir = log_path.parent()?.parent()?;
    let stem = log_path.file_stem()?.to_string_lossy();
    let reports = session_reports_dir(runtime_dir);
    let report = reports.join(format!("{stem}.md"));
    if !report.is_file() {
        return None;
    }
    let mut stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let mut aside = reports.join(format!("{stem}.orphaned-{stamp}.md"));
    while aside.exists() {
        stamp += 1;
        aside = reports.join(format!("{stem}.orphaned-{stamp}.md"));
    }
    fs::rename(&report, &aside).ok()?;
    Some(format!(
        "session report {} belongs to no log at that name; renamed it to {}",
        report.display(),
        aside.display()
    ))
}
