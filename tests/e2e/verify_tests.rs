//! `rhei verify` over a clean workspace: what it prints, `--digest`, a
//! workspace with nothing to verify, where the manifest is announced, and
//! that it writes nothing. §FS-rhei-verify

use std::fs;

use super::evidence_support::{
    assert_store_exists, every_file, evidence_workspace, only_manifest, sha256_hex, Profile,
};
use super::verify_findings_tests::finding;

/// A clean run verifies, and the text names the run and its full digest.
/// §FS-rhei-verify.4 §FS-rhei-verify.5
#[test]
fn a_clean_run_verifies_and_prints_its_full_digest() {
    let ws = evidence_workspace("verify-clean", Profile::default());
    ws.run();
    let verify = ws.verify(&[]);
    assert!(verify.status.success(), "verify exits 0:\n{}\n{}", verify.stdout, verify.stderr);

    let (path, manifest) = only_manifest(&ws.evidence_dir());
    let digest = sha256_hex(&fs::read(&path).expect("manifest bytes"));
    let run_id = manifest["run_id"].as_str().expect("run_id");
    let header = verify
        .stdout
        .lines()
        .find(|line| line.starts_with(&format!("run {run_id} ")))
        .unwrap_or_else(|| panic!("a block for run {run_id}:\n{}", verify.stdout));
    assert!(header.contains(&format!("manifest sha256:{digest}")), "the full digest: {header}");
    assert!(verify.stdout.lines().any(|line| line == "verified: 1 run"), "{}", verify.stdout);
}

/// `--digest` takes the full digest: the manifest's passes, an unknown one is
/// `unknown_digest`, and a shortened one is a usage error. §FS-rhei-verify.1
/// §FS-rhei-verify.3
#[test]
fn digest_selects_one_manifest_by_its_full_sha256() {
    let ws = evidence_workspace("verify-digest", Profile::default());
    ws.run();
    let (path, _) = only_manifest(&ws.evidence_dir());
    let digest = sha256_hex(&fs::read(&path).expect("manifest bytes"));

    let right = ws.verify(&["--digest", &digest]);
    assert!(right.status.success(), "the right digest passes:\n{}\n{}", right.stdout, right.stderr);

    let unknown = "f".repeat(64);
    let (result, json) = ws.verify_json(&["--digest", &unknown]);
    assert_eq!(result.status.code(), Some(1), "{}", result.stdout);
    let found = finding(&json, "unknown_digest").expect("an unknown_digest finding");
    assert!(found["run_id"].is_null(), "{found:#}");

    let short = ws.verify(&["--digest", &digest[..12]]);
    assert_eq!(short.status.code(), Some(2), "a shortened digest is refused: {}", short.stderr);
}

/// A workspace with no manifest has nothing to verify, which fails.
/// §FS-rhei-verify.3 §FS-rhei-verify.5
#[test]
fn a_workspace_with_no_manifest_has_nothing_to_verify() {
    let ws = evidence_workspace("verify-nothing", Profile::default());
    fs::create_dir_all(ws.root.join("runtime/logs")).expect("a runtime from before evidence");
    let (result, json) = ws.verify_json(&[]);
    assert_eq!(result.status.code(), Some(1), "{}", result.stdout);
    let found = finding(&json, "nothing_to_verify").expect("a nothing_to_verify finding");
    assert!(found["run_id"].is_null(), "{found:#}");
    assert_eq!(json["verified"], false);
}

/// `run.json` and the `run_finished` record name the manifest and its digest.
/// §FS-rhei-evidence.9.2 §FS-rhei-run-json.2.1 §FS-rhei-run-headless.2
#[test]
fn run_json_and_run_finished_announce_the_manifest() {
    let ws = evidence_workspace("verify-announce", Profile::default());
    ws.run();
    let descriptor: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(ws.root.join("runtime/run.json")).expect("run.json"),
    )
    .expect("run.json parses");
    let announced = &descriptor["evidence_manifest"];
    assert!(announced.is_object(), "run.json carries evidence_manifest: {descriptor:#}");

    let (path, manifest) = only_manifest(&ws.evidence_dir());
    let digest = sha256_hex(&fs::read(&path).expect("manifest bytes"));
    let run_id = manifest["run_id"].as_str().expect("run_id");
    let expected = serde_json::json!({
        "path": format!("runtime/evidence/runs/{run_id}/manifest.json"),
        "sha256": digest,
    });
    assert_eq!(*announced, expected);

    let events = fs::read_to_string(ws.root.join("runtime/events.jsonl")).expect("events.jsonl");
    let finished: serde_json::Value = events
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|record| record["event"] == "run_finished")
        .expect("a run_finished record");
    assert_eq!(finished["evidence_manifest"], expected, "{finished:#}");
}

/// Verify only reads: every byte under `runtime/` is the same afterwards.
/// §FS-rhei-verify
#[test]
fn verify_writes_nothing() {
    let ws = evidence_workspace("verify-read-only", Profile::default());
    ws.run();
    let runtime = ws.root.join("runtime");
    let before = every_file(&runtime);
    let verify = ws.verify(&[]);
    assert!(verify.status.success(), "verify exits 0:\n{}\n{}", verify.stdout, verify.stderr);
    assert_store_exists(&ws.evidence_dir());
    let after = every_file(&runtime);
    let names = |files: &[(std::path::PathBuf, Vec<u8>)]| {
        files.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(&before), names(&after), "verify created or removed a file");
    assert!(before == after, "verify changed a file under runtime/");
}
