// What a taken log name is when a run killed outright may account for it: the
// one place evidence is read to decide something.
//
// Its own part because it reads the evidence store to answer a planning
// question, which neither the planner nor the store does anywhere else. It
// writes nothing: a spawn site acts on its verdict, and a reading predicts.

// §AR-source-file-size.3 §FS-rhei-evidence.11 §FS-rhei-agents.8.1.1

use rhei_core::platform::worker_liveness::{LiveProcess, WorkerLiveness};

/// The `ending` of an attempt killed together with its run. §FS-rhei-agents.8.4
const KILLED_ENDING: &str = "killed";

/// What the evidence of runs killed outright says about a planned log name
/// that is taken and that no spawn record accounts for: a killed attempt to
/// resume (§FS-rhei-agents.8.1.1), or a refusal's case (§FS-rhei-evidence.11.3).
#[derive(Debug)]
enum TakenLog {
    /// A killed attempt of this visit: every condition of §FS-rhei-evidence.11.1
    /// holds. Its start record's `started_at`.
    Killed { started: String },
    /// A program whose run was killed while it ran: only the `kind` fails.
    Program,
    /// The killed worker's group is not gone, and these are what is left in it.
    StillRunning(Vec<LiveProcess>),
    /// Everything holds but the group record, so nothing says the agent stopped.
    NoGroupRecord,
    /// No record of a killed run accounts for it.
    Unaccounted,
}

impl TakenLog {
    /// Which verdict wins when two start records name the same log; a live
    /// group is decided before any of these. §FS-rhei-evidence.11.3
    fn rank(&self) -> u8 {
        match self {
            TakenLog::StillRunning(_) => 4,
            TakenLog::Killed { .. } => 3,
            TakenLog::Program => 2,
            TakenLog::NoGroupRecord => 1,
            TakenLog::Unaccounted => 0,
        }
    }

    /// The refusal of a spawn whose log `log` is taken, in the case this
    /// verdict names. `transitions` are the program case's `rhei transition`
    /// lines; a reading, which prints none, passes none. §FS-rhei-evidence.11.3
    fn refusal(
        &self,
        log: &Path,
        task_id: &str,
        reset: Option<&ResetOffer>,
        transitions: &[String],
    ) -> String {
        let log_shown = log.display();
        match self {
            // No `kill` line, no move, no reset: each would start a second
            // worker beside the first. §DA-supervised-process-groups
            TakenLog::StillRunning(processes) => {
                let listed = processes.iter().fold(String::new(), |mut listed, process| {
                    use std::fmt::Write as _;
                    let _ = write!(listed, "\n    pid {}: {}", process.pid, process.command);
                    listed
                });
                format!(
                    "refusing to spawn: {log_shown} belongs to a worker whose run was killed, and \
                     its process group still has processes{listed}\n  help: stop them if they \
                     are the killed worker's, then run again; the visit resumes once the group \
                     is gone"
                )
            }
            TakenLog::Program => {
                let or_reset = reset
                    .map(|reset| format!(", or run {}", reset.for_ticket(task_id)))
                    .unwrap_or_default();
                let mut message = format!(
                    "refusing to spawn: {log_shown} belongs to a program whose run was killed \
                     while it ran; it may have finished"
                );
                if !transitions.is_empty() {
                    message.push_str("\n  help: if it did, move the ticket on with one of:");
                    for line in transitions {
                        message.push('\n');
                        message.push_str(line);
                    }
                    message.push_str("\n  if not,");
                } else {
                    message.push_str("\n  help:");
                }
                message.push_str(&format!(" move that file away{or_reset}"));
                message
            }
            TakenLog::NoGroupRecord => {
                let (first, help) = unaccounted_log_message(log, task_id, reset)
                    .split_once('\n')
                    .map(|(first, help)| (first.to_string(), help.to_string()))
                    .unwrap_or_default();
                format!(
                    "{first}; its run was killed, but rhei cannot tell whether that agent has \
                     stopped\n{help}"
                )
            }
            TakenLog::Killed { .. } | TakenLog::Unaccounted => {
                unaccounted_log_message(log, task_id, reset)
            }
        }
    }
}

/// One start record of a killed run that names the planned log: what its run's
/// manifest recorded that log's hash as, and what its group record says.
struct KilledStart {
    record: serde_json::Value,
    recorded_sha256: Option<String>,
    /// `None` where there is no group record, or none that names a worker.
    group: Option<WorkerLiveness>,
}

/// Sort the taken log of `plan` by the records of runs killed outright, under
/// the conditions of §FS-rhei-evidence.11.1. A spawn site and every reading
/// that writes nothing ask this one function, so a reading never predicts a
/// resume the spawn site would refuse, or the other way round.
///
/// `program` says whether the state runs a program, for the `kind` condition.
// §FS-rhei-evidence.11.1 §FS-rhei-agents.8.1.1 §FS-rhei-evidence.11.3
fn classify_taken_log(
    plan: &SpawnPlan,
    task_root: &Path,
    task_id: &str,
    state_name: &str,
    program: bool,
) -> TakenLog {
    let starts = killed_starts(&evidence_root_for(task_root), &plan.log);
    // A live group wins over every other case, a program and a changed log
    // among them. §FS-rhei-evidence.11.3
    let running: Vec<&Vec<LiveProcess>> = starts
        .iter()
        .filter_map(|start| match &start.group {
            Some(WorkerLiveness::Running(processes)) => Some(processes),
            _ => None,
        })
        .collect();
    if !running.is_empty() {
        return TakenLog::StillRunning(running.into_iter().flatten().cloned().collect());
    }
    let Ok(bytes) = fs::read(&plan.log) else { return TakenLog::Unaccounted };
    let found = sha256_hex(&bytes);
    starts
        .iter()
        .map(|start| start.verdict(plan, task_id, state_name, program, &found))
        .max_by_key(TakenLog::rank)
        .unwrap_or(TakenLog::Unaccounted)
}

impl KilledStart {
    /// What this one start record says, once its group is known to be gone or
    /// unrecorded. §FS-rhei-evidence.11.1
    fn verdict(
        &self,
        plan: &SpawnPlan,
        task_id: &str,
        state_name: &str,
        program: bool,
        found_sha256: &str,
    ) -> TakenLog {
        let record = &self.record;
        // Condition 4: the spawn's task, state, move count and attempt.
        let attempt = record["invocation_id"].as_str().and_then(invocation_attempt_parts);
        let same_spawn = record["task_id"].as_str() == Some(task_id)
            && record["state"].as_str() == Some(state_name)
            && attempt.is_some_and(|(_, moves, attempt)| {
                moves == plan.moves && attempt == plan.attempt
            });
        // Condition 5: the log is what finalization hashed.
        let unchanged = self.recorded_sha256.as_deref() == Some(found_sha256);
        if !same_spawn || !unchanged {
            return TakenLog::Unaccounted;
        }
        // Condition 3, and condition 6 now that no group is running.
        match (record["kind"].as_str(), program, &self.group) {
            (Some("agent"), false, Some(WorkerLiveness::Gone)) => TakenLog::Killed {
                started: record["started_at"].as_str().unwrap_or_default().to_string(),
            },
            (Some("program"), true, Some(WorkerLiveness::Gone)) => TakenLog::Program,
            (Some("agent"), false, None) => TakenLog::NoGroupRecord,
            _ => TakenLog::Unaccounted,
        }
    }
}

/// Every start record that names `log`, has no end record, and belongs to a
/// run whose manifest says `incomplete`, whichever run finalized it: the
/// candidates of conditions 1, 2 and the `log_path` half of 4. A live run's
/// directory has no manifest yet, so nothing it is doing is a candidate.
// §FS-rhei-evidence.11.1 §FS-rhei-evidence.9.3
fn killed_starts(root: &Path, log: &Path) -> Vec<KilledStart> {
    let canonical = |path: &Path| {
        rhei_core::platform::canonical_path(path).unwrap_or_else(|_| path.to_path_buf())
    };
    let planned = canonical(log);
    let read = |path: &Path| -> Option<serde_json::Value> {
        serde_json::from_slice(&fs::read(path).ok()?).ok()
    };
    let Ok(runs) = fs::read_dir(evidence_dir(root).join("runs")) else { return Vec::new() };
    let mut found = Vec::new();
    for run in runs.flatten() {
        let dir = run.path();
        let Some(manifest) = read(&dir.join("manifest.json")) else { continue };
        if manifest["status"].as_str() != Some("incomplete") {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(key) = name.to_str().and_then(|name| name.strip_suffix(".start.json")) else {
                continue;
            };
            if dir.join(format!("{key}.end.json")).exists() {
                continue;
            }
            let Some(record) = read(&entry.path()) else { continue };
            let Some(log_path) = record["log_path"].as_str() else { continue };
            if canonical(&evidence_join(root, log_path)) != planned {
                continue;
            }
            let recorded_sha256 = manifest["files"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|file| file["path"].as_str() == Some(log_path))
                .and_then(|file| file["sha256"].as_str())
                .map(str::to_string);
            let group = read(&dir.join(format!("{key}.group.json")))
                .filter(|group| group["invocation_id"] == record["invocation_id"])
                .and_then(|group| recorded_worker_liveness(&group));
            found.push(KilledStart { record, recorded_sha256, group });
        }
    }
    found
}

/// Whether the worker a group record names is gone: its group, read with
/// signal `0`. A record that names no usable group is no group record at all.
// §FS-rhei-evidence.11.2
#[cfg(unix)]
fn recorded_worker_liveness(group: &serde_json::Value) -> Option<WorkerLiveness> {
    // Group 0 is the caller's own and 1 is init's: neither is a worker's.
    let pgid = i32::try_from(group["process_group"].as_u64()?).ok().filter(|pgid| *pgid > 1)?;
    Some(rhei_core::platform::worker_liveness::group_liveness(pgid))
}

/// Whether the agent a group record names is gone: its pid, held to the
/// creation time the record wrote beside it. §FS-rhei-evidence.11.2
#[cfg(windows)]
fn recorded_worker_liveness(group: &serde_json::Value) -> Option<WorkerLiveness> {
    let pid = u32::try_from(group["pid"].as_u64()?).ok()?;
    let created_at = group["created_at"].as_u64()?;
    Some(rhei_core::platform::worker_liveness::process_liveness(pid, created_at))
}
