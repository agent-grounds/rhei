//! Controlled faults in d's three reads of c's spawn record - d's poll, the
//! scheduler's `c_succeeded()` and its snapshot at d's deliberate exit - in the
//! actual fixtures, rather than reproduction of the unobserved Windows refusal.
//! §FS-rhei-agent-visit-pairing.5.1

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::failure_support::{check_failed_own_record_with, STARTUP_SCHEDULER};
use super::python_fixture::python_command;
use super::D_FAILS_AGENT;
use super::*;

const C_RECORD: &str = "task-plan.1-work-mock-mock-c-2.json";

/// Each read, named by the phase of d's interpreter it happens in, and the
/// startup order under which that read is the one that decides.
const SITES: [(&str, &str); 3] =
    [("scheduler", "c-first"), ("poll", "d-first"), ("snapshot", "d-first")];

fn fault_prefix(site: &str, kind: &str) -> String {
    format!(
        "_fault_site = '{site}'\n_fault_kind = '{kind}'\n{}",
        include_str!("fixtures/agent_target_edit_read_fault.py")
    )
}

fn events(dir: &Path) -> Vec<Value> {
    fs::read_to_string(dir.join("read-fault-events.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("fault event JSON"))
        .collect()
}

fn named<'a>(events: &'a [Value], event: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["event"] == event).collect()
}

/// The whole failed-own-record scenario, with one refusal injected into each
/// of d's interpreters at `site`: it must still pass, snapshot included.
fn check_transient(site: &str, order: &str) {
    let inspect = |dir: &Path| {
        let events = events(dir);
        let escaped = named(&events, "escaped");
        assert!(
            escaped.is_empty(),
            "the injected refusal at {site} escaped d's fixture instead of being read again, \
             so d exited 1 without its deliberate SystemExit: {escaped:?}\nevents: {events:?}"
        );
        assert!(!named(&events, "refused").is_empty(), "the {site} read was refused: {events:?}");
    };
    check_failed_own_record_with(Some(order), &fault_prefix(site, "transient"), &inspect);
}

// §FS-rhei-agent-visit-pairing.5.1
#[test]
fn a_refused_scheduler_read_of_cs_record_still_dispatches_d() {
    check_transient("scheduler", "c-first");
}

// §FS-rhei-agent-visit-pairing.5.1
#[test]
fn a_refused_poll_of_cs_record_still_reaches_ds_deliberate_exit() {
    check_transient("poll", "d-first");
}

// §FS-rhei-agent-visit-pairing.5.1
#[test]
fn a_refused_snapshot_read_still_records_c_at_ds_exit() {
    check_transient("snapshot", "d-first");
}

/// Run the actual wrapper and agent as d directly, with c's successful record
/// already on disk; the injector's virtual clock bounds the wait.
fn run_focused(site: &str, order: &str, kind: &str) -> (TestDir, std::process::Output) {
    let dir = unique_temp_dir(&format!("agent-target-edit-read-{kind}-{site}"));
    let spawns = dir.join("runtime").join("spawns");
    fs::create_dir_all(&spawns).expect("spawn records directory");
    let c_record =
        r#"{"task": "plan.1", "state": "work", "moves": 2, "ending": "exited", "code": 0}"#;
    fs::write(spawns.join(C_RECORD), c_record).expect("c's successful record");
    fs::write(dir.join("runtime").join("scheduled-c-started"), "1").expect("c started");
    fs::write(dir.join("startup-order"), order).expect("startup order");
    write_python_agent(&dir, "mock-agent.py", D_FAILS_AGENT);
    let wrapper = dir.join("startup-scheduler.py");
    fs::write(&wrapper, format!("{}{STARTUP_SCHEDULER}", fault_prefix(site, kind)))
        .expect("scheduler under fault injection");
    let mut command = Command::new(python_command());
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"RHEI_") {
            command.env_remove(key);
        }
    }
    let ran = command
        .arg(&wrapper)
        .current_dir(&*dir)
        .env("RHEI_TARGET", "mock:mock:d")
        .env("RHEI_ROOT", &*dir)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run the actual scheduler and agent as d");
    (dir, ran)
}

/// What is wrong with one site's permanent refusal, or nothing.
fn permanent_problems(site: &str, order: &str) -> Vec<String> {
    let (dir, ran) = run_focused(site, order, "permanent");
    let error = stderr(&ran);
    let events = events(&dir);
    let path = dir.join("runtime").join("spawns").join(C_RECORD).display().to_string();
    let finished = named(&events, "finished");
    let elapsed = finished.last().and_then(|e| e["elapsed"].as_f64()).unwrap_or(-1.0);
    let reads = finished.last().and_then(|e| e["reads"].as_u64()).unwrap_or(0);
    let mut problems = Vec::new();
    let mut expect = |ok: bool, what: &str| {
        if !ok {
            problems.push(format!("{site}: {what}"));
        }
    };
    expect(ran.status.code() == Some(97), &format!("exit 97, got {:?}", ran.status.code()));
    expect(named(&events, "escaped").is_empty(), "the refusal escaped the fixture");
    expect((5.0..=6.0).contains(&elapsed), &format!("retried to the 5s bound, elapsed={elapsed}"));
    expect(reads > 1, &format!("read again after a refusal, reads={reads}"));
    expect(error.contains("timed out"), "says it timed out");
    expect(error.contains(&path), "names c's record path");
    expect(error.contains("rhei-695 controlled refusal"), "names the last refusal");
    expect(!error.contains("fault injector:"), "ended at the fixture's bound, not the injector's");
    expect(!dir.join("runtime/c-at-d-exit.json").exists(), "no snapshot of a refused read");
    if !problems.is_empty() {
        problems.push(format!("{site} stderr:\n{error}\nevents: {events:?}"));
    }
    problems
}

// §FS-rhei-agent-visit-pairing.5.1
#[test]
fn a_permanent_refusal_of_any_read_expires_naming_the_path_and_last_error() {
    let problems: Vec<String> =
        SITES.iter().flat_map(|(site, order)| permanent_problems(site, order)).collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// §FS-rhei-agent-visit-pairing.5.1: a guard against widening the retry; it passes today.
#[test]
fn an_unrelated_error_in_any_read_is_not_retried() {
    for (site, order) in SITES {
        let (dir, ran) = run_focused(site, order, "unrelated");
        let error = stderr(&ran);
        let events = events(&dir);
        assert_ne!(ran.status.code(), Some(0), "{site}: an unrelated error fails\n{error}");
        assert_ne!(ran.status.code(), Some(97), "{site}: not retried to the bound\n{error}");
        assert!(
            error.contains("OSError: [Errno 5] rhei-695 unrelated I/O fault"),
            "{site}: {error}"
        );
        assert_eq!(named(&events, "unrelated").len(), 1, "{site}: {events:?}");
        let finished = named(&events, "finished");
        let finished = finished.last().expect("virtual clock report");
        assert_eq!(finished["reads"], 1, "{site}: propagated at once: {events:?}");
        assert_eq!(finished["elapsed"], 0.0, "{site}: no retry sleep: {events:?}");
    }
}
