//! Source records make replay guarantees only when the recorded identity is immutable.
//! §FS-rhei-library.4.1
use std::fs;
use std::path::Path;
use std::process::Command;

use super::block_composition_support::*;
use super::block_provenance_support::*;
use super::*;

pub(super) fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git should run for provenance fixture");
    assert!(status.success(), "git {args:?} should succeed");
}

pub(super) fn instantiate_one(dir: &Path, source: &str, output_name: &str) -> std::path::PathBuf {
    let output = dir.join(output_name);
    let mount = format!("sample={source}");
    let result = rhei_command(dir.join(".home"))
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", fs::canonicalize(dir).expect("discovery ceiling"))
        .args(["instantiate", "--mount", &mount, "--output", output.to_str().expect("output path")])
        .output()
        .expect("compose source fixture");
    assert_success(&CliRun::from(&result));
    output
}

pub(super) fn only_mounted_revision(lock: &serde_json::Value) -> &serde_json::Value {
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
    assert_eq!(only_mounted_revision(&locks[4])["kind"], "local");
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

#[test]
fn local_composition_survives_missing_git_but_preserves_source_errors() {
    let dir = provenance_test_dir("missing-git");
    let block = dir.join("block");
    write_provenance_block(&block, false);
    let empty_path = dir.join("empty-path");
    fs::create_dir(&empty_path).unwrap();
    let mount = mount_arg("sample", &block);
    let compose = |name: &str| {
        rhei_command(dir.join(".home"))
            .current_dir(&dir)
            .env("PATH", &empty_path)
            .args(["instantiate", "--mount", &mount, "--output", dir.join(name).to_str().unwrap()])
            .output()
            .unwrap()
    };
    assert_success(&CliRun::from(&compose("output")));
    let lock = read_lock(&dir.join("output"));
    let revision = only_mounted_revision(&lock);
    assert_eq!(revision["kind"], "unavailable");
    assert_eq!(revision["status"], "unavailable");
    assert_eq!(revision["replay"], "not-guaranteed");
    assert!(revision["commit"].is_null());
    assert!(revision["content"].as_str().unwrap().starts_with("sha256:"));
    let source_id = lock["mounts"]["m6_sample__"]["source"].as_str().unwrap();
    assert_eq!(
        lock["sources"][source_id]["locator"]["requested"],
        block.to_str().unwrap().replace('\\', "/")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let git_path = empty_path.join("git");
        fs::write(&git_path, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&git_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_success(&CliRun::from(&compose("unexecutable")));
        let unavailable = read_lock(&dir.join("unexecutable"));
        assert_eq!(only_mounted_revision(&unavailable)["kind"], "unavailable");
        assert_eq!(only_mounted_revision(&unavailable)["replay"], "not-guaranteed");
    }

    fs::write(block.join("states.yaml"), "invalid: [unterminated\n").unwrap();
    assert_failed_with(&CliRun::from(&compose("invalid")), &["authored state fragment"]);
    assert!(!dir.join("invalid").exists());
}
