// The `rhei` binary the two test homes drive as a subprocess. Cargo names a
// package's own binaries to its tests and nobody else's, so a test living
// outside `crates/rhei-cli` finds it beside itself in the profile directory it
// was built into. Before either harness first uses it, Cargo verifies that the
// binary matches the current checkout and rebuilds it when needed. The result
// is shared for the rest of that harness process.
//
// Shared by both homes: one pulls it in with `#[path]` into its module tree,
// the other with `include!` into its flat module — so this comment is `//`
// rather than `//!`, matching `support/test_dir.rs` and
// `support/python_fixture.rs`, since an inner doc comment cannot open an
// included file.
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, PartialEq, Eq)]
enum BinaryVerificationError {
    BuildFailed,
    MissingOutput,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `target/<profile>/`, read off the running test binary's own location.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current test binary");
    let deps = exe.parent().expect("deps dir");
    assert_eq!(
        deps.file_name().and_then(|name| name.to_str()),
        Some("deps"),
        "unexpected test binary location {}",
        exe.display()
    );
    deps.parent().expect("profile dir").to_path_buf()
}

/// One freshness build: the binary the harness will execute, and the `cargo`
/// arguments meant to produce it.
///
/// Both are derived from the same profile directory, so the directory the
/// harness checks and the directory the nested build is directed at cannot
/// drift apart. §AR-ci-release.1
struct FreshnessBuild {
    binary: PathBuf,
    args: Vec<OsString>,
}

impl FreshnessBuild {
    /// Derived from the profile directory the running test binary sits in, so it
    /// follows whichever target directory this run was given. The nested build is
    /// directed at that directory with `--target-dir`, because `--target-dir` on
    /// the outer `cargo test` reaches no child process and an inherited
    /// `CARGO_TARGET_DIR` can name a third place. A profile directory with no
    /// parent cannot come out of `profile_dir()`, which asserts its own shape, so
    /// that case leaves the flag off and lets Cargo choose rather than panicking
    /// in a helper that has nothing useful to say. §AR-ci-release.1
    fn for_profile_dir(profile_dir: &Path) -> Self {
        let binary = profile_dir.join(format!("rhei{}", std::env::consts::EXE_SUFFIX));
        let mut args: Vec<OsString> =
            ["build", "-p", "rhei-cli", "--locked"].into_iter().map(OsString::from).collect();
        // Cargo lays profile directories under the target directory, so the parent is
        // what the flag takes: `<target>`, or `<target>/<triple>` cross-compiled.
        if let Some(target_dir) = profile_dir.parent() {
            args.push("--target-dir".into());
            args.push(target_dir.as_os_str().to_os_string());
        }
        if profile_dir.file_name().and_then(|name| name.to_str()) == Some("release") {
            args.push("--release".into());
        }
        Self { binary, args }
    }

    /// The diagnostic for a build that succeeded and left no binary at `binary`.
    /// It quotes the build that ran after the path it checked, so a reader sees
    /// which directory the rebuild was aimed at instead of going looking for a
    /// broken checkout. The opening sentence is fixed: the reproducer for
    /// agent-grounds/rhei#181 recognises the defect by it. §AR-ci-release.1
    fn missing_output_message(&self) -> String {
        let args: Vec<String> =
            self.args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
        format!("no rhei binary at {} after `cargo {}`", self.binary.display(), args.join(" "))
    }
}

fn verify_rhei_binary(
    path: &Path,
    build: impl FnOnce() -> bool,
) -> Result<(), BinaryVerificationError> {
    if !build() {
        return Err(BinaryVerificationError::BuildFailed);
    }
    if !path.is_file() {
        return Err(BinaryVerificationError::MissingOutput);
    }
    Ok(())
}

/// The binary every subprocess use runs, once `build` has verified its output.
///
/// Today it is the profile directory's own `rhei`, the path Cargo unlinks and
/// relinks whenever any freshness build replaces it, so a spawn that lands in
/// that gap fails with NotFound (agent-grounds/rhei#689). It must be a binary no
/// later build can pull out from under a running test, kept under `build`'s own
/// target directory and still named `rhei` for the platform. §AR-ci-release.1
fn spawnable_binary(build: &FreshnessBuild) -> PathBuf {
    build.binary.clone()
}

// §AR-ci-release.1
pub fn rhei_binary() -> PathBuf {
    static RHEI_BINARY: OnceLock<PathBuf> = OnceLock::new();
    RHEI_BINARY
        .get_or_init(|| {
            let build = FreshnessBuild::for_profile_dir(&profile_dir());
            let result = verify_rhei_binary(&build.binary, || {
                let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
                Command::new(cargo)
                    .args(&build.args)
                    .current_dir(repo_root())
                    .status()
                    .unwrap_or_else(|err| panic!("run cargo build -p rhei-cli: {err}"))
                    .success()
            });
            match result {
                Ok(()) => spawnable_binary(&build),
                Err(BinaryVerificationError::BuildFailed) => {
                    panic!("cargo build -p rhei-cli failed")
                }
                Err(BinaryVerificationError::MissingOutput) => {
                    panic!("{}", build.missing_output_message())
                }
            }
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    use super::{spawnable_binary, verify_rhei_binary, BinaryVerificationError, FreshnessBuild};

    /// A profile directory under a target directory of the shape a caller gives
    /// with `--target-dir`. Built from components, so the assertions read the
    /// same on Windows, and never touched on disk — the seam is pure.
    fn supplied_profile_dir(profile: &str) -> PathBuf {
        Path::new("supplied-target").join(profile)
    }

    fn target_dir_arg(args: &[OsString]) -> Option<&OsStr> {
        args.iter()
            .position(|arg| arg == "--target-dir")
            .and_then(|flag| args.get(flag + 1))
            .map(OsString::as_os_str)
    }

    /// The nested build is directed at the directory the harness checks, so a
    /// run given a target directory of its own rebuilds where the harness is
    /// looking. §AR-ci-release.1
    #[test]
    fn freshness_build_is_directed_at_the_profile_directory_it_checks() {
        let profile_dir = supplied_profile_dir("debug");

        let build = FreshnessBuild::for_profile_dir(&profile_dir);

        let target_dir = target_dir_arg(&build.args).unwrap_or_else(|| {
            panic!("the nested build names no target directory: {:?}", build.args)
        });
        assert_eq!(Path::new(target_dir), profile_dir.parent().expect("a target directory"));
        assert!(
            build.binary.starts_with(target_dir),
            "{} is not under the target directory {}",
            build.binary.display(),
            Path::new(target_dir).display()
        );
    }

    /// Directing the rebuild does not drop what the profile directory already
    /// told the harness. §AR-ci-release.1
    #[test]
    fn freshness_build_keeps_the_release_flag_for_a_release_profile() {
        let release = FreshnessBuild::for_profile_dir(&supplied_profile_dir("release"));
        let debug = FreshnessBuild::for_profile_dir(&supplied_profile_dir("debug"));

        assert!(release.args.iter().any(|arg| arg == "--release"), "{:?}", release.args);
        assert!(!debug.args.iter().any(|arg| arg == "--release"), "{:?}", debug.args);
    }

    /// A build that succeeded and left nothing behind says the path it checked
    /// and the build it ran, not the path alone. §AR-ci-release.1
    #[test]
    fn missing_output_names_the_path_checked_and_the_build_that_ran() {
        let build = FreshnessBuild::for_profile_dir(&supplied_profile_dir("debug"));

        let message = build.missing_output_message();

        assert!(
            message.contains(&build.binary.display().to_string()),
            "{message:?} does not name the path it checked"
        );
        for arg in &build.args {
            assert!(
                message.contains(arg.to_string_lossy().as_ref()),
                "{message:?} does not name the nested build's {arg:?}"
            );
        }
    }

    #[test]
    fn existing_binary_still_runs_freshness_build() {
        let existing = std::env::current_exe().expect("current test binary");
        let mut builds = 0;

        let result = verify_rhei_binary(&existing, || {
            builds += 1;
            true
        });

        assert_eq!(result, Ok(()));
        assert_eq!(builds, 1);
    }

    #[test]
    fn failed_freshness_build_rejects_existing_binary() {
        let existing = std::env::current_exe().expect("current test binary");

        let result = verify_rhei_binary(&existing, || false);

        assert_eq!(result, Err(BinaryVerificationError::BuildFailed));
    }

    /// A profile directory of its own under the running test binary's target
    /// directory: the same filesystem as the live one, so a hard link works,
    /// but never the live `target/<profile>/rhei` a running suite spawns.
    fn scratch_profile_dir(name: &str) -> PathBuf {
        let exe = std::env::current_exe().expect("current test binary");
        let target = exe.ancestors().nth(3).expect("the running test binary's target directory");
        target
            .join("rhei-test-scratch")
            .join(format!("{name}-{}", std::process::id()))
            .join("debug")
    }

    /// Stands in for the uplift path: any runnable program that exits 0 on
    /// every platform. The running test binary is one: `--list` with a filter
    /// that matches nothing prints an empty list and exits 0.
    fn lay_stand_in_binary(at: &Path) {
        let exe = std::env::current_exe().expect("current test binary");
        std::fs::create_dir_all(at.parent().expect("a profile directory"))
            .expect("create the scratch profile directory");
        if std::fs::hard_link(&exe, at).is_err() {
            std::fs::copy(&exe, at).expect("copy the stand-in binary");
        }
    }

    /// A relink of the profile directory's own `rhei` - Cargo's unlink, then
    /// link - does not take away the binary the harness already resolved: a test
    /// that spawns it in that gap still runs it. agent-grounds/rhei#689
    /// §AR-ci-release.1
    #[test]
    #[ignore = "red until #689: a relink of target/<profile>/rhei removes the binary a test spawns; implement removes this"]
    fn spawnable_binary_survives_a_relink_of_the_profile_binary() {
        let profile_dir = scratch_profile_dir("relink");
        let scratch = profile_dir.parent().expect("the scratch target directory").to_path_buf();
        let _ = std::fs::remove_dir_all(&scratch);
        let build = FreshnessBuild::for_profile_dir(&profile_dir);
        lay_stand_in_binary(&build.binary);

        let resolved = spawnable_binary(&build);
        // The first half of Cargo's uplift; the second half links a new inode in.
        std::fs::remove_file(&build.binary).expect("unlink the profile binary");
        let spawned = std::process::Command::new(&resolved)
            .args(["--list", "no-test-is-named-this-689"])
            .output();
        lay_stand_in_binary(&build.binary);
        let respawned = std::process::Command::new(&resolved)
            .args(["--list", "no-test-is-named-this-689"])
            .output();
        let _ = std::fs::remove_dir_all(&scratch);

        let spawned = spawned.unwrap_or_else(|err| {
            panic!("spawning {} after the profile binary was unlinked: {err:?}", resolved.display())
        });
        assert!(spawned.status.success(), "{} exited {}", resolved.display(), spawned.status);
        let respawned = respawned.unwrap_or_else(|err| {
            panic!("spawning {} after the profile binary was relinked: {err:?}", resolved.display())
        });
        assert!(respawned.status.success(), "{} exited {}", resolved.display(), respawned.status);
        assert_eq!(
            resolved.file_name(),
            build.binary.file_name(),
            "the spawned binary keeps the platform's `rhei` name"
        );
        assert!(
            resolved.starts_with(&scratch),
            "{} is not under the build's target directory {}",
            resolved.display(),
            scratch.display()
        );
    }
}
