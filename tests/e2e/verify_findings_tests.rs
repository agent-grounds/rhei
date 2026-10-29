//! What `rhei verify` reports when a workspace no longer holds what its runs
//! wrote: each finding's `reason_code`, path and hashes, in text and `--json`.
//! §FS-rhei-verify

use std::fs;
use std::path::{Path, PathBuf};

use super::evidence_support::{
    assert_store_exists, evidence_workspace, one_start, only_manifest, run_dirs, sha256_hex,
    EvidenceWorkspace, Profile,
};
use super::run_cli;
use super::snapshot_tests::{write_fake_snapshot_agent, write_fake_snapshot_settings};
use super::{unique_temp_dir, write_fixture_file};

/// The first finding with `code` in a `verify --json` object.
pub(super) fn finding<'a>(
    json: &'a serde_json::Value,
    code: &str,
) -> Option<&'a serde_json::Value> {
    json["findings"].as_array()?.iter().find(|found| found["reason_code"] == code)
}

/// A finished probe run and its manifest's `files` entry whose path ends in
/// `suffix`, as `(path, recorded sha256)`.
fn named_file(ws: &EvidenceWorkspace, suffix: &str) -> (String, String) {
    let (_, manifest) = only_manifest(&ws.evidence_dir());
    let entry = manifest["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|entry| entry["path"].as_str().is_some_and(|path| path.ends_with(suffix)))
        .unwrap_or_else(|| panic!("the manifest names a file ending {suffix}: {manifest:#}"));
    (
        entry["path"].as_str().expect("path").to_string(),
        entry["sha256"].as_str().expect("sha256").to_string(),
    )
}

/// Join a manifest path, `/`-separated, onto the run's root.
fn on_disk(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |joined, part| joined.join(part))
}

/// Exit 1 with one finding of `code` at `path`; returns the finding.
fn assert_fails_with(ws: &EvidenceWorkspace, code: &str, path: &str) -> serde_json::Value {
    let (result, json) = ws.verify_json(&[]);
    assert_eq!(result.status.code(), Some(1), "{code} fails verify: {json:#}");
    assert_eq!(json["verified"], false);
    let found = finding(&json, code).unwrap_or_else(|| panic!("a {code} finding: {json:#}"));
    assert_eq!(found["path"], path, "{found:#}");
    found.clone()
}

/// An edited log is a mismatch, named with its path and both full hashes.
/// §FS-rhei-verify.3 §FS-rhei-verify.4 §FS-rhei-verify.4.1
#[test]
fn an_edited_log_is_a_mismatch_with_both_hashes() {
    let ws = evidence_workspace("verify-edited-log", Profile::default());
    ws.run();
    let (path, recorded) = named_file(&ws, "-work.log");
    let log = on_disk(&ws.root, &path);
    let mut bytes = fs::read(&log).expect("log");
    bytes.extend_from_slice(b"edited\n");
    fs::write(&log, &bytes).expect("edit the log");
    let found_hash = sha256_hex(&bytes);

    let text = ws.verify(&[]);
    assert_eq!(text.status.code(), Some(1), "{}\n{}", text.stdout, text.stderr);
    assert!(text.stdout.contains(&format!("MISMATCH {path}")), "{}", text.stdout);
    assert!(text.stdout.contains(&format!("recorded sha256:{recorded}")), "{}", text.stdout);
    assert!(text.stdout.contains(&format!("found    sha256:{found_hash}")), "{}", text.stdout);

    let found = assert_fails_with(&ws, "mismatch", &path);
    assert_eq!(found["recorded_sha256"], recorded);
    assert_eq!(found["found_sha256"], found_hash);
}

/// An edited start record is a mismatch on that record. §FS-rhei-verify.3
#[test]
fn an_edited_start_record_is_a_mismatch() {
    let ws = evidence_workspace("verify-edited-record", Profile::default());
    ws.run();
    let (path, recorded) = named_file(&ws, ".start.json");
    let record = on_disk(&ws.root, &path);
    let mut text = fs::read_to_string(&record).expect("record");
    text.insert(text.len() - 1, ' ');
    fs::write(&record, &text).expect("edit the record");

    let found = assert_fails_with(&ws, "mismatch", &path);
    assert_eq!(found["recorded_sha256"], recorded);
    assert_eq!(found["found_sha256"], sha256_hex(text.as_bytes()));
}

/// A deleted prompt object is missing. §FS-rhei-verify.3
#[test]
fn a_deleted_prompt_object_is_missing() {
    let ws = evidence_workspace("verify-missing-object", Profile::default());
    ws.run();
    let evidence = ws.evidence_dir();
    assert_store_exists(&evidence);
    let hash = one_start(&evidence, "agent")["prompt"]["sha256"]
        .as_str()
        .expect("prompt.sha256")
        .to_string();
    let path = format!("runtime/evidence/objects/sha256/{hash}");
    fs::remove_file(on_disk(&ws.root, &path)).expect("delete the prompt object");

    let found = assert_fails_with(&ws, "missing", &path);
    assert_eq!(found["recorded_sha256"], hash);
    assert!(found["found_sha256"].is_null(), "{found:#}");
}

/// A record its manifest does not name, in a finalized run, is unlisted.
/// §FS-rhei-verify.2 §FS-rhei-verify.3
#[test]
fn an_extra_record_in_a_finalized_run_is_unlisted() {
    let ws = evidence_workspace("verify-unlisted", Profile::default());
    ws.run();
    let (path, _) = named_file(&ws, ".start.json");
    let extra = path.rsplit_once('/').expect("a run directory").0.to_string()
        + "/00000000000000000000000000000000.start.json";
    fs::copy(on_disk(&ws.root, &path), on_disk(&ws.root, &extra)).expect("plant a record");

    assert_fails_with(&ws, "unlisted", &extra);
}

/// A run directory with no manifest is reported, not failed.
/// §FS-rhei-verify.3 §FS-rhei-verify.4.1
#[test]
fn a_run_with_no_manifest_is_not_finalized_and_does_not_fail() {
    let ws = evidence_workspace("verify-not-finalized", Profile::default());
    ws.run();
    let (path, _) = named_file(&ws, ".start.json");
    let dead = ws.evidence_dir().join("runs/deadbe");
    fs::create_dir_all(&dead).expect("a run directory");
    let name = path.rsplit('/').next().expect("record name");
    fs::copy(on_disk(&ws.root, &path), dead.join(name)).expect("copy a start record");
    assert_eq!(run_dirs(&ws.evidence_dir()).len(), 2);

    let (result, json) = ws.verify_json(&[]);
    assert!(result.status.success(), "not_finalized does not fail: {json:#}");
    assert!(finding(&json, "not_finalized").is_some(), "{json:#}");
    let runs = json["runs"].as_array().expect("runs");
    let unfinished = runs.iter().find(|run| run["run_id"] == "deadbe").expect("the dead run");
    assert!(unfinished["digest"].is_null(), "{unfinished:#}");
    assert_eq!(json["verified"], true);
}

pub(super) const SNAPSHOT_MACHINE: &str = r#"name: verify-snapshot
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: fake:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: impl
        required: true
        select:
          state: source
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: review
  - from: review
    to: completed
"#;

/// A snapshot generation no longer in the cache is not checked, and does not
/// fail. §FS-rhei-verify.3 §FS-rhei-evidence.8
#[test]
fn a_snapshot_generation_not_in_the_cache_is_reported_and_passes() {
    let dir = unique_temp_dir("verify-snapshot-gone");
    let agent = write_fake_snapshot_agent(&dir);
    write_fake_snapshot_settings(&dir, &agent);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Snapshot\n\n## Tasks\n\n### Task 1: Carry context\n**State:** source\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", SNAPSHOT_MACHINE);
    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(run.status.success(), "fixture run: {}\n{}", run.stdout, run.stderr);

    let evidence = dir.join("runtime/evidence");
    let (_, manifest) = only_manifest(&evidence);
    assert!(
        manifest["snapshots"].as_array().is_some_and(|all| !all.is_empty()),
        "the manifest names the generations used: {manifest:#}"
    );
    fs::remove_dir_all(dir.join(".rhei/cache/snapshots")).expect("prune the snapshot cache");

    let output = super::rhei_command(dir.join(".home"))
        .args(["verify", &dir.display().to_string(), "--json"])
        .output()
        .expect("rhei verify runs");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("one JSON object");
    assert!(output.status.success(), "snapshot_not_cached does not fail: {json:#}");
    assert!(finding(&json, "snapshot_not_cached").is_some(), "{json:#}");
}
