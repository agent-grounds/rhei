//! Snapshot generations under `rhei verify` and `rhei archive`: a cached
//! transcript is held to its recorded hash by its bytes, and every generation
//! a manifest names is packed, from the in-workspace cache or an absolute
//! `snapshots.cache_dir`, and verified from the archive. §FS-rhei-verify.2
//! §FS-rhei-archive.2.1

use std::fs;
use std::path::{Path, PathBuf};

use super::archive_support::{
    archive, assert_refused, elsewhere, flip_member_byte, index, only_run_id, rhei_in,
    verify_archive_json,
};
use super::evidence_support::every_file;
use super::snapshot_tests::{write_fake_snapshot_agent, write_fake_snapshot_settings};
use super::verify_findings_tests::{finding, SNAPSHOT_MACHINE};
use super::{run_cli, unique_temp_dir, write_fixture_file, TestDir};

/// A finished snapshot run, its generations in `cache` when one is given and
/// in `.rhei/cache/snapshots` otherwise.
fn snapshot_run(dir: &Path, cache: Option<&Path>) {
    let agent = write_fake_snapshot_agent(dir);
    write_fake_snapshot_settings(dir, &agent);
    if let Some(cache) = cache {
        let path = dir.join(".agent-grounds/rhei/settings.json");
        let mut settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).expect("settings");
        settings["snapshots"] = serde_json::json!({ "cache_dir": cache.display().to_string() });
        fs::write(&path, settings.to_string()).expect("write settings");
    }
    let plan = write_fixture_file(
        dir,
        "plan.rhei.md",
        "# Rhei: Snapshot\n\n## Tasks\n\n### Task 1: Carry context\n**State:** source\n",
    );
    let machine = write_fixture_file(dir, "states.yaml", SNAPSHOT_MACHINE);
    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(run.status.success(), "fixture run: {}\n{}", run.stdout, run.stderr);
    assert!(!transcripts(cache.unwrap_or(&dir.join(".rhei/cache/snapshots"))).is_empty());
}

/// Every `transcript.*` file under `cache`.
fn transcripts(cache: &Path) -> Vec<PathBuf> {
    every_file(cache)
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| {
            path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("transcript."))
        })
        .collect()
}

/// `rhei verify <dir> --json`, parsed.
fn verify_in_place(dir: &Path) -> (super::CliRun, serde_json::Value) {
    let root = dir.display().to_string();
    let result = rhei_in(dir, &dir.join(".home"), &["verify", &root, "--json"]);
    let json = serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("one JSON object: {err}\n{}\n{}", result.stdout, result.stderr)
    });
    (result, json)
}

/// Archive the one run of `dir` into a fresh directory: `(guard, dir, tar)`.
fn pack(dir: &Path, prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let (guard, kept) = elsewhere(prefix);
    let tar = kept.join("run.tar");
    let result = archive(dir, &only_run_id(dir), &tar, &[]);
    assert!(result.status.success(), "rhei archive: {}\n{}", result.stdout, result.stderr);
    (guard, kept, tar)
}

/// The packed transcript members the index lists.
fn packed_transcripts(tar: &Path) -> Vec<String> {
    let index = index(tar);
    let generations = index["snapshots"].as_array().expect("index.snapshots");
    assert!(!generations.is_empty(), "the index lists the generations: {index:#}");
    generations
        .iter()
        .flat_map(|generation| generation["members"].as_array().cloned().unwrap_or_default())
        .filter_map(|member| member.as_str().map(str::to_string))
        .filter(|member| {
            member.rsplit('/').next().is_some_and(|name| name.starts_with("transcript."))
        })
        .collect()
}

/// A cached transcript whose bytes were edited fails in place, though the
/// generation's own `manifest.json` still records the old hash.
/// §FS-rhei-verify.2 §FS-rhei-verify.3
#[test]
#[ignore = "red until #525: verify compares the generation's manifest field and never hashes the transcript; implement removes this"]
fn an_edited_cached_transcript_fails_in_place() {
    let dir = unique_temp_dir("verify-transcript-bytes");
    snapshot_run(&dir, None);
    let (before, json) = verify_in_place(&dir);
    assert!(before.status.success(), "fixture: verifies before the edit: {json:#}");
    for transcript in transcripts(&dir.join(".rhei/cache/snapshots")) {
        let mut bytes = fs::read(&transcript).expect("transcript");
        bytes.extend_from_slice(b"\n{\"edited\":true}\n");
        fs::write(&transcript, bytes).expect("edit the transcript");
    }
    let (result, json) = verify_in_place(&dir);
    assert_refused(&result, &json, "mismatch");
}

/// The in-workspace cache's generations are packed and verified from the
/// archive after the workspace is gone. §FS-rhei-archive.2.1
#[test]
#[ignore = "red until #525: rhei archive does not exist; implement removes this"]
fn generations_in_the_workspace_cache_are_packed() {
    let dir = unique_temp_dir("archive-workspace-cache");
    snapshot_run(&dir, None);
    let (_guard, kept, tar) = pack(&dir, "archive-workspace-cache-kept");
    assert!(!packed_transcripts(&tar).is_empty(), "a transcript is packed");
    fs::remove_dir_all(&*dir).expect("remove the workspace");
    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    assert!(result.status.success(), "{json:#}\n{}", result.stderr);
    assert!(finding(&json, "snapshot_not_cached").is_none(), "every generation packed: {json:#}");
}

/// A cache outside the workspace: packed while it exists; once removed, in
/// place reports `snapshot_not_cached` and the archive still verifies the
/// generation, until its packed transcript is edited. §FS-rhei-archive.2.1
/// §FS-rhei-verify.3
#[test]
#[ignore = "red until #525: rhei archive does not exist and verify reads no archive; implement removes this"]
fn generations_in_an_external_cache_are_verified_from_the_archive() {
    let dir = unique_temp_dir("archive-external-cache");
    let cache_dir = unique_temp_dir("archive-external-cache-store");
    let cache = cache_dir.join("snapshots");
    snapshot_run(&dir, Some(&cache));
    let (_guard, kept, tar) = pack(&dir, "archive-external-cache-kept");
    let packed = packed_transcripts(&tar);
    assert!(!packed.is_empty(), "the external generation is packed");

    fs::remove_dir_all(&cache).expect("remove the cache");
    let (result, json) = verify_in_place(&dir);
    assert!(result.status.success(), "not cached does not fail: {json:#}");
    assert!(finding(&json, "snapshot_not_cached").is_some(), "{json:#}");

    fs::remove_dir_all(&*dir).expect("remove the workspace");
    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    assert!(result.status.success(), "{json:#}\n{}", result.stderr);
    assert!(finding(&json, "snapshot_not_cached").is_none(), "verified from the archive: {json:#}");

    flip_member_byte(&tar, &packed[0]);
    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    assert_eq!(assert_refused(&result, &json, "mismatch")["path"], packed[0].as_str());
}
