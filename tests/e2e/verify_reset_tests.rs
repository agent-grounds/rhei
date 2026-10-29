//! `rhei verify` across a narrowed reset: a path the reset event names is
//! pruned for the runs before it, whatever is there now, and a log removed by
//! hand is still missing. §FS-rhei-verify §FS-rhei-verify.3
//! §FS-rhei-evidence.10.1

use std::fs;

use super::evidence_support::{
    assert_store_exists, evidence_workspace, run_dirs, EvidenceWorkspace, Profile,
};
use super::verify_findings_tests::finding;
use super::CliRun;

/// The probe's program-then-agent ticket as the one ticket of member rhei
/// `auth` in a Panta project, whose root is the run's root.
fn project(prefix: &str) -> EvidenceWorkspace {
    let ws = evidence_workspace(prefix, Profile::default());
    fs::remove_file(&ws.plan).expect("drop the single-file plan");
    fs::write(ws.root.join("index.panta.md"), "# Panta: Probe\n").expect("project");
    fs::create_dir_all(ws.root.join("auth/tasks")).expect("member tasks");
    fs::write(ws.root.join("auth/index.rhei.md"), "# Rhei: Auth\n").expect("member");
    fs::write(ws.root.join("auth/tasks/probe.md"), "### Task 1: Probe\n**State:** measure\n")
        .expect("ticket");
    ws
}

fn run_project(ws: &EvidenceWorkspace) {
    let machine = ws.machine.display().to_string();
    let run = ws.rhei(&["--state-machine", &machine, "run", ".", "--no-tui", "--no-callbacks"]);
    assert!(run.status.success(), "fixture run: {}\n{}", run.stdout, run.stderr);
}

fn reset_auth(ws: &EvidenceWorkspace) -> CliRun {
    let machine = ws.machine.display().to_string();
    let reset = ws.rhei(&["--state-machine", &machine, "reset", ".", "--rhei", "auth", "--yes"]);
    assert!(reset.status.success(), "fixture reset: {}\n{}", reset.stdout, reset.stderr);
    reset
}

/// Every `runtime/logs/` path the one run's manifest names.
fn manifest_logs(ws: &EvidenceWorkspace) -> Vec<String> {
    let dirs = run_dirs(&ws.evidence_dir());
    assert_eq!(dirs.len(), 1, "one run so far under {}", ws.evidence_dir().display());
    let text = fs::read_to_string(dirs[0].join("manifest.json")).expect("the run's manifest");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("manifest parses");
    let logs: Vec<String> = manifest["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .filter(|path| path.starts_with("runtime/logs/"))
        .map(str::to_string)
        .collect();
    assert!(!logs.is_empty(), "the manifest names the run's logs: {manifest:#}");
    logs
}

fn first_run_id(ws: &EvidenceWorkspace) -> String {
    let dirs = run_dirs(&ws.evidence_dir());
    dirs[0].file_name().expect("run id").to_string_lossy().into_owned()
}

/// The author's pinned case: a narrowed reset, a rerun of the same ticket
/// that re-creates its plain log names with new bytes, then verify over both
/// runs exits 0, the old paths pruned for the first run only.
/// §FS-rhei-verify.3 §FS-rhei-evidence.10.1
#[test]
#[ignore = "red until #520: a run writes no manifest and a narrowed reset writes no event; implement removes this"]
fn a_narrowed_reset_and_a_rerun_of_the_ticket_verify_both_runs() {
    let ws = project("verify-reset-rerun");
    run_project(&ws);
    assert_store_exists(&ws.evidence_dir());
    let logs = manifest_logs(&ws);
    let first = first_run_id(&ws);
    reset_auth(&ws);
    run_project(&ws);
    for log in &logs {
        assert!(ws.root.join(log).is_file(), "fixture: the rerun re-created {log}");
    }

    let (result, json) = ws.verify_json(&[]);
    assert!(result.status.success(), "both runs verify: {json:#}");
    assert_eq!(json["runs"].as_array().map(Vec::len), Some(2), "{json:#}");
    assert!(finding(&json, "mismatch").is_none(), "no mismatch: {json:#}");
    let findings = json["findings"].as_array().expect("findings");
    for log in &logs {
        let pruned: Vec<&serde_json::Value> = findings
            .iter()
            .filter(|found| found["reason_code"] == "pruned" && found["path"] == log.as_str())
            .collect();
        assert_eq!(pruned.len(), 1, "{log} pruned once: {json:#}");
        assert_eq!(pruned[0]["run_id"], first.as_str(), "pruned for the first run: {json:#}");
    }
}

/// A log a narrowed reset removed and nothing re-created is pruned, not
/// missing. §FS-rhei-verify.3
#[test]
#[ignore = "red until #520: a run writes no manifest and a narrowed reset writes no event; implement removes this"]
fn a_log_removed_by_a_narrowed_reset_is_pruned() {
    let ws = project("verify-reset-pruned");
    run_project(&ws);
    assert_store_exists(&ws.evidence_dir());
    let logs = manifest_logs(&ws);
    reset_auth(&ws);
    for log in &logs {
        assert!(!ws.root.join(log).exists(), "fixture: the reset removed {log}");
    }

    let (result, json) = ws.verify_json(&[]);
    assert!(result.status.success(), "a pruned log does not fail: {json:#}");
    assert!(finding(&json, "missing").is_none(), "{json:#}");
    for log in &logs {
        assert!(
            json["findings"]
                .as_array()
                .expect("findings")
                .iter()
                .any(|found| found["reason_code"] == "pruned" && found["path"] == log.as_str()),
            "{log} is pruned: {json:#}"
        );
    }
}

/// A log deleted by hand, with no reset to explain it, is missing.
/// §FS-rhei-verify.3
#[test]
#[ignore = "red until #520: a run writes no manifest; implement removes this"]
fn a_log_deleted_by_hand_is_missing() {
    let ws = project("verify-hand-deleted");
    run_project(&ws);
    assert_store_exists(&ws.evidence_dir());
    let log = manifest_logs(&ws).remove(0);
    fs::remove_file(ws.root.join(&log)).expect("delete the log by hand");

    let (result, json) = ws.verify_json(&[]);
    assert_eq!(result.status.code(), Some(1), "{json:#}");
    let found = finding(&json, "missing").unwrap_or_else(|| panic!("a missing finding: {json:#}"));
    assert_eq!(found["path"], log.as_str());
}
