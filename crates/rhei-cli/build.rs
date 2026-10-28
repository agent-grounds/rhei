//! Records the commit rhei is built from, as `RHEI_BUILD_COMMIT` and
//! `RHEI_BUILD_DIRTY`, for `rhei version` and every run record.
//!
//! §FS-rhei-version.2: both variables are always set, `unknown` included, so
//! the `env!` that reads them never fails a build, `cargo publish`'s verify
//! build among them. The build is redone when what it read changes.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "src/build_identity.rs"]
#[cfg_attr(not(test), allow(dead_code))]
mod build_identity;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build_identity.rs");
    println!("cargo:rerun-if-env-changed=RHEI_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=RHEI_BUILD_DIRTY");

    let env_commit = std::env::var("RHEI_BUILD_COMMIT").ok();
    let env_dirty = std::env::var("RHEI_BUILD_DIRTY").ok();
    let vcs_info_path = manifest_dir.join(".cargo_vcs_info.json");
    let vcs_info = std::fs::read_to_string(&vcs_info_path).ok();
    if vcs_info.is_some() {
        println!("cargo:rerun-if-changed=.cargo_vcs_info.json");
    }
    let mut git = |args: &[&str]| git_in(&manifest_dir, args);
    let identity = build_identity::resolve(
        env_commit.as_deref(),
        env_dirty.as_deref(),
        vcs_info.as_deref(),
        &mut git,
    );
    if vcs_info.is_none() {
        for path in watched_paths(&manifest_dir) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let (commit, dirty) = identity.env_values();
    println!("cargo:rustc-env=RHEI_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RHEI_BUILD_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=RHEI_BUILD_VERSION_LABEL={} ({})",
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
        identity.parenthetical()
    );
}

/// `git --no-optional-locks <args>` in `dir`: trimmed stdout, or `None`.
fn git_in(dir: &Path, args: &[&str]) -> Option<String> {
    let out =
        Command::new("git").arg("--no-optional-locks").args(args).current_dir(dir).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// HEAD, the branch ref HEAD names, `packed-refs` and `logs/HEAD` as
/// `--git-path` locates them (a worktree's `.git` is a file), plus the three
/// paths the dirty check reads; only those that exist, since cargo treats a
/// missing path as always changed. The index is not watched.
fn watched_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Some(top) = git_in(manifest_dir, &["rev-parse", "--show-toplevel"]) else {
        return paths;
    };
    let mut git_paths = vec!["HEAD".to_string(), "packed-refs".into(), "logs/HEAD".into()];
    if let Some(branch) = git_in(manifest_dir, &["symbolic-ref", "-q", "HEAD"]) {
        git_paths.push(branch);
    }
    for name in git_paths {
        if let Some(path) = git_in(manifest_dir, &["rev-parse", "--git-path", &name]) {
            paths.push(manifest_dir.join(path));
        }
    }
    let top = PathBuf::from(top);
    paths.extend(["crates", "Cargo.toml", "Cargo.lock"].map(|name| top.join(name)));
    paths.retain(|path| path.exists());
    paths
}
