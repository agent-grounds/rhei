//! Port of #702's controlled reproducer: keep the real truncation/write window
//! open until the scheduler has sampled it. No bytes or exits are injected.
//! Linux needs cc and LD_PRELOAD; the original advisory test runs everywhere.
//! §FS-rhei-transition-cmd.3.4 §REQ-cross-platform.4.1

use super::provider_limit_support::RunningChild;
use super::*;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

fn publication_preserves_live_release(scope: &str) {
    let dir = unique_temp_dir("issue-702-publication");
    let observer = dir.join("delay.so");
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-Wall", "-Wextra", "-Werror"])
        .arg(fixture_path("live_gate_budget_delay_write.c"))
        .args(["-ldl", "-o"])
        .arg(&observer)
        .output()
        .expect("controlled Linux observer requires cc");
    assert!(compiled.status.success(), "{}", stderr(&compiled));

    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "live_gate_budget_tests::issue_653_gate_release_without_descriptor_gives_foreground_recovery",
            "--exact",
            "--nocapture",
        ])
        .env("LD_PRELOAD", &observer)
        .env("RHEI702_DELAY", scope)
        .env("RHEI_KEEP_TEST_DIRS", "1")
        .env("TMPDIR", &*dir);
    let mut harness = RunningChild::spawn(&mut command, &dir, "harness");
    // The observer itself has a 60s safety bound; this margin includes both
    // original scopes and leaves time to report a failed observer separately.
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = harness.child().try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "controlled harness stalled; {}", harness.output());
        std::thread::sleep(Duration::from_millis(25));
    };
    let mut output = harness.output();
    for entry in fs::read_dir(&*dir).unwrap() {
        let root = entry.unwrap().path();
        if root.is_dir() {
            output.push_str(&stderr_from_file(root.join("unpublished.stderr")));
        }
    }
    assert!(!output.contains("could not observe run lock release"), "{output}");
    if !status.success() {
        // Classify the reproducer before the success assertion: a compiler,
        // observer, or startup failure must not masquerade as this regression.
        for evidence in [
            "opened existing machine with O_TRUNC",
            "run lock HELD at truncation",
            "run lock FREE before replacement write and before transition",
            "the document must be a mapping",
            "missing live gate-release advisory",
            "stderr: \"\"",
        ] {
            assert!(
                output.contains(evidence),
                "missing reproduction evidence {evidence:?}; {output}"
            );
        }
        if scope == "project" {
            assert!(output.contains("project run lock FREE"), "{output}");
        }
    }
    assert!(
        status.success(),
        "{scope} budget publication killed the live run before its recovery advisory; {output}"
    );
    assert!(output.contains("1 passed; 0 failed"), "original advisory test must execute; {output}");
}

#[test]
#[ignore = "red until #702: standalone budget publication must preserve the live advisory; implement removes this"]
fn issue_702_standalone_budget_publication_preserves_live_release() {
    publication_preserves_live_release("standalone");
}

#[test]
#[ignore = "red until #702: project budget publication must preserve the live advisory; implement removes this"]
fn issue_702_project_budget_publication_preserves_live_release() {
    publication_preserves_live_release("project");
}
