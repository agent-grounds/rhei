//! A deterministic version of #454's transient-publication failure, using
//! the portable test's actual fixture. §FS-rhei-run.3.7.3 §FS-rhei-run.3.7.4
//!
//! The external worker must publish a complete current image. Atomic replacement
//! alone does not exclude stale reads of sibling finalization: implement must
//! also synchronize the authoritative read/edit with the engine writer, using
//! its stable sidecar or a bounded terminal-finalization barrier. The ordinary
//! test checks the finalized result link in every existing mode.
//! §AR-agent-orchestrator-workflow.3.3.1 §AR-agent-orchestrator-workflow.3.3.1.1
//!
//! This opt-in observer requires glibc Linux, /proc and cc; the portable test
//! runs by default everywhere. Run with --include-ignored to enforce this gate.

use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::worker_edit_one_file_support::OneFileScenario;
use super::worker_edit_revert_support::{end_records, meta_value, Mode};
use super::*;

// §FS-rhei-run.3.7.3 §FS-rhei-run.3.7.4
#[test]
#[ignore = "controlled Linux read observer: requires cc and LD_PRELOAD; run with --include-ignored"]
fn a_shared_file_publication_never_exposes_a_missing_sibling() {
    let scenario = OneFileScenario::new(
        "worker-edit-publication",
        Mode::Parallel,
        include_str!("fixtures/worker_edit_truncate_gate.py"),
    );
    let ran = observe(&scenario, "worker_edit_read_barrier.c");
    let output = scenario.diagnostics(&ran);
    for error in ["barrier-timeout", "unexpected-read"] {
        assert!(!scenario.dir.join(error).exists(), "observer infrastructure: {error}\n{output}");
    }
    // If the old publisher was used, require the exact real-read failure and
    // preserved on-disk evidence before the success assertion turns red. Do
    // not let infrastructure errors masquerade as the issue's reproduction.
    if scenario.dir.join("empty-captured").exists() {
        assert!(ran.stderr.contains("scheduling reader captured 0 bytes"), "{output}");
        assert!(ran.stderr.contains("Task ws.3 depends on missing Task ws.2"), "{output}");
        assert!(scenario.dir.join("restore-announced").exists(), "{output}");
        scenario.assert_sibling_preserved(&output);
        let ends = end_records(&scenario.journal(), "ws.1", "cover");
        assert_eq!(ends.len(), 1, "{output}");
        assert_eq!(meta_value(&ends[0], "outcome").as_deref(), Some("failed"), "{output}");
        assert!(meta_value(&ends[0], "reverted").is_some(), "{output}");
        let spawn: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(scenario.ws.join("runtime/spawns/task-ws.1-cover.json"))
                .expect("bad attempt spawn"),
        )
        .expect("spawn JSON");
        assert_eq!(spawn["charged"], 1, "{output}");
        assert_eq!(spawn["attempt_charged"], true, "{output}");
        let transitions = fs::read_to_string(scenario.ws.join("runtime/state-transitions.log"))
            .unwrap_or_default();
        assert!(!transitions.lines().any(|line| line.contains("ws.1")), "{output}");
    }
    scenario.assert_completed(&ran);
}

/// A genuine sibling writer reads its completed state while its sidecar is
/// held. The fixture's old authoritative read captures that intermediate
/// image, then waits until the writer publishes its result link before writing
/// the malformed edit. Restoration must not inherit a lost result link.
// §FS-rhei-run.3.7.3 §FS-rhei-run.3.7.4
#[test]
#[ignore = "controlled Linux finalization observer: requires cc and LD_PRELOAD; run with --include-ignored"]
fn a_shared_file_edit_keeps_the_siblings_finalized_result_link() {
    let scenario = OneFileScenario::new(
        "worker-edit-finalization",
        Mode::Parallel,
        include_str!("fixtures/worker_edit_finalization_gate.py"),
    );
    let ran = observe(&scenario, "worker_edit_finalization_barrier.c");
    let output = scenario.diagnostics(&ran);
    assert!(
        scenario.dir.join("state-published").exists(),
        "observer did not pause finalization\n{output}"
    );
    assert!(scenario.dir.join("finalized").exists(), "sibling never finalized\n{output}");
    if scenario.dir.join("snapshot-held").exists() {
        let stale =
            fs::read_to_string(scenario.dir.join("stale-shared.md")).expect("stale snapshot");
        let finalized = fs::read_to_string(scenario.dir.join("finalized-shared.md"))
            .expect("actual finalization image");
        assert!(!stale.contains("> **Result:** [ws.2]"), "{output}");
        assert!(finalized.contains("> **Result:** [ws.2](runtime/results/ws.2.md)"), "{output}");
    }
    scenario.assert_completed(&ran);
}

fn observe(scenario: &OneFileScenario, barrier: &str) -> CliRun {
    let observer = scenario.dir.join("observer.so");
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-Wall", "-Wextra", "-Werror", "-o"])
        .arg(&observer)
        .arg(fixture_path(barrier))
        .arg("-ldl")
        .output()
        .expect("controlled observer requires cc on PATH");
    assert!(compiled.status.success(), "{}", stderr(&compiled));

    let out_path = scenario.dir.join("stdout.log");
    let err_path = scenario.dir.join("stderr.log");
    let mut command = rhei_command(scenario.dir.join(".home"));
    command
        .arg("--state-machine")
        .arg(&scenario.machine)
        .arg("run")
        .arg(&scenario.ws)
        .args(["--no-tui", "--no-callbacks", "--parallel", "2"])
        .env("LD_PRELOAD", &observer)
        .env("RHEI_REPRO_GATE", &*scenario.dir)
        .stdout(Stdio::from(fs::File::create(&out_path).expect("stdout capture")))
        .stderr(Stdio::from(fs::File::create(&err_path).expect("stderr capture")));
    let mut child = command.spawn().expect("run instrumented Rhei");
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll Rhei") {
            break status;
        }
        if stderr_from_file(&err_path).contains("Reverted Task ws.1") {
            fs::write(scenario.dir.join("restore-announced"), "").expect("release captured read");
        }
        if scenario
            .task_file("01-shared.md")
            .contains("> **Result:** [ws.2](runtime/results/ws.2.md)")
            && !scenario.dir.join("finalized").exists()
        {
            fs::write(scenario.dir.join("finalized-shared.md"), scenario.task_file("01-shared.md"))
                .expect("capture published result link");
            fs::write(scenario.dir.join("finalized"), "").expect("release stale snapshot");
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill timed-out fixture");
            child.wait().expect("reap fixture");
            panic!("controlled publication exceeded 45 seconds\n{}", stderr_from_file(&err_path));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    CliRun { status, stdout: stdout_from_file(&out_path), stderr: stderr_from_file(&err_path) }
}
