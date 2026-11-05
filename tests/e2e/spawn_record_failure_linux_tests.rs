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
        #[ignore = "red until #548: report ENOSPC without changing successful completion; implement removes this"]
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
#[ignore = "red until #548: report record staging failures; implement removes this"]
fn completion_record_stage_failure_is_observable() {
    one_shot(false, "1", "stage");
}

#[test]
#[ignore = "red until #548: report record replacement failures; implement removes this"]
fn completion_record_replace_failure_is_observable() {
    one_shot(false, "1", "replace");
}

fn polling(parallel: &'static str, warning: bool) {
    let fixture = Fixture::new(false, parallel, "shim", 75, true);
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
        #[ignore = "red until #548: report poll completion-record write failures; implement removes this"]
        fn $warning() {
            polling($parallel, true);
        }

        #[test]
        #[ignore = "red until #548: explain same-run poll spawn refusal at halt; implement removes this"]
        fn $halt() {
            polling($parallel, false);
        }
    };
}

poll_contract!(poll_serial_write_warning, poll_serial_refusal_halt, "1");
poll_contract!(poll_parallel_write_warning, poll_parallel_refusal_halt, "2");
