// Git's repository variables, kept out of everything the suite spawns.
//
// Shared verbatim by the CLI's two harnesses — one pulls it in with `#[path]`,
// the other with `include!` into its flat module — so the comments here are
// `//` rather than `//!`: an inner doc comment cannot open an included file.
#![allow(dead_code)]

use std::ffi::{OsStr, OsString};
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
    // Both lists are collected before anything is removed: `get_envs` borrows
    // `cmd`, so a removal taken while reading it would not borrow-check — and
    // the inherited names have to be spelled out on the command too, because
    // `env_remove` is the only way to say "not this one" about a variable the
    // child would otherwise be handed by the parent.
    let staged: Vec<OsString> = cmd
        .get_envs()
        .map(|(key, _)| key.to_os_string())
        .filter(|key| is_git_variable(key))
        .collect();
    let inherited: Vec<OsString> =
        std::env::vars_os().map(|(key, _)| key).filter(|key| is_git_variable(key)).collect();
    for key in staged.into_iter().chain(inherited) {
        cmd.env_remove(key);
    }
}

/// The prefix, not a list of names.
///
/// `GIT_CONFIG_PARAMETERS` is covered because it begins the same way as the rest
/// — which is the point of a prefix: it carries git's `-c` settings down the
/// same inheritance, so a list that forgot it would let an outer `-c
/// core.hooksPath=…` reach every fixture. §REQ-test-isolation.3
fn is_git_variable(key: &OsStr) -> bool {
    key.as_encoded_bytes().starts_with(b"GIT_")
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

/// `git init` for a fixture that would rather skip than fail when the machine
/// has no `git` at all, with the one skip §REQ-test-isolation.3 allows.
///
/// `false` means the program failed to start, which is the only thing that is a
/// skip. A `git` that started and *refused* panics here instead, naming the
/// directory and what git said: the test has not exercised what it claims to,
/// and a refusal is exactly what an inherited `GIT_DIR` produces — so a test
/// that skips on one reports `ok` under the gate and nowhere else.
pub fn git_init_or_absent(dir: &Path) -> bool {
    let output = match git_command().args(["init", "-q"]).current_dir(dir).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => panic!("`git init` at {} could not be started: {error}", dir.display()),
    };
    assert!(
        output.status.success(),
        "`git init` at {} was refused: {}. A `git` that started and refused fails the test \
         that owns it rather than skipping, because a refusal is what an inherited repository \
         variable produces (§REQ-test-isolation.3).",
        dir.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    true
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
