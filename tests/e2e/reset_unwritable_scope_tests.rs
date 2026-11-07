//! The reach of a filesystem refusal beyond a whole workspace: a Single-File plan and
//! a reset narrowed with `--rhei`, which removes only what its tickets own, so it may
//! refuse only over what it would touch (agent-grounds/rhei#609).

// §FS-rhei-reset.2.1 §FS-rhei-reset.2.3 §FS-rhei-reset.4

use std::fs;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;
use super::reset_unwritable_support::*;

const ONE_TASK: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: One\n**State:** open\n";

/// Sibling single-file rheis `a` and `b`, both finished, sharing one `runtime/`.
fn finished_siblings(name: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    world.write("states.yaml", MACHINE);
    world.write("index.panta.md", "# Panta: Reset fixture\n");
    world.write("a.rhei.md", ONE_TASK);
    world.write("b.rhei.md", ONE_TASK);
    finish(&world, "a.1", &[]);
    finish(&world, "b.1", &[]);
    world
}

/// Its directory read-only: nothing half-resets today, but the dry run promises a
/// reset that then stops at its first write.
#[test]
#[ignore = "red until #609: dry run exits 0 over a plan --yes cannot write; implement removes this"]
fn a_read_only_single_file_plans_directory_refuses_the_reset_and_its_dry_run_intact() {
    let world = plan_world("reset-unwritable-single", MACHINE, ONE_TASK);
    finish(&world, "1", &[]);
    let Some(_plans) = read_only(&world, ".") else { return };

    assert_refused_intact(&world, &[], &["plans"]);
}

#[test]
#[ignore = "red until #609: --yes rewrites the plan and stops part way through the tree; implement removes this"]
fn a_single_file_plans_runtime_that_cannot_be_emptied_refuses_the_reset_intact() {
    let world = plan_world("reset-unwritable-single-runtime", MACHINE, ONE_TASK);
    finish(&world, "1", &[]);
    let Some(_results) = read_only(&world, "runtime/results") else { return };

    assert_refused_intact(&world, &[], &["plans/runtime/results"]);
}

/// The narrowed ticket's own result cannot be removed.
#[test]
#[ignore = "red until #609: --yes rewrites the plan, then cannot remove the result; implement removes this"]
fn a_narrowed_reset_refuses_intact_over_the_directory_holding_its_result() {
    let world = finished_siblings("reset-unwritable-narrowed-result");
    let Some(_results) = read_only(&world, "runtime/results") else { return };

    assert_refused_intact(&world, &["--rhei", "a"], &["project/runtime/results"]);
}

/// The ledger keeps `b`'s lines, so pruning `a`'s writes a new file beside it.
#[test]
#[ignore = "red until #609: --yes rewrites the plan and removes the result, then stops; implement removes this"]
fn a_narrowed_reset_refuses_intact_over_a_ledger_it_cannot_prune() {
    let world = finished_siblings("reset-unwritable-narrowed-ledger");
    let Some(_runtime) = read_only(&world, "runtime") else { return };

    assert_refused_intact(&world, &["--rhei", "a"], &["project/runtime"]);
}

/// A narrowed reset keeps the evidence store and records its removals there; finishing
/// without the record would leave `rhei verify` no way to tell them from tampering.
#[test]
#[ignore = "red until #609: --yes exits 0 with a warning and no reset event; implement removes this"]
fn a_narrowed_reset_refuses_intact_where_it_cannot_record_its_reset_event() {
    let world = finished_siblings("reset-unwritable-narrowed-event");
    fs::create_dir_all(world.root.join("runtime/evidence/events")).expect("the store's events");
    let Some(_events) = read_only(&world, "runtime/evidence/events") else { return };

    assert_refused_intact(&world, &["--rhei", "a"], &["project/runtime/evidence/events"]);
}

/// An in-scope member's index holds metadata only its own write clears.
#[test]
#[ignore = "red until #609: --yes rewrites the task file and stops at the index; implement removes this"]
fn a_narrowed_reset_refuses_intact_over_a_members_index_it_cannot_write() {
    let world = project_of_workspaces("reset-unwritable-narrowed-index", &["alpha", "beta"]);
    finish(&world, "alpha.1", &[]);
    let index = "# Rhei: alpha\n\n---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        \
         open: 2\n---\n";
    world.write("alpha/index.rhei.md", index);
    let Some(_alpha) = read_only(&world, "alpha") else { return };

    assert_refused_intact(&world, &["--rhei", "alpha"], &["project/alpha"]);
}

/// The other side, and the point of narrowing: another rhei's read-only directories
/// are no business of this reset. Passes today, and must keep passing.
#[test]
fn an_unrelated_read_only_directory_does_not_refuse_a_narrowed_reset() {
    let world = project_of_workspaces("reset-unwritable-narrowed-other", &["alpha", "beta"]);
    finish(&world, "alpha.1", &[]);
    finish(&world, "beta.1", &[]);
    let beta_task = fs::read(world.root.join("beta/tasks/01-a.md")).expect("read beta");
    let Some(_beta) = read_only(&world, "beta") else { return };
    let Some(_beta_tasks) = read_only(&world, "beta/tasks") else { return };

    assert_finishes(&world, &["--rhei", "alpha"]);

    assert_eq!(listed_state(&world, "alpha.1"), "open", "the named rhei goes back");
    assert_eq!(listed_state(&world, "beta.1"), "done", "the other stays");
    assert_eq!(fs::read(world.root.join("beta/tasks/01-a.md")).expect("read beta"), beta_task);
}

/// A shared directory holding only another ticket's output: a check that asked every
/// directory a narrowed reset might touch would refuse here. Passes today, and must
/// keep passing.
#[test]
fn a_read_only_directory_holding_only_another_tickets_output_does_not_refuse_a_narrowed_reset() {
    let world = finished_siblings("reset-unwritable-narrowed-logs");
    world.write("runtime/logs/task-b.1-open.log", "the sibling's log\n");
    let Some(_logs) = read_only(&world, "runtime/logs") else { return };

    assert_finishes(&world, &["--rhei", "a"]);

    assert_eq!(listed_state(&world, "a.1"), "open", "the named rhei goes back");
    assert!(world.root.join("runtime/logs/task-b.1-open.log").exists(), "the sibling keeps it");
}
