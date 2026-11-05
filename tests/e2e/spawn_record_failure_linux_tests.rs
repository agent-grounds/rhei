//! The intake's R and L variants: only publication fails; every other write works.
//! The Linux shim also pins staging and replacement failures without permissions.
//! §FS-rhei-agents.8.4 §FS-rhei-agents.8.1 §FS-rhei-run.3

use super::spawn_record_failure_support::{assert_refusal_halt, text, Fixture};
use super::*;

fn injected_run(fixture: &Fixture, mode: &str) -> CliRun {
    let shim = fixture.dir.join("fault.so");
    let compiled = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-O1", "-o"])
        .arg(&shim)
        .arg(fixture_path("spawn-record-failures.c"))
        .arg("-ldl")
        .output()
        .expect("Linux fault fixture requires cc");
    assert!(compiled.status.success(), "{}", stderr(&compiled));
    let out = fixture
        .command()
        .env("LD_PRELOAD", shim)
        .env("SPAWN_FAULT_ROOT", &*fixture.dir)
        .env("SPAWN_FAULT_MODE", mode)
        .output()
        .expect("injected run");
    let out = CliRun::from(&out);
    fixture.assert_dispatch(&out);
    assert_eq!(
        fs::read_to_string(fixture.dir.join("fault-hit")).unwrap_or_default(),
        mode,
        "fixture must hit the intended syscall before testing diagnostics:\n{}",
        text(&out)
    );
    out
}

fn one_shot(agent: bool, parallel: &'static str, mode: &str) {
    let fixture = Fixture::new(agent, parallel, "shim", 0, false);
    let out = injected_run(&fixture, mode);
    fixture.assert_worker(&out, 0, true);
    fixture.assert_warning(
        &out,
        if mode == "replace" {
            "Is a directory (os error 21)"
        } else {
            "No space left on device (os error 28)"
        },
    );
}

macro_rules! write_contract {
    ($name:ident, $agent:expr, $parallel:expr) => {
        #[test]
        fn $name() {
            one_shot($agent, $parallel, "write");
        }
    };
}

write_contract!(program_serial_write, false, "1");
write_contract!(program_parallel_write, false, "2");
write_contract!(agent_serial_write, true, "1");
write_contract!(agent_parallel_write, true, "2");

#[test]
fn completion_record_stage_failure_is_observable() {
    one_shot(false, "1", "stage");
}

#[test]
fn completion_record_replace_failure_is_observable() {
    one_shot(false, "1", "replace");
}

fn polling(parallel: &'static str, warning: bool) {
    polling_at_dispatch(parallel, warning, false);
}

fn polling_at_dispatch(parallel: &'static str, warning: bool, refill: bool) {
    let fixture = Fixture::at_dispatch(false, parallel, "shim", 75, true, refill);
    let out = injected_run(&fixture, "write");
    fixture.assert_worker(&out, 75, false);
    assert!(text(&out).contains("refusing to spawn:"), "{}", text(&out));
    let bytes = fs::read(fixture.log()).unwrap();
    // With the fault cleared, neither the next run nor the same-run retry may
    // derive an attempt from the completed log. §FS-rhei-agents.8.1
    fixture.clear_fault("shim");
    let rerun = fixture.run();
    fixture.assert_worker(&rerun, 75, false);
    assert_eq!(fs::read(fixture.log()).unwrap(), bytes);
    if warning {
        fixture.assert_warning(&out, "No space left on device (os error 28)");
    } else {
        assert_refusal_halt(&out);
    }
}

macro_rules! poll_contract {
    ($warning:ident, $halt:ident, $parallel:expr) => {
        #[test]
        fn $warning() {
            polling($parallel, true);
        }

        #[test]
        fn $halt() {
            polling($parallel, false);
        }
    };
}

poll_contract!(poll_serial_write_warning, poll_serial_refusal_halt, "1");
poll_contract!(poll_parallel_write_warning, poll_parallel_refusal_halt, "2");

#[test]
fn poll_program_pool_refill_write_warning_and_halt() {
    polling_at_dispatch("2", true, true);
    polling_at_dispatch("2", false, true);
}

// The writer wins only at O_EXCL, after successful planning. No target worker
// or successful sibling can produce a later preflight that hides a lost
// completion-path refusal. §FS-rhei-agents.8.1 §FS-rhei-run.3
fn exclusive_open_refusal(agent: bool, parallel: &'static str, json: bool) {
    let fixture = Fixture::new(agent, parallel, "race", 0, false);
    if json {
        write_fixture_file(&fixture.dir, "json-output", "yes");
    }
    let out = injected_run(&fixture, "race");
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(!fixture.dir.join("calls").exists(), "refused worker must never start");
    assert!(!fixture.record().exists(), "refusal cannot create a completion record");
    assert_eq!(fs::read(fixture.log()).unwrap(), b"raced-log\n");
    assert_refusal_halt(&out);
    let events = if json {
        out.stdout.clone()
    } else {
        fs::read_to_string(fixture.dir.join("runtime/events.jsonl")).unwrap()
    };
    let events: Vec<serde_json::Value> = events
        .lines()
        .map(|line| serde_json::from_str(line).expect("every stdout/event line is JSON"))
        .collect();
    let refusals: Vec<_> = events
        .iter()
        .filter(|e| {
            e["event"] == "message"
                && e["text"].as_str().unwrap_or_default().contains("refusing to spawn:")
        })
        .collect();
    assert_eq!(refusals.len(), 1, "no subsequent preflight may mask this error: {events:?}");
    assert!(
        refusals[0]["text"].as_str().unwrap().contains("error"),
        "the refusal must arrive through completion error handling: {events:?}"
    );
}

macro_rules! exclusive_open_contract {
    ($name:ident, $agent:expr, $parallel:expr) => {
        #[test]
        fn $name() {
            exclusive_open_refusal($agent, $parallel, false);
            exclusive_open_refusal($agent, $parallel, true);
        }
    };
}

exclusive_open_contract!(program_serial_exclusive_open_refusal, false, "1");
exclusive_open_contract!(program_pool_exclusive_open_refusal, false, "2");
exclusive_open_contract!(agent_serial_exclusive_open_refusal, true, "1");
exclusive_open_contract!(agent_pool_exclusive_open_refusal, true, "2");
