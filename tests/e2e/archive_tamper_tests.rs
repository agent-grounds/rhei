//! `rhei verify <archive>` on an archive changed after it was packed: an edited,
//! removed or added member, a reset event planted to hide an edit, unsafe
//! members, and tar files that cannot be read. §FS-rhei-verify.2.1
//! §FS-rhei-verify.3

use std::fs;
use std::path::{Path, PathBuf};

use super::archive_support::{
    append, append_raw, archive_ok, assert_refused, elsewhere, flip_member_byte, index, members,
    rewrite, verify_archive_json, with_appended,
};
use super::evidence_support::{evidence_workspace, only_manifest, sha256_hex, Profile};
use super::TestDir;

/// A packed probe run, its workspace removed: `(kept dir guard, kept dir,
/// archive, the manifest's logs)`.
fn packed(prefix: &str) -> (TestDir, PathBuf, PathBuf, Vec<String>) {
    let ws = evidence_workspace(prefix, Profile::default());
    ws.run();
    let (_, manifest) = only_manifest(&ws.evidence_dir());
    let logs: Vec<String> = manifest["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .filter(|path| path.starts_with("runtime/logs/"))
        .map(str::to_string)
        .collect();
    assert!(!logs.is_empty(), "fixture: the manifest names logs: {manifest:#}");
    let (guard, kept) = elsewhere(&format!("{prefix}-kept"));
    let tar = kept.join("run.tar");
    archive_ok(&ws, &tar);
    fs::remove_dir_all(&ws.root).expect("remove the workspace");
    (guard, kept, tar, logs)
}

/// One byte of a log changed inside the tar is a mismatch with both hashes.
/// §FS-rhei-verify.3
#[test]
#[ignore = "red until #525: verify reads no archive; implement removes this"]
fn an_edited_member_is_a_mismatch() {
    let (_guard, kept, tar, logs) = packed("archive-edited");
    let recorded = sha256_hex(&members(&tar).into_iter().find(|(p, _)| *p == logs[0]).unwrap().1);
    flip_member_byte(&tar, &logs[0]);
    let (result, json) = verify_archive_json(&kept, &tar, &[]);
    let found = assert_refused(&result, &json, "mismatch");
    assert_eq!(found["path"], logs[0].as_str());
    assert_eq!(found["recorded_sha256"], recorded);
    let found_hash = found["found_sha256"].as_str().expect("found_sha256");
    assert_ne!(found_hash, recorded, "{found:#}");
}

/// A member taken out is missing; a member added is unlisted.
/// §FS-rhei-verify.2.1 §FS-rhei-verify.3
#[test]
#[ignore = "red until #525: verify reads no archive; implement removes this"]
fn a_removed_member_is_missing_and_an_added_one_unlisted() {
    let (_guard, kept, tar, logs) = packed("archive-missing");
    let removed = kept.join("removed.tar");
    rewrite(&tar, &removed, |path, bytes| (path != logs[0]).then_some(bytes), &[]);
    let (result, json) = verify_archive_json(&kept, &removed, &[]);
    assert_eq!(assert_refused(&result, &json, "missing")["path"], logs[0].as_str());

    let added = kept.join("added.tar");
    let extra = ("runtime/logs/planted.log".to_string(), b"planted\n".to_vec());
    rewrite(&tar, &added, |_, bytes| Some(bytes), &[extra]);
    let (result, json) = verify_archive_json(&kept, &added, &[]);
    assert_eq!(assert_refused(&result, &json, "unlisted")["path"], "runtime/logs/planted.log");
}

/// A reset event planted beside an edited log does not hide the edit: the
/// packer never writes a member at a pruned path, so the log is unlisted,
/// even under the manifest's `--digest`. §FS-rhei-verify.2.1
#[test]
#[ignore = "red until #525: verify reads no archive; implement removes this"]
fn a_planted_reset_event_turns_the_edited_member_unlisted() {
    let (_guard, kept, tar, logs) = packed("archive-planted-event");
    let packed_index = index(&tar);
    let run_id = packed_index["run_id"].as_str().expect("run_id").to_string();
    let digest = packed_index["manifest"]["sha256"].as_str().expect("digest").to_string();
    let event_path = "runtime/evidence/events/00000000-0000-4000-8000-000000000000.json";
    let event = serde_json::json!({
        "schema": "rhei.evidence.event.v1", "event_id": "00000000-0000-4000-8000-000000000000",
        "kind": "reset", "at": "2026-10-29T00:00:00Z", "rheis": ["plan"], "tickets": [],
        "runs": [run_id], "removed": [logs[0]],
    });
    let mut planted_index = packed_index.clone();
    planted_index["events"] = serde_json::json!([event_path]);
    let edited = kept.join("edited.tar");
    rewrite(
        &tar,
        &edited,
        |path, mut bytes| {
            if path == "rhei-archive.json" {
                return Some(serde_json::to_vec(&planted_index).unwrap());
            }
            if path == logs[0] {
                bytes.extend_from_slice(b"edited\n");
            }
            Some(bytes)
        },
        &[(event_path.to_string(), serde_json::to_vec(&event).unwrap())],
    );
    let (result, json) = verify_archive_json(&kept, &edited, &["--digest", &digest]);
    assert_eq!(assert_refused(&result, &json, "unlisted")["path"], logs[0].as_str());
}

/// Each unsafe member is refused with its code, and nothing is written.
/// §FS-rhei-verify.2.1 §FS-rhei-verify.3
#[test]
#[ignore = "red until #525: verify reads no archive; implement removes this"]
fn unsafe_members_are_refused() {
    let (_guard, kept, tar, logs) = packed("archive-unsafe");
    let work = kept.join("work");
    fs::create_dir_all(&work).expect("a working directory");
    type Add = Box<dyn FnOnce(&mut tar::Builder<Vec<u8>>)>;
    let duplicate = logs[0].clone();
    let cases: Vec<(&str, Add)> = vec![
        ("parent", Box::new(|b| append_raw(b, "../x", tar::EntryType::Regular, None, b"x"))),
        ("absolute", Box::new(|b| append_raw(b, "/etc/x", tar::EntryType::Regular, None, b"x"))),
        (
            "symlink",
            Box::new(|b| append_raw(b, "runtime/link", tar::EntryType::Symlink, Some("/etc"), b"")),
        ),
        ("duplicate", Box::new(move |b| append(b, &duplicate, b"again\n"))),
    ];
    for (name, add) in cases {
        let bad = kept.join(format!("{name}.tar"));
        with_appended(&tar, &bad, add);
        let (result, json) = verify_archive_json(&work, &bad, &[]);
        assert_refused(&result, &json, "archive_unsafe_member");
        assert!(!kept.join("x").exists(), "{name}: verify wrote outside its directory");
    }
}

/// Bytes that are a tar but not a whole archive are `archive_unreadable`, with
/// no run. §FS-rhei-verify.1 §FS-rhei-verify.3
#[test]
#[ignore = "red until #525: verify reads no archive; implement removes this"]
fn an_unreadable_archive_is_refused_with_no_run() {
    let (_guard, kept, tar, _) = packed("archive-unreadable");
    let bytes = fs::read(&tar).expect("archive bytes");
    let truncated = kept.join("truncated.tar");
    fs::write(&truncated, &bytes[..bytes.len() / 2 + 7]).expect("truncate");
    let trailing = kept.join("trailing.tar");
    let mut garbage = bytes.clone();
    garbage.extend_from_slice(b"appended after the end of the archive\n");
    fs::write(&trailing, garbage).expect("append");
    let reordered = kept.join("reordered.tar");
    let mut all = members(&tar);
    let first = all.remove(0);
    all.push(first);
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in &all {
        append(&mut builder, path, bytes);
    }
    fs::write(&reordered, builder.into_inner().unwrap()).expect("write");

    for bad in [&truncated, &trailing, &reordered] {
        assert_unreadable(&kept, bad);
    }
}

fn assert_unreadable(cwd: &Path, archive: &Path) {
    let (result, json) = verify_archive_json(cwd, archive, &[]);
    let found = assert_refused(&result, &json, "archive_unreadable");
    assert!(found["run_id"].is_null(), "{}: {found:#}", archive.display());
}
