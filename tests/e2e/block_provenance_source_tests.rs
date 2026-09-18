//! Source records make replay guarantees only when the recorded identity is immutable.
//! §FS-rhei-library.4.1
use std::fs;
use std::path::Path;
use std::process::Command;

use super::block_composition_support::*;
use super::block_provenance_support::*;
use super::*;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git should run for provenance fixture");
    assert!(status.success(), "git {args:?} should succeed");
}

fn instantiate_one(dir: &Path, source: &str, output_name: &str) -> std::path::PathBuf {
    let output = dir.join(output_name);
    let mount = format!("sample={source}");
    assert_success(&run_compose(
        dir,
        &["instantiate", "--mount", &mount, "--output", output.to_str().expect("output path")],
    ));
    output
}

fn only_mounted_revision(lock: &serde_json::Value) -> &serde_json::Value {
    let mount = lock["mounts"]
        .as_object()
        .expect("mount table")
        .values()
        .find(|mount| mount["chain"] == serde_json::json!(["sample"]))
        .expect("sample mount");
    let source = mount["source"].as_str().expect("source id");
    &lock["sources"][source]["revision"]
}

#[test]
fn source_identity_distinguishes_shipped_clean_dirty_untracked_local_and_unavailable() {
    let dir = provenance_test_dir("sources");
    let repo = dir.join("repo");
    fs::create_dir_all(&repo).expect("git fixture root");
    let clean = write_composition_fixtures(&repo.join("clean")).review;
    let dirty = write_composition_fixtures(&repo.join("dirty")).review;
    let untracked = write_composition_fixtures(&repo.join("untracked")).review;
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Rhei Test",
            "-c",
            "user.email=rhei@example.invalid",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
    );
    fs::write(dirty.join("prompt_templates/shared.md"), "dirty tracked source bytes\n")
        .expect("dirty source");
    fs::write(untracked.join("extra.txt"), "untracked source bytes\n").expect("untracked source");

    let local = write_composition_fixtures(&dir.join("outside-git")).review;
    let unavailable_root = dir.join("unavailable");
    let unavailable = write_composition_fixtures(&unavailable_root).review;
    fs::write(unavailable_root.join(".git"), "gitdir: missing-metadata\n")
        .expect("broken git metadata marker");

    let outputs = [
        instantiate_one(&dir, "fix", "built-in"),
        instantiate_one(&dir, clean.to_str().expect("clean path"), "clean"),
        instantiate_one(&dir, dirty.to_str().expect("dirty path"), "dirty"),
        instantiate_one(&dir, untracked.to_str().expect("untracked path"), "untracked"),
        instantiate_one(&dir, local.to_str().expect("local path"), "local"),
        instantiate_one(
            &dir,
            unavailable.to_str().expect("unavailable path"),
            "unavailable-output",
        ),
    ];
    let locks = outputs.iter().map(|output| read_lock(output)).collect::<Vec<_>>();

    let expected = [
        ("shipped", "exact"),
        ("clean", "exact"),
        ("dirty", "not-guaranteed"),
        ("untracked", "not-guaranteed"),
        ("unversioned", "not-guaranteed"),
        ("unavailable", "not-guaranteed"),
    ];
    for (lock, (status, replay)) in locks.iter().zip(expected) {
        let revision = only_mounted_revision(lock);
        assert_eq!(revision["status"], status);
        assert_eq!(revision["replay"], replay);
        assert!(revision["content"].as_str().is_some_and(|value| value.starts_with("sha256:")));
    }
    assert!(only_mounted_revision(&locks[0])["release"].is_string());
    assert!(only_mounted_revision(&locks[1])["commit"].is_string());
    assert!(only_mounted_revision(&locks[5])["commit"].is_null());

    for lock in &locks {
        for source in lock["sources"].as_object().expect("sources").values() {
            let locator = &source["locator"];
            if locator["requested"].as_str().is_some_and(|path| Path::new(path).is_absolute()) {
                assert_eq!(locator["portable"], false, "absolute locator must be host-local");
            }
        }
    }
}
