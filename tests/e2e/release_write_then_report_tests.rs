//! `rhei release` writes, then reports: a `Released` line is printed only for a
//! claim that is gone from the file (agent-grounds/rhei#627). The write is made
//! to fail by a read-only directory, where the plan's lock file and its atomic
//! rename would have to be created; a read-only file in a writable directory
//! does not stop it.

// §FS-rhei-release.3

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::*;

const MACHINE: &str = "name: scratch\nversion: 1\nstates:\n  open:\n    initial: true\n    \
description: Open\n  done:\n    final: true\n    description: Done\ntransitions:\n  - {from: open, to: done}\n";

const TWO_CLAIMS: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: First\n\n**State:** open\n\
**Assignee:** me\n\nDo.\n\n### Task 2: Second\n\n**State:** open\n**Assignee:** you\n\nDo.\n";

/// A directory's mode, put back when this leaves scope so the tree can be removed.
struct RestoredMode {
    path: PathBuf,
    mode: u32,
}

impl Drop for RestoredMode {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
    }
}

/// `dir` with its write permission off, or `None` where that does not bind, as
/// under root: such a run has nothing to show, so it skips.
#[must_use]
fn read_only(dir: &Path) -> Option<RestoredMode> {
    let mode = fs::metadata(dir).expect("the directory exists").permissions().mode();
    fs::set_permissions(dir, fs::Permissions::from_mode(mode & !0o222)).expect("chmod a-w");
    let held = RestoredMode { path: dir.to_path_buf(), mode };
    let probe = dir.join(".chmod-probe");
    if fs::File::create(&probe).is_ok() {
        fs::remove_file(&probe).expect("remove the probe");
        eprintln!("skipped: chmod a-w does not bind on {}", dir.display());
        return None;
    }
    Some(held)
}

/// The tickets stdout says were released, as `(id, assignee)`.
fn released_lines(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("Released Task "))
        .filter_map(|rest| {
            let (id, who) = rest.split_once(" (was assigned to ")?;
            Some((id.to_string(), who.strip_suffix(')')?.to_string()))
        })
        .collect()
}

/// Whether `file` still carries `**Assignee:** <who>`.
fn still_claimed(file: &Path, who: &str) -> bool {
    let text = fs::read_to_string(file).expect("read the task file");
    text.lines().any(|line| line.trim() == format!("**Assignee:** {who}"))
}

/// A single-file plan with two claimed tickets beside its machine, in a fresh
/// directory whose isolated home exists before the directory is made read-only.
fn plain_plan(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", TWO_CLAIMS);
    let machine = write_fixture_file(&dir, "states.yaml", MACHINE);
    fs::create_dir_all(isolated_home_for(&plan)).expect("create the isolated home");
    assert_success(&run_cli("validate", &plan, &machine, &[]));
    (dir, plan, machine)
}

/// A refused write for `--task` exits non-zero and prints no `Released` line
/// for the claim it could not drop. §FS-rhei-release.3
#[test]
#[ignore = "red until #627: release prints Released before the write that fails; implement removes this"]
fn release_task_whose_write_fails_prints_no_released_line() {
    let (dir, plan, machine) = plain_plan("release-627-task");
    let Some(_held) = read_only(&dir) else { return };

    let run = run_cli("release", &plan, &machine, &["--task", "1"]);

    assert!(!run.status.success(), "a refused write must exit non-zero:\n{}", run.stdout);
    assert!(still_claimed(&plan, "me"), "the refused write left the claim in place");
    assert_eq!(
        released_lines(&run.stdout),
        Vec::<(String, String)>::new(),
        "stdout reported a release whose claim is still in the file:\n{}",
        run.stdout
    );
}

/// `--all` over two claims a read-only directory protects prints a `Released`
/// line for neither. §FS-rhei-release.3
#[test]
#[ignore = "red until #627: release prints Released before the write that fails; implement removes this"]
fn release_all_whose_writes_fail_prints_no_released_line() {
    let (dir, plan, machine) = plain_plan("release-627-all");
    let Some(_held) = read_only(&dir) else { return };

    let run = run_cli("release", &plan, &machine, &["--all"]);

    assert!(!run.status.success(), "a refused write must exit non-zero:\n{}", run.stdout);
    assert!(still_claimed(&plan, "me") && still_claimed(&plan, "you"), "nothing was written");
    assert_eq!(
        released_lines(&run.stdout),
        Vec::<(String, String)>::new(),
        "stdout reported releases whose claims are still in the file:\n{}",
        run.stdout
    );
}

/// A project of two rheis, `a` and `b`, each holding one claimed ticket in
/// `<rhei>/tasks/01.md`, assigned to `wa` and `wb`.
fn two_rhei_project(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("project");
    for rhei in ["a", "b"] {
        fs::create_dir_all(project.join(rhei).join("tasks")).expect("create the rhei");
    }
    write_fixture_file(&project, "index.panta.md", "# Panta: Two\n");
    write_fixture_file(&project, "states.yaml", MACHINE);
    for rhei in ["a", "b"] {
        let root = project.join(rhei);
        write_fixture_file(&root, "index.rhei.md", &format!("# Rhei: {rhei}\n"));
        let task = format!("### Task 1: One\n\n**State:** open\n**Assignee:** w{rhei}\n\nDo.\n");
        write_fixture_file(&root.join("tasks"), "01.md", &task);
    }
    (dir, project)
}

/// `--all` stops at the first write that fails, and its stdout names exactly
/// the tickets released before it: `a.1` is released and says so, `b.1` keeps
/// its claim and is not reported. §FS-rhei-release.3
#[test]
#[ignore = "red until #627: release prints Released before the write that fails; implement removes this"]
fn release_all_reports_exactly_the_tickets_released_before_a_failed_write() {
    let (dir, project) = two_rhei_project("release-627-partial");
    let home = dir.join(".home");
    fs::create_dir_all(&home).expect("create the isolated home");
    let rhei = |args: &[&str]| {
        CliRun::from(&rhei_command(home.clone()).args(args).output().expect("rhei runs"))
    };
    assert_success(&rhei(&["validate", project.to_str().expect("utf-8 path")]));
    let Some(_held) = read_only(&project.join("b").join("tasks")) else { return };

    let run = rhei(&["release", project.to_str().expect("utf-8 path"), "--all"]);

    assert!(!run.status.success(), "a refused write must exit non-zero:\n{}", run.stdout);
    assert!(!still_claimed(&project.join("a/tasks/01.md"), "wa"), "a.1's write was allowed");
    assert!(still_claimed(&project.join("b/tasks/01.md"), "wb"), "b.1's write was refused");
    assert_eq!(
        released_lines(&run.stdout),
        vec![("a.1".to_string(), "wa".to_string())],
        "stdout must name a.1, released, and not b.1, still claimed:\n{}",
        run.stdout
    );
}
