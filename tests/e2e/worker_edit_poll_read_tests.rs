//! Controlled harness faults in the actual shared-file fixture, rather than
//! reproduction of the unobserved Windows exception. §FS-rhei-run.3.7.3.1

use std::fs;
use std::process::Command;

use super::python_fixture::python_command;
use super::worker_edit_one_file_support::OneFileScenario;
use super::worker_edit_revert_support::Mode;
use super::*;

fn scenario(fault: &str) -> OneFileScenario {
    OneFileScenario::new(
        &format!("worker-edit-poll-{fault}"),
        Mode::Parallel,
        &format!("_fault_mode = '{fault}'\n{}", include_str!("fixtures/worker_edit_poll_fault.py")),
    )
}

fn events(scenario: &OneFileScenario) -> Vec<serde_json::Value> {
    fs::read_to_string(scenario.dir.join("read-events.jsonl"))
        .expect("fault instrumentation ran")
        .lines()
        .map(|line| serde_json::from_str(line).expect("fault event JSON"))
        .collect()
}

fn run_parallel(scenario: &OneFileScenario) -> CliRun {
    run_cli(
        "run",
        &scenario.ws,
        &scenario.machine,
        &["--no-tui", "--no-callbacks", "--parallel", "2"],
    )
}

// §FS-rhei-run.3.7.3.1 §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.7
#[test]
fn a_refused_poll_still_reaches_the_concurrent_restore_and_charged_retry() {
    let scenario = scenario("transient");
    let ran = run_parallel(&scenario);
    let output = scenario.diagnostics(&ran);
    let events = events(&scenario);
    let refused: Vec<_> =
        events.iter().enumerate().filter(|(_, e)| e["event"] == "poll-refused").collect();
    assert_eq!(refused.len(), 1, "exactly one injected polling refusal: {events:?}\n{output}");
    let refusal = refused[0].0;
    if !ran.status.success() {
        let log = fs::read_to_string(scenario.ws.join("runtime/logs/task-ws.1-cover.log"))
            .expect("culprit program log");
        assert!(log.contains("PermissionError"), "unexpected fixture failure:\n{log}");
        assert!(log.contains("rhei-623 controlled polling refusal"), "{log}");
        assert!(!events.iter().any(|e| e["event"] == "edit"), "{events:?}\n{log}");
        scenario.assert_sibling_preserved(&output);
    }
    // The same assertions cover finalized result contents/link, one sibling
    // execution, later dependency/completion, scoped restore and both charges.
    scenario.assert_completed(&ran);
    assert!(
        events[refusal + 1..].iter().any(|e| e["event"] == "poll-read"),
        "a successful poll follows the refusal: {events:?}\n{output}"
    );
    assert!(events.iter().any(|e| e["event"] == "edit-read"), "{events:?}\n{output}");
}

// §FS-rhei-run.3.7.3.1
#[test]
fn diagnostics_retain_an_independent_failing_programs_traceback() {
    let scenario = scenario("diagnostics");
    let ran = run_parallel(&scenario);
    let output = scenario.diagnostics(&ran);
    assert_eq!(ran.status.code(), Some(1), "the independent failure stays failed\n{output}");
    let log = fs::read_to_string(scenario.ws.join("runtime/logs/task-ws.1-cover.log"))
        .expect("actual program log");
    for needle in ["Traceback (most recent call last)", "rhei-623 independent diagnostic fault"] {
        assert!(log.contains(needle), "independent fault reached its program log:\n{log}");
    }
    for needle in [
        "runtime/logs/task-ws.1-cover.log:",
        "Traceback (most recent call last)",
        "rhei-623 independent diagnostic fault",
    ] {
        assert!(output.contains(needle), "diagnostics omitted {needle:?}\n{output}");
    }
}

/// Run the same cover.py directly: the fault never reaches publication, and a
/// private virtual clock bounds the wait without controlling engine timing.
fn run_focused(scenario: &OneFileScenario) -> std::process::Output {
    let mut command = Command::new(python_command());
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"RHEI_") {
            command.env_remove(key);
        }
    }
    command
        .arg(scenario.dir.join("cover.py"))
        .arg("concurrent")
        .env("RHEI_TASK_ID_LOCAL", "1")
        .output()
        .expect("run actual ONE_FILE_COVER with a virtual polling clock")
}

// §FS-rhei-run.3.7.3.1
#[test]
fn a_permanent_polling_refusal_expires_with_path_and_last_error() {
    let scenario = scenario("permanent");
    let ran = run_focused(&scenario);
    let error = stderr(&ran);
    assert!(!ran.status.success(), "a permanent refusal must fail");
    let events = events(&scenario);
    let finished = events.last().expect("virtual clock report");
    assert_eq!(finished["event"], "finished", "{events:?}");
    let elapsed = finished["elapsed"].as_f64().expect("elapsed seconds");
    assert!(
        (15.0..=16.0).contains(&elapsed),
        "refusal must retry to the existing 15-second deadline; elapsed={elapsed}\n{error}"
    );
    assert!(finished["reads"].as_u64().unwrap() > 1, "{events:?}");
    assert!(error.contains("timed out") || error.contains("timeout"), "{error}");
    assert!(error.contains("rhei-623 controlled polling refusal"), "{error}");
    assert!(
        error.contains(&scenario.ws.join("tasks/01-shared.md").display().to_string()),
        "{error}"
    );
    assert!(
        !error.contains("fault injector:"),
        "instrumentation guard, not fixture bound\n{error}"
    );
    assert!(!events.iter().any(|e| e["event"] == "edit"), "{events:?}");
}

// §FS-rhei-run.3.7.3.1
#[test]
fn an_unrelated_polling_error_is_not_retried() {
    let scenario = scenario("unrelated");
    let ran = run_focused(&scenario);
    let error = stderr(&ran);
    assert!(!ran.status.success(), "unrelated errors stay failed");
    assert!(error.contains("OSError: [Errno 5] rhei-623 unrelated I/O fault"), "{error}");
    let events = events(&scenario);
    assert_eq!(events.len(), 2, "one fault and the clock report: {events:?}");
    assert_eq!(events[0]["event"], "poll-unrelated", "{events:?}");
    assert_eq!(events[1]["event"], "finished", "{events:?}");
    assert_eq!(events[1]["reads"], 1, "unrelated errors propagate immediately: {events:?}");
    assert_eq!(events[1]["elapsed"], 0.0, "no retry sleep: {events:?}");
}
