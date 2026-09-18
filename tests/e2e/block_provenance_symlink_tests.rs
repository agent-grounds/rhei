//! Real Git symlinks must inventory consumed bytes and establish immutable replay honestly.
//! §FS-rhei-library.4.1
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::symlink;

use super::block_provenance_source_tests::{git, instantiate_one, only_mounted_revision};
use super::block_provenance_support::*;

#[test]
fn source_identity_tracks_effective_symlink_bytes_and_resolved_escapes() {
    let dir = provenance_test_dir("symlinks");
    for case in ["ignored", "tracked", "escape", "intermediate"] {
        let repo = dir.join(case);
        let block = repo.join("block");
        write_provenance_block(&block, false);
        let escaping = matches!(case, "escape" | "intermediate");
        let target = if escaping { repo.join("outside.yaml") } else { block.join(".payload.yaml") };
        fs::rename(block.join("states.yaml"), &target).unwrap();
        let link = match case {
            "intermediate" => {
                symlink("../outside.yaml", block.join(".bridge")).unwrap();
                ".bridge"
            }
            "escape" => "../outside.yaml",
            _ => ".payload.yaml",
        };
        symlink(link, block.join("states.yaml")).unwrap();
        if case == "ignored" {
            fs::write(repo.join(".gitignore"), "block/.payload.yaml\n").unwrap();
        }
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
                "symlink fixture",
            ],
        );

        let before = instantiate_one(&dir, block.to_str().unwrap(), &format!("{case}-before"));
        let first = read_lock(&before);
        let revision = only_mounted_revision(&first);
        assert_eq!(revision["status"], if case == "ignored" { "untracked" } else { "clean" });
        assert_eq!(revision["replay"], if case == "tracked" { "exact" } else { "not-guaranteed" });
        assert!(revision["commit"].is_string());

        let original = fs::read_to_string(&target).unwrap();
        fs::write(
            &target,
            original.replace("description: Work", "description: Changed consumed bytes"),
        )
        .unwrap();
        let after = instantiate_one(&dir, block.to_str().unwrap(), &format!("{case}-after"));
        let second = read_lock(&after);
        let changed = only_mounted_revision(&second);
        assert_eq!(revision["commit"], changed["commit"]);
        assert_ne!(
            revision["content"], changed["content"],
            "effective bytes must enter the digest: {case}"
        );
        assert_eq!(changed["status"], if case == "ignored" { "untracked" } else { "dirty" });
        assert_eq!(changed["replay"], "not-guaranteed");
        assert!(fs::read_to_string(after.join("states.yaml"))
            .unwrap()
            .contains("Changed consumed bytes"));
    }
}

#[test]
fn source_identity_checks_hidden_intermediate_links_at_the_commit() {
    let dir = provenance_test_dir("intermediate-link");
    let block = dir.join("repo/block");
    write_provenance_block(&block, false);
    fs::rename(block.join("states.yaml"), block.join(".payload.yaml")).unwrap();
    symlink(".payload.yaml", block.join(".bridge")).unwrap();
    symlink(".bridge", block.join("states.yaml")).unwrap();
    let repo = block.parent().unwrap();
    fs::write(repo.join(".gitignore"), "block/.bridge\n").unwrap();
    git(repo, &["init", "-q"]);
    git(repo, &["add", "."]);
    git(
        repo,
        &[
            "-c",
            "user.name=Rhei Test",
            "-c",
            "user.email=rhei@example.invalid",
            "commit",
            "-q",
            "-m",
            "intermediate fixture",
        ],
    );
    let output = instantiate_one(&dir, block.to_str().unwrap(), "output");
    let lock = read_lock(&output);
    let revision = only_mounted_revision(&lock);
    assert_eq!(revision["status"], "untracked");
    assert_eq!(
        revision["replay"], "not-guaranteed",
        "a tracked endpoint cannot make an ignored intermediate link replayable"
    );
}
