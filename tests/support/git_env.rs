// Git's repository variables, kept out of everything the suite spawns.
//
// Shared verbatim by the CLI's two harnesses — one pulls it in with `#[path]`,
// the other with `include!` into its flat module — so the comments here are
// `//` rather than `//!`: an inner doc comment cannot open an included file.
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// Take git's repository variables off `cmd`, so the child chooses the
/// repository it acts on instead of inheriting one.
///
/// Both sources count, because both reach the child: a `GIT_*` this harness
/// process inherited — which is how a suite run from a hook is handed a
/// `GIT_DIR` naming the repository being committed to — and a `GIT_*` a caller
/// has already put on the command. What a caller sets *after* this is the
/// deliberate value and is kept, which is how `GIT_CEILING_DIRECTORIES` still
/// bounds a fixture's ancestor discovery to its own directory.
///
/// §REQ-test-isolation.3
pub fn scrub_git_repository_env(cmd: &mut Command) {
    // Deliberately empty. The removal is the behaviour §REQ-test-isolation.3
    // asks for, and it is written by the change the tests around this seam
    // pin — not by the commit that writes them.
    let _ = cmd;
}

/// Every `git` the suite spawns.
///
/// The one place in the suite that names `git` as a program, so that the
/// removal above is not a thing a call site can forget. §REQ-test-isolation.3
pub fn git_command() -> Command {
    let mut cmd = Command::new("git");
    scrub_git_repository_env(&mut cmd);
    cmd
}

/// The variables git exports into a hook, pointed at `repo`.
///
/// These are the six observed to do damage, which is not the same as the set
/// [`scrub_git_repository_env`] covers — that is every `GIT_*`
/// (§REQ-test-isolation.3). `GIT_WORK_TREE` is deliberately absent: git then
/// reads the repository from `GIT_DIR` and only the work tree from the current
/// directory, which is the shape in which `-C <fixture>` looks like isolation
/// and is not.
pub fn repository_env_as_a_hook_leaves_it(repo: &Path) -> Vec<(&'static str, OsString)> {
    let git_dir = repo.join(".git");
    vec![
        ("GIT_DIR", git_dir.clone().into_os_string()),
        ("GIT_COMMON_DIR", git_dir.clone().into_os_string()),
        ("GIT_INDEX_FILE", git_dir.join("index").into_os_string()),
        ("GIT_OBJECT_DIRECTORY", git_dir.join("objects").into_os_string()),
        ("GIT_PREFIX", OsString::new()),
        ("GIT_CONFIG_PARAMETERS", OsString::from("'user.name'='Rhei Test'")),
    ]
}

/// Four more names, pointed at `repo`, that no report about this mentioned.
///
/// They are here because the rule is every `GIT_*` rather than the list that
/// happened to do damage (§REQ-test-isolation.3), and a list is what the next
/// variable gets left off.
pub fn repository_env_names_no_report_mentioned(repo: &Path) -> Vec<(&'static str, OsString)> {
    let value = repo.as_os_str().to_os_string();
    vec![
        ("GIT_WORK_TREE", value.clone()),
        ("GIT_ALTERNATE_OBJECT_DIRECTORIES", value.clone()),
        ("GIT_NAMESPACE", value.clone()),
        ("GIT_CEILING_DIRECTORIES", value),
    ]
}

/// [`git_command`] with `staged` put on the child *first*, the way a hook
/// leaves git's repository variables in the environment the suite inherits.
///
/// A stand-in, because the real thing cannot be staged: putting `GIT_DIR` into
/// this process's own environment would race every other test in the binary and
/// is unsafe besides. Setting it on the child and letting the removal run
/// afterwards exercises the same seam in the same order, deterministically.
/// Whatever a caller sets on the returned command is the deliberate value.
pub fn git_command_with_staged_env(staged: &[(&str, OsString)]) -> Command {
    let mut cmd = Command::new("git");
    for (key, value) in staged {
        cmd.env(key, value);
    }
    scrub_git_repository_env(&mut cmd);
    cmd
}

/// [`git_command_with_staged_env`] with exactly what a hook leaves, pointed at
/// `repo` — a repository no test named, so anything the child writes into it is
/// a write §REQ-test-isolation.3 forbids.
pub fn git_command_as_if_inherited(repo: &Path) -> Command {
    git_command_with_staged_env(&repository_env_as_a_hook_leaves_it(repo))
}
