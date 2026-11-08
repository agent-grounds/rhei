//! The real failed-own-record scenario, optionally run under controlled startup
//! schedules. The scheduler wraps the agent; it never changes its exit policy.
//! §FS-rhei-agent-visit-pairing.2 §FS-rhei-agent-visit-pairing.5

use super::*;
use serde_json::Value;

/// Port of #619's interpreter shim. Private files coordinate actual startup;
/// the one-second delay deliberately makes c slow, rather than fixing a race.
/// A prerequisite timeout has a distinct exit code and diagnostic.
const STARTUP_SCHEDULER: &str = r#"import json
import os
from pathlib import Path
import runpy
import sys
import time

root = Path(__file__).parent
runtime = root / 'runtime'
order = (root / 'startup-order').read_text()
target = os.environ.get('RHEI_TARGET', '')


def wait_for(description, predicate):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.005)
    print('startup prerequisite timed out: ' + description, file=sys.stderr)
    sys.exit(97)


def c_succeeded():
    path = runtime / 'spawns/task-plan.1-work-mock-mock-c-2.json'
    try:
        record = json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return False
    return (record.get('task') == 'plan.1' and record.get('state') == 'work'
            and record.get('moves') == 2 and record.get('ending') == 'exited'
            and record.get('code') == 0)


if target == 'mock:mock:c':
    (runtime / 'scheduled-c-started').write_text(str(os.getpid()))
    if order == 'd-first':
        wait_for('d dispatched', lambda: (runtime / 'scheduled-d-dispatched').exists())
        time.sleep(1)
elif target == 'mock:mock:d':
    if order == 'd-first':
        wait_for('c started', lambda: (runtime / 'scheduled-c-started').exists())
    else:
        wait_for('c successful current-visit record', c_succeeded)
    (runtime / 'scheduled-d-dispatched').write_text(str(os.getpid()))
try:
    runpy.run_path(str(root / 'mock-agent.py'), run_name='__main__')
except SystemExit as outcome:
    if target == 'mock:mock:d' and outcome.code == 1:
        try:
            c_record = json.loads((runtime / 'spawns/task-plan.1-work-mock-mock-c-2.json').read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            c_record = None
        (runtime / 'c-at-d-exit.json').write_text(json.dumps(c_record))
    raise
"#;

fn records(fixture: &Finished) -> [Value; 3] {
    ["c", "d", "x"].map(|name| {
        let path = fixture.record(&format!("mock-mock-{name}"));
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let record: Value = serde_json::from_str(&text).expect("current-visit JSON record");
        assert_eq!(record["task"], "plan.1", "{text}");
        assert_eq!(record["state"], "work", "{text}");
        assert_eq!(record["moves"], 2, "{text}");
        record
    })
}

fn assert_ending(record: &Value, code: i32) {
    assert_eq!(record["ending"], "exited", "{record}");
    assert_eq!(record["code"], code, "{record}");
}

fn assert_c_finished_before_d_exit(fixture: &Finished, successful_c: &Value) {
    let snapshot = fs::read_to_string(fixture.dir.join("runtime/c-at-d-exit.json"))
        .expect("scheduler observes c's record at d's deliberate exit, without waiting");
    let snapshot: Value = serde_json::from_str(&snapshot).expect("c record at d exit");
    assert_eq!(&snapshot, successful_c, "c had already finished when d deliberately exited");
}

pub(super) fn check_failed_own_record(order: Option<&str>) {
    let fixture = finished_then_died(
        "agent-target-edit-own-failed",
        r#"all_targets: ["mock:mock:x"]"#,
        Entry::Reentry,
    );
    let agent = write_python_agent(&fixture.dir, "mock-agent.py", D_FAILS_AGENT);
    write_settings(&fixture.dir, &agent, false);
    if let Some(order) = order {
        fs::write(fixture.dir.join("startup-order"), order).expect("private startup schedule");
        let wrapper = fixture.dir.join("startup-scheduler.py");
        fs::write(&wrapper, STARTUP_SCHEDULER).expect("private interpreter scheduler");
        write_settings(&fixture.dir, &wrapper, false);
    }
    fs::write(&fixture.machine, machine(r#"all_targets: ["mock:mock:c", "mock:mock:d"]"#))
        .expect("edit the target in place");
    let args = ["--no-tui", "--no-callbacks"];
    let first = run_cli("run", &fixture.plan, &fixture.machine, &args);
    let first_records = records(&fixture);
    let journal = fs::read_to_string(fixture.dir.join("runtime/transitions.log")).expect("journal");
    eprintln!("schedule={order:?}\nrecords c/d/x: {first_records:#?}\njournal:\n{journal}");
    for name in ["c", "d"] {
        let log = fixture.record_name(&format!("mock-mock-{name}")).replace(".json", ".log");
        assert!(
            journal.lines().any(|line| line.contains("start@work") && line.ends_with(&log)),
            "both invocations must really start: {journal}"
        );
    }
    assert_ending(&first_records[1], 1);
    assert_ending(&first_records[2], 0);
    if order.is_some() {
        assert!(fixture.dir.join("runtime/scheduled-c-started").exists(), "c interpreter started");
        assert!(fixture.dir.join("runtime/scheduled-d-dispatched").exists(), "d dispatched");
    }
    let mut spawned = spawn_lines(&fixture.dir).split_off(fixture.spawns_before_edit);
    spawned.sort();
    spawned.dedup();
    assert_eq!(spawned, ["mock:mock:c", "mock:mock:d"], "an ambiguous edit spawns both");
    assert_ending(&first_records[0], 0);
    if order.is_some() {
        assert_c_finished_before_d_exit(&fixture, &first_records[0]);
    }
    assert!(first.stderr.contains("warning"), "{}", first.stderr);
    assert!(first.stderr.contains(&fixture.record_name("mock-mock-x")), "{}", first.stderr);
    assert!(
        first.stderr.contains("mock:mock:c, mock:mock:d have no spawn record of their own"),
        "{}",
        first.stderr
    );
    assert!(reuse_notes(&first).is_empty(), "ambiguous: nothing is paired:\n{}", first.stdout);
    let state = || fs::read_to_string(&fixture.plan).expect("plan");
    assert!(state().contains("**State:** work"), "d failed, so the ticket stays in work");

    let before_second = spawn_lines(&fixture.dir).len();
    let second = run_cli("run", &fixture.plan, &fixture.machine, &args);
    let respawned = spawn_lines(&fixture.dir).split_off(before_second);
    assert_eq!(
        respawned,
        ["mock:mock:d"],
        "only d's own failed record needs a retry:\nstdout:\n{}\nstderr:\n{}",
        second.stdout,
        second.stderr
    );
    assert!(reuse_notes(&second).is_empty(), "x must not answer for d:\n{}", second.stdout);
    assert!(state().contains("**State:** work"), "x's work must not advance the ticket");
    let second_records = records(&fixture);
    assert_eq!(second_records[0], first_records[0], "successful c is not rerun");
    assert_eq!(second_records[2], first_records[2], "successful orphan x remains untouched");
    assert_ending(&second_records[1], 1);
    if order.is_some() {
        assert_c_finished_before_d_exit(&fixture, &second_records[0]);
    }
    assert_eq!(second_records[1]["attempt"], 2, "d really retried in this visit");
    assert_ne!(second_records[1]["log"], first_records[1]["log"], "d has a fresh attempt log");
}
