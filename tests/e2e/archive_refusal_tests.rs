//! What `rhei archive` refuses, and that a refusal writes no file: a run that
//! is unknown or not finalized, a run that fails in place, an output that
//! exists or cannot be written, and a manifest path that is not safe to read.
//! §FS-rhei-archive.3 §FS-rhei-archive.5

use std::fs;
use std::path::Path;

use super::archive_support::{archive, assert_refused, elsewhere, only_run_id};
use super::evidence_support::{
    evidence_workspace, only_manifest, run_dirs, sha256_hex, EvidenceWorkspace, Profile,
};
use super::CliRun;

/// `rhei archive … --json` for `run` into `output`, parsed.
fn archive_json(ws: &EvidenceWorkspace, run: &str, output: &Path) -> (CliRun, serde_json::Value) {
    let result = archive(&ws.root, run, output, &["--json"]);
    let json = serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("archive --json prints one object: {err}\n{}\n{}", result.stdout, result.stderr)
    });
    (result, json)
}

/// Exit 1 with `code`, `archive: null`, and no file at `output`.
fn assert_nothing_written(ws: &EvidenceWorkspace, run: &str, output: &Path, code: &str) {
    let (result, json) = archive_json(ws, run, output);
    assert_refused(&result, &json, code);
    assert!(json["archive"].is_null(), "{code}: archive is null: {json:#}");
    assert!(!output.exists(), "{code}: no file at {}", output.display());
}

fn finished(prefix: &str) -> (EvidenceWorkspace, String) {
    let ws = evidence_workspace(prefix, Profile::default());
    ws.run();
    let run = only_run_id(&ws.root);
    (ws, run)
}

/// An unknown run and one with no manifest are refused. §FS-rhei-archive.3.1
/// §FS-rhei-archive.5.1
#[test]
#[ignore = "red until #525: rhei archive does not exist; implement removes this"]
fn an_unknown_or_unfinalized_run_is_refused() {
    let (ws, run) = finished("archive-unknown-run");
    let (_keep, kept) = elsewhere("archive-unknown-run-kept");
    assert_nothing_written(&ws, "000000", &kept.join("unknown.tar"), "unknown_run");

    let source = run_dirs(&ws.evidence_dir())[0].clone();
    let record = fs::read_dir(&source)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with(".start.json"))
        .expect("a start record");
    let dead = ws.evidence_dir().join("runs/deadbe");
    fs::create_dir_all(&dead).expect("a run directory");
    fs::copy(&record, dead.join(record.file_name().unwrap())).expect("copy a start record");
    assert_nothing_written(&ws, "deadbe", &kept.join("dead.tar"), "not_finalized");
    assert_ne!(run, "deadbe");
}

/// A run that fails verify in place is not packed. §FS-rhei-archive.3.1
#[test]
#[ignore = "red until #525: rhei archive does not exist; implement removes this"]
fn a_run_that_fails_in_place_is_not_packed() {
    let (ws, run) = finished("archive-mismatch");
    let (_, manifest) = only_manifest(&ws.evidence_dir());
    let log = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .find(|path| path.starts_with("runtime/logs/"))
        .expect("a named log")
        .to_string();
    let on_disk = ws.root.join(&log);
    let mut bytes = fs::read(&on_disk).expect("log");
    bytes.extend_from_slice(b"edited\n");
    fs::write(&on_disk, bytes).expect("edit the log");
    let (_keep, kept) = elsewhere("archive-mismatch-kept");
    assert_nothing_written(&ws, &run, &kept.join("run.tar"), "mismatch");
}

/// An existing output is never replaced; an output that cannot be written is
/// `write_failed`; one under `runtime/evidence/` is a usage error.
/// §FS-rhei-archive.1 §FS-rhei-archive.3.2 §FS-rhei-archive.5.1
#[test]
#[ignore = "red until #525: rhei archive does not exist; implement removes this"]
fn the_output_is_never_replaced_and_a_failed_write_leaves_nothing() {
    let (ws, run) = finished("archive-output");
    let (_keep, kept) = elsewhere("archive-output-kept");
    let existing = kept.join("existing.tar");
    fs::write(&existing, b"kept as it was").expect("an existing output");
    let (result, json) = archive_json(&ws, &run, &existing);
    assert!(assert_refused(&result, &json, "output_exists")["path"].is_string(), "{json:#}");
    assert!(json["archive"].is_null(), "{json:#}");
    assert_eq!(fs::read(&existing).unwrap(), b"kept as it was", "the output is untouched");

    let blocked = kept.join("a-file");
    fs::write(&blocked, b"not a directory").expect("a file where a directory goes");
    assert_nothing_written(&ws, &run, &blocked.join("run.tar"), "write_failed");

    let inside = ws.evidence_dir().join("run.tar");
    let usage = archive(&ws.root, &run, &inside, &[]);
    assert_eq!(usage.status.code(), Some(2), "{}\n{}", usage.stdout, usage.stderr);
    assert!(!inside.exists(), "no file under runtime/evidence/");
}

/// A manifest path with `..` is never read, even when it hashes as recorded.
/// §FS-rhei-archive.3.3
#[test]
#[ignore = "red until #525: rhei archive does not exist; implement removes this"]
fn a_manifest_path_with_a_parent_component_is_refused() {
    let (ws, run) = finished("archive-dotdot");
    let outside = ws.root.parent().expect("a parent").join(format!("{run}-outside.txt"));
    fs::write(&outside, b"outside the workspace\n").expect("a file outside");
    let (path, mut manifest) = only_manifest(&ws.evidence_dir());
    let name = outside.file_name().unwrap().to_string_lossy().into_owned();
    manifest["files"].as_array_mut().unwrap().push(serde_json::json!({
        "path": format!("../{name}"), "sha256": sha256_hex(b"outside the workspace\n"),
    }));
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).expect("rewrite the manifest");
    let (_keep, kept) = elsewhere("archive-dotdot-kept");
    let output = kept.join("run.tar");
    assert_nothing_written(&ws, &run, &output, "archive_unsafe_member");
    let _ = fs::remove_file(outside);
}

/// A log replaced by a symlink to identical bytes outside the workspace is
/// refused: the packer does not follow it. §FS-rhei-archive.3.3
#[cfg(unix)]
#[test]
#[ignore = "red until #525: rhei archive does not exist; implement removes this"]
fn a_log_replaced_by_a_symlink_outside_is_refused() {
    let (ws, run) = finished("archive-symlink");
    let (_, manifest) = only_manifest(&ws.evidence_dir());
    let log = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .find(|path| path.starts_with("runtime/logs/"))
        .expect("a named log")
        .to_string();
    let (_keep, kept) = elsewhere("archive-symlink-kept");
    let target = kept.join("outside.log");
    fs::copy(ws.root.join(&log), &target).expect("copy the log outside");
    fs::remove_file(ws.root.join(&log)).expect("remove the log");
    std::os::unix::fs::symlink(&target, ws.root.join(&log)).expect("symlink the log");
    assert_nothing_written(&ws, &run, &kept.join("run.tar"), "archive_unsafe_member");
}
