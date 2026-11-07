//! A reset of a whole plan, over a directory it cannot write in or remove from. It
//! refuses before any preview with nothing changed, or, where it has nothing to do in
//! that directory, it finishes (agent-grounds/rhei#609).

// §FS-rhei-reset.2.3 §FS-rhei-reset.3 §FS-rhei-reset.4

use std::fs;

use super::reset_invalid_plan_support::*;
use super::reset_unwritable_support::*;

const A: (&str, &str) = ("01-a.md", "### Task 1: A");
const NESTED_A: (&str, &str) = ("a/01-a.md", "### Task 1: A");
const NESTED_B: (&str, &str) = ("b/02-b.md", "### Task 2: B");

/// A task that never moved and still changes: a reset takes its claim away.
const CLAIMED_TASK: &str = "### Task 1: A\n\n**State:** open\n**Assignee:** me\n\nDo the thing.\n";

/// The index's runtime metadata, which only the reset's write of it clears.
const INDEX_WITH_VISITS: &str =
    "# Rhei: Reset fixture\n\n---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        \
     open: 2\n---\n";

/// The issue's fixture: the root is read-only, so `runtime/` cannot be removed.
#[test]
fn a_read_only_workspace_root_refuses_the_reset_and_its_dry_run_intact() {
    let world = workspace("reset-unwritable-root", &[A]);
    finish(&world, "1", &[]);
    let Some(_root) = read_only(&world, ".") else { return };

    assert_refused_intact(&world, &[], &["ws"]);
}

/// A second directory of task files, after the first took its write.
#[test]
fn a_second_read_only_task_directory_refuses_the_reset_intact() {
    let world = workspace("reset-unwritable-second", &[NESTED_A, NESTED_B]);
    finish(&world, "1", &[]);
    finish(&world, "2", &[]);
    let Some(_b) = read_only(&world, "tasks/b") else { return };

    assert_refused_intact(&world, &[], &["ws/tasks/b"]);
}

/// No `runtime/` here: the index is the one write the root would have to take.
#[test]
fn an_index_with_runtime_metadata_under_a_read_only_root_refuses_the_reset_intact() {
    let world = workspace("reset-unwritable-index", &[A]);
    lay_sidecars(&world);
    world.write("index.rhei.md", INDEX_WITH_VISITS);
    world.write("tasks/01-a.md", CLAIMED_TASK);
    let Some(_root) = read_only(&world, ".") else { return };

    assert_refused_intact(&world, &[], &["ws"]);
}

/// The root is writable, and the tree still cannot be removed from inside.
#[test]
fn a_directory_inside_runtime_that_cannot_be_emptied_refuses_the_reset_intact() {
    let world = workspace("reset-unwritable-runtime", &[A]);
    finish(&world, "1", &[]);
    let Some(_results) = read_only(&world, "runtime/results") else { return };

    assert_refused_intact(&world, &[], &["ws/runtime/results"]);
}

/// Creating a temp file asks only for write permission, and removing a tree has to
/// list it too. A directory that takes a file and cannot be listed therefore fails the
/// removal after every plan file was written, and the check has to see it.
#[test]
fn a_directory_inside_runtime_that_cannot_be_listed_refuses_the_reset_intact() {
    use std::os::unix::fs::PermissionsExt;

    let world = workspace("reset-unwritable-unlistable", &[A]);
    finish(&world, "1", &[]);
    world.write("runtime/logs/task-ws.1-open.log", "the log\n");
    let task = fs::read(world.root.join("tasks/01-a.md")).expect("read the task");
    let ledger = fs::read(world.root.join("runtime/state-transitions.log")).expect("the ledger");
    let logs = world.root.join("runtime/logs");
    fs::set_permissions(&logs, fs::Permissions::from_mode(0o300)).expect("chmod a-r");
    let _logs = RestoredMode { path: logs.clone(), mode: 0o755 };
    if fs::read_dir(&logs).is_ok() {
        eprintln!("skipped: chmod a-r does not bind on {}", logs.display());
        return;
    }

    let dry = world.rhei("reset", &["--dry-run"]);
    let yes = world.rhei("reset", &["--yes"]);

    let task_kept = fs::read(world.root.join("tasks/01-a.md")).expect("read the task") == task;
    let ledger_kept =
        fs::read(world.root.join("runtime/state-transitions.log")).ok().as_ref() == Some(&ledger);
    assert!(
        !dry.status.success() && !yes.status.success() && task_kept && ledger_kept,
        "both must refuse and change nothing.\n--dry-run exited {:?}\n--yes exited {:?}, \
         task file kept: {task_kept}, ledger kept: {ledger_kept}\n--yes said:\n{}{}",
        dry.status.code(),
        yes.status.code(),
        yes.stdout,
        yes.stderr
    );
    for run in [&dry, &yes] {
        assert!(run.stderr.contains(UNWRITABLE_REFUSAL), "it says it refused:\n{}", run.stderr);
        assert!(run.stderr.contains("/ws/runtime/logs: "), "it names it:\n{}", run.stderr);
    }
}

/// Every directory that fails is named, so one chmod is not followed by another refusal.
#[test]
fn a_refusal_names_every_directory_that_fails() {
    let world = workspace("reset-unwritable-every", &[NESTED_A, NESTED_B]);
    finish(&world, "1", &[]);
    finish(&world, "2", &[]);
    let Some(_b) = read_only(&world, "tasks/b") else { return };
    let Some(_results) = read_only(&world, "runtime/results") else { return };

    assert_refused_intact(&world, &[], &["ws/tasks/b", "ws/runtime/results"]);
}

/// A member's root read-only under a project reset, after the other member was written.
#[test]
fn a_read_only_member_root_refuses_a_project_reset_intact() {
    let world = project_of_workspaces("reset-unwritable-member", &["alpha", "beta"]);
    finish(&world, "alpha.1", &[]);
    finish(&world, "beta.1", &[]);
    let Some(_beta) = read_only(&world, "beta") else { return };

    assert_refused_intact(&world, &[], &["project/beta"]);
}

/// The lock a reset would create is a write too. The real reset stops there having
/// changed nothing, so the dry run has to say the same.
#[test]
fn a_lock_sidecar_that_cannot_be_created_refuses_the_dry_run_too() {
    let world = workspace("reset-unwritable-sidecar", &[NESTED_A, NESTED_B]);
    finish(&world, "1", &[]);
    assert!(!world.root.join("tasks/b/02-b.md.lock").exists(), "fixture: B was never locked");
    let Some(_b) = read_only(&world, "tasks/b") else { return };

    assert_refused_intact(&world, &[], &["ws/tasks/b"]);
}

/// The other side: a read-only directory of files the reset leaves as they are.
#[test]
fn a_read_only_directory_of_unchanged_files_does_not_refuse_the_reset() {
    let world = workspace("reset-unwritable-unchanged", &[NESTED_A, NESTED_B]);
    lay_sidecars(&world);
    finish(&world, "1", &[]);
    let untouched = fs::read(world.root.join("tasks/b/02-b.md")).expect("read B");
    let Some(_b) = read_only(&world, "tasks/b") else { return };

    assert_finishes(&world, &[]);

    assert_eq!(listed_state(&world, "ws.1"), "open", "the moved task goes back");
    assert!(!world.root.join("runtime").exists(), "the runtime tree is gone");
    assert_eq!(fs::read(world.root.join("tasks/b/02-b.md")).expect("read B"), untouched);
}

/// The index with nothing to clear and no `runtime/`: a read-only root has nothing to
/// take, while the task file still loses its claim.
#[test]
fn a_read_only_root_with_an_unchanged_index_and_no_runtime_does_not_refuse_the_reset() {
    let world = workspace("reset-unwritable-index-unchanged", &[A]);
    lay_sidecars(&world);
    world.write("tasks/01-a.md", CLAIMED_TASK);
    let Some(_root) = read_only(&world, ".") else { return };

    assert_finishes(&world, &[]);

    let task = fs::read_to_string(world.root.join("tasks/01-a.md")).expect("read the task");
    assert!(!task.contains("**Assignee:**"), "the claim is gone:\n{task}");
}

/// A plan a reset has nothing to do to, in directories that would refuse everything.
#[test]
fn a_plan_with_nothing_to_reset_finishes_under_read_only_directories() {
    let world = workspace("reset-unwritable-nothing", &[A]);
    lay_sidecars(&world);
    let before = snapshot(&world);
    let Some(_tasks) = read_only(&world, "tasks") else { return };
    let Some(_root) = read_only(&world, ".") else { return };

    assert_finishes(&world, &[]);

    assert_eq!(snapshot(&world), before, "nothing needed writing");
}

/// An empty read-only directory inside the tree holds nothing to remove from itself,
/// and removing the tree takes it anyway, so it refuses nothing. Passes today, and must
/// keep passing.
#[test]
fn an_empty_read_only_directory_inside_runtime_does_not_refuse_the_reset() {
    let world = workspace("reset-unwritable-empty", &[A]);
    finish(&world, "1", &[]);
    fs::create_dir_all(world.root.join("runtime/empty")).expect("an empty directory");
    let Some(_empty) = read_only(&world, "runtime/empty") else { return };

    assert_finishes(&world, &[]);

    assert_eq!(listed_state(&world, "ws.1"), "open", "the task goes back");
    assert!(!world.root.join("runtime").exists(), "the tree is gone");
}

/// The rename a write ends in asks the directory, not the file, so a read-only plan
/// file in a writable directory is no refusal. Passes today, and must keep passing.
#[test]
fn a_read_only_file_in_a_writable_directory_does_not_refuse_the_reset() {
    use std::os::unix::fs::PermissionsExt;

    let world = workspace("reset-unwritable-file", &[A]);
    finish(&world, "1", &[]);
    let task = world.root.join("tasks/01-a.md");
    fs::set_permissions(&task, fs::Permissions::from_mode(0o444)).expect("chmod the task file");

    assert_finishes(&world, &[]);

    assert_eq!(listed_state(&world, "ws.1"), "open", "the task goes back");
}
