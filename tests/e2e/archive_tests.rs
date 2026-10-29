//! `rhei archive` and `rhei verify <archive>` over intact runs: the round trip
//! away from the workspace with the same output as in place, the bytes, the
//! format, the index, `--json`, and what each packs. §FS-rhei-archive
//! §FS-rhei-verify

use std::fs;

use super::archive_support::{
    archive, archive_ok, elsewhere, index, members, only_run_id, rhei_in, verify_archive,
    verify_archive_json,
};
use super::evidence_seal_tests::strict_run;
use super::evidence_support::{
    assert_valid, evidence_workspace, only_manifest, sha256_hex, Profile,
};

/// Archive a run, remove its workspace, and verify the archive from elsewhere:
/// text, `--json` and `--digest` agree with the in-place verify before it.
/// §FS-rhei-verify §FS-rhei-verify.1 §FS-rhei-verify.2.1 §FS-rhei-archive.1
#[test]
fn an_archive_verifies_away_from_its_workspace_as_it_did_in_place() {
    let ws = evidence_workspace("archive-round-trip", Profile::default());
    ws.run();
    let in_text = ws.verify(&[]);
    let (in_place, in_json) = ws.verify_json(&[]);
    assert!(in_place.status.success(), "fixture: in place verifies: {in_json:#}");
    let digest = in_json["runs"][0]["digest"].as_str().expect("digest").to_string();

    let (_keep, kept) = elsewhere("archive-round-trip-kept");
    let tar = kept.join("run.tar");
    archive_ok(&ws, &tar);
    fs::remove_dir_all(&ws.root).expect("remove the workspace");

    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    assert!(result.status.success(), "the archive verifies: {json:#}\n{}", result.stderr);
    assert_eq!(json["runs"], in_json["runs"]);
    assert_eq!(json["findings"], in_json["findings"]);
    assert_eq!(json["verified"], true);
    let text = verify_archive(&kept, &tar, &[]);
    assert_eq!(text.stdout, in_text.stdout, "the text is the in-place text");

    let pinned = verify_archive(&kept, &tar, &["--digest", &digest]);
    assert!(pinned.status.success(), "--digest holds: {}\n{}", pinned.stdout, pinned.stderr);
    let (other, json) = verify_archive_json(&kept, &tar, &["--digest", &"f".repeat(64)]);
    assert_eq!(other.status.code(), Some(1), "{json:#}");
    assert!(super::verify_findings_tests::finding(&json, "unknown_digest").is_some(), "{json:#}");
}

/// Packing the same run twice gives the same bytes: the index first, then the
/// members sorted. §FS-rhei-archive.4.1
#[test]
fn packing_a_run_twice_gives_the_same_bytes() {
    let ws = evidence_workspace("archive-deterministic", Profile::default());
    ws.run();
    let (_keep, kept) = elsewhere("archive-deterministic-kept");
    let (first, second) = (kept.join("a.tar"), kept.join("b.tar"));
    archive_ok(&ws, &first);
    archive_ok(&ws, &second);
    assert!(fs::read(&first).unwrap() == fs::read(&second).unwrap(), "byte-identical archives");

    let names: Vec<String> = members(&first).into_iter().map(|(path, _)| path).collect();
    assert_eq!(names[0], "rhei-archive.json");
    let mut sorted = names[1..].to_vec();
    sorted.sort();
    assert_eq!(names[1..], sorted[..], "members after the index are sorted");
}

/// The index names the run and its manifest, validates against the published
/// `rhei.archive.v1`, and every member the manifest names is packed at its
/// path. §FS-rhei-archive.2 §FS-rhei-archive.4.2
#[test]
fn the_index_and_the_members_are_what_the_manifest_names() {
    let ws = evidence_workspace("archive-index", Profile::default());
    ws.run();
    let (manifest_path, manifest) = only_manifest(&ws.evidence_dir());
    let (_keep, kept) = elsewhere("archive-index-kept");
    let tar = kept.join("run.tar");
    archive_ok(&ws, &tar);

    let index = index(&tar);
    assert_valid("rhei.archive.v1", &index);
    assert_eq!(index["schema"], "rhei.archive.v1");
    assert_eq!(index["run_id"], manifest["run_id"]);
    assert_eq!(index["manifest"]["sha256"], sha256_hex(&fs::read(&manifest_path).unwrap()));
    let packed = members(&tar);
    for entry in manifest["files"].as_array().expect("files") {
        let path = entry["path"].as_str().expect("path");
        let (_, bytes) = packed
            .iter()
            .find(|(member, _)| member == path)
            .unwrap_or_else(|| panic!("{path} is packed"));
        assert_eq!(sha256_hex(bytes), entry["sha256"].as_str().unwrap(), "{path}");
    }
    let schemas = rhei_in(&ws.root, &ws.root.join(".home"), &["schema", "--list"]);
    assert!(schemas.stdout.lines().any(|id| id == "rhei.archive.v1"), "{}", schemas.stdout);
}

/// A member whose path ustar cannot hold round-trips through a GNU long name.
/// §FS-rhei-archive.4
#[test]
fn a_member_path_ustar_cannot_hold_round_trips() {
    let mut ws = evidence_workspace("archive-long-path", Profile::default());
    let long =
        "a-rhei-whose-id-is-long-enough-that-its-log-names-need-a-gnu-long-name-in-a-tar-file";
    let plan = ws.root.join(format!("{long}.rhei.md"));
    fs::rename(&ws.plan, &plan).expect("rename the plan");
    ws.plan = plan;
    ws.run();
    let (_, manifest) = only_manifest(&ws.evidence_dir());
    let paths: Vec<&str> =
        manifest["files"].as_array().unwrap().iter().filter_map(|e| e["path"].as_str()).collect();
    let long_member = paths
        .iter()
        .find(|path| path.rsplit('/').next().is_some_and(|name| name.len() > 100))
        .unwrap_or_else(|| panic!("fixture: a file name over ustar's 100 bytes: {paths:#?}"))
        .to_string();

    let (_keep, kept) = elsewhere("archive-long-path-kept");
    let tar = kept.join("run.tar");
    archive_ok(&ws, &tar);
    assert!(members(&tar).iter().any(|(path, _)| *path == long_member), "{long_member} packed");
    fs::remove_dir_all(&ws.root).expect("remove the workspace");
    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    assert!(result.status.success(), "the long path verifies: {json:#}\n{}", result.stderr);
}

/// A strict run's seal generation is packed at its path and verified from the
/// archive. §FS-rhei-archive.2.2
#[test]
fn a_strict_runs_seal_generation_is_packed_and_verified() {
    let ws = strict_run("archive-seal");
    let (_, manifest) = only_manifest(&ws.evidence_dir());
    let seal = manifest["seals"][0]["path"].as_str().expect("a named seal").to_string();
    let (_keep, kept) = elsewhere("archive-seal-kept");
    let tar = kept.join("run.tar");
    archive_ok(&ws, &tar);
    assert!(members(&tar).iter().any(|(path, _)| *path == seal), "{seal} is packed");
    fs::remove_dir_all(&ws.root).expect("remove the workspace");
    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    assert!(result.status.success(), "the seal verifies: {json:#}\n{}", result.stderr);
}

/// `rhei archive --json` is the verify object plus `archive: {path, sha256}`.
/// §FS-rhei-archive.5.1
#[test]
fn archive_json_is_the_verify_object_plus_the_file_written() {
    let ws = evidence_workspace("archive-json", Profile::default());
    ws.run();
    let run = only_run_id(&ws.root);
    let (_keep, kept) = elsewhere("archive-json-kept");
    let tar = kept.join("run.tar");
    let result = archive(&ws.root, &run, &tar, &["--json"]);
    let json: serde_json::Value = serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("archive --json prints one object: {err}\n{}\n{}", result.stdout, result.stderr)
    });
    assert!(result.status.success(), "{json:#}");
    assert_eq!(json["verified"], true);
    assert_eq!(json["runs"].as_array().map(Vec::len), Some(1), "{json:#}");
    assert_eq!(json["runs"][0]["run_id"], run.as_str());
    assert!(json["findings"].is_array(), "{json:#}");
    let written = json["archive"]["path"].as_str().expect("archive.path");
    assert_eq!(fs::canonicalize(written).unwrap(), fs::canonicalize(&tar).unwrap());
    assert_eq!(json["archive"]["sha256"], sha256_hex(&fs::read(&tar).unwrap()));
}

/// Both commands' help name the archive. §FS-rhei-archive.6 §FS-rhei-verify.1
#[test]
fn help_names_the_archive() {
    let (_dir, here) = elsewhere("archive-help");
    let help = rhei_in(&here, &here.join(".home"), &["archive", "--help"]);
    assert!(help.status.success(), "{}", help.stderr);
    assert!(help.stdout.contains("--output") && help.stdout.contains("--run"), "{}", help.stdout);
    let verify = rhei_in(&here, &here.join(".home"), &["verify", "--help"]);
    assert!(verify.stdout.to_lowercase().contains("archive"), "{}", verify.stdout);
}
