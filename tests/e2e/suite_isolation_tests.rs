//! The suite's own hygiene: where a spawned `rhei` writes is what the test
//! chose, not what the test happened to stand in. §REQ-test-isolation
//!
//! Both of a spawn's state locations are pinned because the run registry they
//! name is machine-wide — `$XDG_STATE_HOME/rhei/runs/<id>.json`, per
//! §FS-rhei-run-headless.2 — so what these tests watch is that the pinning put
//! the state under the home the test chose and nowhere near the directory the
//! binary stood in.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::templates_tests::run_raw;
use super::*;

/// The names directly under `dir`, so two readings taken around a spawn can be
/// compared for whatever appeared between them.
fn entry_names(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .expect("directory should be readable")
        .map(|entry| entry.expect("directory entry").file_name().to_string_lossy().into_owned())
        .collect()
}

/// Which of `names` `git status` would report at `dir`. An ignored entry is not
/// what §REQ-test-isolation.1 forbids: a test that must write inside the
/// repository writes beneath the ignored `scratchpad/`, and two of them do.
///
/// Each name is offered both bare and with a trailing slash, because a pattern
/// written `scratchpad/` matches a directory only. Bare, it matches just while
/// the directory is on disk — and these names are read from a tree other tests
/// are still creating and removing their own directories in, so a name asked
/// about a moment too late would come back unignored. The slashed form matches
/// whether the directory is there or not, and neither form matches a name no
/// pattern covers, so offering both cannot excuse an entry that the repository
/// does not ignore.
///
/// Where git cannot answer, every name counts. An absent git makes this
/// assertion stricter rather than silently weaker, and `check-ignore` exits 1
/// when nothing matched, so the exit code is not consulted.
fn reported_by_git(dir: &Path, names: &BTreeSet<String>) -> Vec<String> {
    let all = || names.iter().cloned().collect::<Vec<_>>();
    if names.is_empty() {
        return Vec::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["check-ignore", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return all();
    };
    let mut stdin = child.stdin.take().expect("git stdin should be piped");
    let listing =
        names.iter().map(|name| format!("{name}\n{name}/")).collect::<Vec<_>>().join("\n");
    let written = stdin.write_all(listing.as_bytes()).and_then(|()| stdin.flush());
    drop(stdin);
    let Ok(output) = child.wait_with_output() else {
        return all();
    };
    if written.is_err() {
        return all();
    }
    let ignored = String::from_utf8_lossy(&output.stdout).into_owned();
    let ignored: BTreeSet<&str> =
        ignored.lines().map(|line| line.trim().trim_end_matches('/')).collect();
    names.iter().filter(|name| !ignored.contains(name.as_str())).cloned().collect()
}

/// §REQ-test-isolation.2: the working directory a test hands the binary comes
/// back exactly as it went in, because the state home is a separate decision.
///
/// This is the pin. It fails whatever order the suite runs in and whatever is
/// already on disk, because the directory it watches is its own and empty.
#[test]
fn spawning_rhei_leaves_its_working_directory_untouched() {
    let cwd = unique_temp_dir("suite-isolation-working-dir");
    let before = entry_names(&cwd);

    let result = run_raw(&["--version"], &cwd);
    assert_success(&result);

    let added: Vec<String> = entry_names(&cwd).difference(&before).cloned().collect();
    assert!(
        added.is_empty(),
        "spawning `rhei` put {added:?} into the working directory it was handed, {}. \
         Where a spawned process writes is chosen, never derived from where it stands \
         (§REQ-test-isolation.2): a helper that joins the state home onto its cwd \
         argument writes into whatever directory the caller only wanted to stand in.",
        cwd.display()
    );
}

/// §REQ-test-isolation.1: the checkout is not a test's directory. The command it
/// spawns is an instantiation (§FS-rhei-templates), chosen because that is the
/// one the defect was found under.
///
/// This is the shape that left an untracked `.home/` at the checkout root after
/// every full gate run — a test that wanted the repository as its working
/// directory and said nothing about state. It is the acceptance test rather than
/// the pin: it compares against what was at the root when it started, so in a
/// suite that still has the defect another test may create the entry first and
/// leave this one nothing to report. Run alone it fails outright, and
/// [`spawning_rhei_leaves_its_working_directory_untouched`] is what holds the
/// regression out regardless of order.
#[test]
fn spawning_rhei_from_the_checkout_adds_nothing_git_would_report() {
    let root = repo_root();
    let output = unique_temp_dir("suite-isolation-checkout");
    let before = entry_names(&root);

    let result = run_raw(
        &[
            "instantiate",
            "hourly-human-intervention",
            "--output",
            output.join("hourly").to_str().expect("output path should be unicode"),
        ],
        &root,
    );
    assert_success(&result);

    let appeared: BTreeSet<String> = entry_names(&root).difference(&before).cloned().collect();
    let reported = reported_by_git(&root, &appeared);
    assert!(
        reported.is_empty(),
        "a test spawned from the checkout put {reported:?} at its root, {}, where \
         `git status` reports it as a change nobody made and this repository's own \
         pre-commit and pre-push hooks read it as one (§REQ-test-isolation.1).",
        root.display()
    );
}
