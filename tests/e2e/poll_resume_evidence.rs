//! Completed invocation evidence for the isolated resumed-program fixture. §FS-rhei-run.3.5

use super::*;

/// Await both the completed record and its independently appended log footer,
/// then establish that they belong to this fixture's second exit-75 attempt.
/// The first run is stopped before restarting; the fresh execution root and
/// exact worker/log identity exclude other runs and the stale first record.
/// §FS-rhei-run.3.5
pub(super) fn completed_retry_start(workspace: &PollWorkspace) -> u64 {
    let record_path = retry_record_path(workspace);
    let log_path =
        workspace.project.join("runtime/logs/task-triage-tool-reports.1-triage-attempt2.log");
    let mut record = serde_json::Value::Null;
    let mut log = String::new();
    wait_for("completed attempt 2 record and program log footer", || {
        record = fs::read_to_string(&record_path)
            .ok()
            .and_then(|body| serde_json::from_str(&body).ok())
            .unwrap_or_default();
        log = fs::read_to_string(&log_path).unwrap_or_default();
        record["attempt"] == 2
            && record["code"].is_i64()
            && record["ended"].is_string()
            && log.contains("\n=== exit ===\ncode: ")
            && log.ends_with("\n===\n")
    });
    let worker = format!(
        "{} {}",
        super::super::python_command(),
        workspace._root.join("poll-program.py").display()
    );
    for (field, expected) in [
        ("task", serde_json::json!("triage-tool-reports.1")),
        ("state", serde_json::json!("triage")),
        ("moves", serde_json::json!(0)),
        ("attempt", serde_json::json!(2)),
        ("charged", serde_json::json!(2)),
        ("attempt_charged", serde_json::json!(true)),
        ("kind", serde_json::json!("program")),
        ("worker", serde_json::json!(worker)),
        ("log", serde_json::json!(log_path)),
        ("code", serde_json::json!(75)),
        ("ending", serde_json::json!("exited")),
    ] {
        assert_eq!(record[field], expected, "wrong attempt 2 {field}: {record:#}");
    }
    assert!(
        log.starts_with(&format!(
            "=== rhei program log v1 ===\nprogram: {worker}\ntask: triage-tool-reports.1\nstate: triage\n"
        )) && log.contains("\n=== exit ===\ncode: 75\n"),
        "attempt 2 must have the expected worker and actual exit-75 footer: {log}"
    );
    let started = utc_seconds(record["started"].as_str().expect("attempt start timestamp"));
    let ended = utc_seconds(record["ended"].as_str().expect("attempt end timestamp"));
    assert!(ended >= started, "completion precedes start: {record:#}");
    started
}

/// The one record this fixture's task and state keep; each attempt replaces it.
fn retry_record_path(workspace: &PollWorkspace) -> PathBuf {
    workspace.project.join("runtime/spawns/task-triage-tool-reports.1-triage.json")
}

/// When the latest retry started, if the record names one: attempt 2 or any later attempt, since
/// each replaces the record. A stop can end a retry before the exit code and footer
/// [`completed_retry_start`] waits for exist, so this asks for neither. The run publishes a
/// record whole, by rename, once the spawn has ended (§FS-rhei-agents.8.4), so it is absent or
/// complete: one that does not read breaks that promise, and excuses nothing. §FS-rhei-run.3.5
pub(super) fn latest_retry_start(workspace: &PollWorkspace) -> Option<u64> {
    let path = retry_record_path(workspace);
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => panic!("cannot read spawn record {}: {err}", path.display()),
    };
    let record: serde_json::Value = serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("a spawn record is published whole: {err}\n{body}"));
    record["attempt"]
        .as_u64()
        .is_some_and(|attempt| attempt >= 2)
        .then(|| utc_seconds(record["started"].as_str().expect("attempt start timestamp")))
}

/// Publish a completed attempt that no process made, for a negative control: its log and spawn
/// record, in the form [`completed_retry_start`] reads. No process is launched. §FS-rhei-run.3.5
pub(super) fn write_synthetic_attempt(workspace: &PollWorkspace, attempt: u64, started: &str) {
    let records = workspace.project.join("runtime/spawns");
    fs::create_dir_all(&records).expect("create synthetic record directory");
    let log = workspace
        .project
        .join(format!("runtime/logs/task-triage-tool-reports.1-triage-attempt{attempt}.log"));
    let worker = format!(
        "{} {}",
        super::super::python_command(),
        workspace._root.join("poll-program.py").display()
    );
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    fs::write(&log, format!("=== rhei program log v1 ===\nprogram: {worker}\ntask: triage-tool-reports.1\nstate: triage\n\n=== exit ===\ncode: 75\n===\n"))
        .unwrap();
    let record = serde_json::json!({
        "task": "triage-tool-reports.1", "state": "triage", "moves": 0,
        "attempt": attempt, "charged": attempt, "attempt_charged": true,
        "kind": "program", "worker": worker, "log": log,
        "started": started, "ended": "2020-01-01T00:00:11Z",
        "duration": "2s", "code": 75, "ending": "exited"
    });
    fs::write(records.join("task-triage-tool-reports.1-triage.json"), record.to_string()).unwrap();
}

/// Parse the shipped whole-second UTC format without using interpreter-entry
/// time. In this fixture, find_ready_tasks gates collect_ready_program_work_items
/// before spawn_and_wait_program samples started_wall, which precedes creation.
/// With a nondecreasing wall clock and an integer deadline D, a lawful attempt
/// therefore has floor(started_wall) >= D: truncation cannot cross D backwards.
/// A smaller sample violates this readiness-to-start ordering; it is not by
/// itself an exact kernel creation timestamp. §FS-rhei-run.3.5
fn utc_seconds(timestamp: &str) -> u64 {
    assert!(
        timestamp.len() == 20 && timestamp.ends_with('Z'),
        "expected whole-second UTC timestamp: {timestamp}"
    );
    let parts: Vec<u32> = timestamp
        .trim_end_matches('Z')
        .split(['-', 'T', ':'])
        .map(|part| part.parse().expect("numeric UTC timestamp component"))
        .collect();
    assert_eq!(parts.len(), 6, "UTC timestamp components");
    let date = time::Date::from_calendar_date(
        parts[0].try_into().unwrap(),
        time::Month::try_from(u8::try_from(parts[1]).unwrap()).unwrap(),
        parts[2].try_into().unwrap(),
    )
    .expect("valid UTC date");
    let clock = time::Time::from_hms(
        parts[3].try_into().unwrap(),
        parts[4].try_into().unwrap(),
        parts[5].try_into().unwrap(),
    )
    .expect("valid UTC time");
    time::PrimitiveDateTime::new(date, clock)
        .assume_utc()
        .unix_timestamp()
        .try_into()
        .expect("timestamp after Unix epoch")
}
