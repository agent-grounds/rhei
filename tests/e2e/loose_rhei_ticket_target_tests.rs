//! A qualified ticket id picks its rhei among loose plans: beside `italy.rhei.md`
//! and `rome.rhei.md` with no `index.panta.md`, `rhei release italy.1` acts on
//! italy as it would inside a project (#676). §FS-rhei-panta.6.7
//! §FS-rhei-complete.2.1 §FS-rhei-usage.2.2.1 §FS-rhei-new.1.3 §FS-rhei-next.2.1
//! §FS-rhei-note.1

use super::loose_rhei_ticket_target_support::{assert_picked, assert_reached_italy, Loose};
use super::*;

/// `rhei show italy.1` prints italy's ticket. §FS-rhei-panta.6.7
#[test]
fn show_picks_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-show");
    let result = loose.rhei(&["show", "italy.1"]);
    assert_picked(&result, "`rhei show italy.1`");
    assert!(result.stdout.contains("Task italy.1: Book the trains"), "{}", result.stdout);
}

/// `rhei release italy.1` hands italy's ticket back. §FS-rhei-panta.6.7
#[test]
fn release_picks_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-release");
    let result = loose.rhei(&["release", "italy.1"]);
    assert_picked(&result, "`rhei release italy.1`");
    assert!(!loose.read("italy.rhei.md").contains("**Assignee:**"), "italy.1 is released");
    loose.assert_rome_untouched();
}

/// `rhei complete italy.1` finishes italy's ticket. §FS-rhei-complete.2.1
#[test]
fn complete_picks_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-complete");
    let result = loose.rhei(&["complete", "italy.1", "--result", "booked"]);
    assert_picked(&result, "`rhei complete italy.1`");
    assert!(loose.read("italy.rhei.md").contains("**State:** completed"));
    loose.assert_rome_untouched();
}

/// `rhei transition italy.2` moves italy's ticket. §FS-rhei-usage.2.2.1
#[test]
fn transition_picks_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-transition");
    let result = loose.rhei(&["transition", "italy.2", "--from", "draft", "--to", "pending"]);
    assert_picked(&result, "`rhei transition italy.2`");
    assert!(!loose.read("italy.rhei.md").contains("**State:** draft"), "italy.2 moved");
    loose.assert_rome_untouched();
}

/// `rhei remove italy.2 --dry-run` reads italy's ticket and writes nothing.
/// §FS-rhei-usage.2.2.1
#[test]
fn remove_dry_run_picks_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-remove");
    let result = loose.rhei(&["remove", "italy.2", "--dry-run"]);
    assert_picked(&result, "`rhei remove italy.2 --dry-run`");
    assert_eq!(loose.read("italy.rhei.md"), super::loose_rhei_ticket_target_support::ITALY);
    loose.assert_rome_untouched();
}

/// `rhei retry italy.1` reaches italy's ticket, which has no visit to refill:
/// its refusal is the ticket's, not the walk's. §FS-rhei-usage.2.2.1
#[test]
fn retry_reaches_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-retry");
    let result = loose.rhei(&["retry", "italy.1"]);
    assert!(!result.status.success(), "italy.1 never ran, so there is nothing to refill");
    assert_reached_italy(&result, "Task italy.1 has no spawn record");
}

/// A ticket given through `--task` with no plan picks the same way.
/// §FS-rhei-complete.2.1
#[test]
fn task_flag_without_a_plan_picks_like_the_positional() {
    let loose = Loose::new("loose-task-flag");
    let result = loose.rhei(&["show", "--task", "italy.1"]);
    assert_picked(&result, "`rhei show --task italy.1`");
    assert!(result.stdout.contains("Task italy.1: Book the trains"), "{}", result.stdout);
}

/// `--under italy` and `--under italy.1` write into italy.rhei.md.
/// §FS-rhei-new.1.3 §FS-rhei-new.1.1
#[test]
fn new_under_picks_the_loose_rhei_it_names() {
    let loose = Loose::new("loose-new-under");
    let ticket = loose.rhei(&["new", "Book the ferry", "--under", "italy"]);
    assert_picked(&ticket, "`rhei new --under italy`");
    let subtask = loose.rhei(&["new", "Seat reservation", "--under", "italy.1"]);
    assert_picked(&subtask, "`rhei new --under italy.1`");
    let italy = loose.read("italy.rhei.md");
    assert!(italy.contains("### Task 3: Book the ferry"), "{italy}");
    assert!(italy.contains("#### Task 1.1: Seat reservation"), "{italy}");
    loose.assert_rome_untouched();
}

/// `rhei next --task italy.1` claims italy's ticket: the "claim it" step of the
/// same gesture. §FS-rhei-next.2.1
#[test]
fn next_task_picks_the_loose_rhei_its_id_names() {
    let loose = Loose::new("loose-next-task");
    let result = loose.rhei(&["next", "--task", "italy.2"]);
    assert_picked(&result, "`rhei next --task italy.2`");
    assert!(result.stdout.contains("Task italy.2 claimed"), "{}", result.stdout);
    loose.assert_rome_untouched();
}

/// `rhei note` resolves by its writing task, given as `--task` or as
/// `RHEI_TASK_ID` the way a program state of a single-file plan has it.
/// §FS-rhei-note.1
#[test]
fn note_picks_the_loose_rhei_of_its_writing_task() {
    let loose = Loose::new("loose-note");
    let flag = loose.rhei(&["note", "--task", "italy.1", "The 9:10 is sold out"]);
    assert_picked(&flag, "`rhei note --task italy.1`");
    let env =
        loose.rhei_in(&loose.dir, &["note", "Take the 11:10"], &[("RHEI_TASK_ID", "italy.1")]);
    assert_picked(&env, "`RHEI_TASK_ID=italy.1 rhei note`");
    loose.assert_rome_untouched();
}

/// `--rhei` still only narrows after the plan resolves: it agrees with the
/// picked rhei, and names an unknown one otherwise. §FS-rhei-panta.6.7
#[test]
fn rhei_flag_narrows_after_the_pick() {
    let loose = Loose::new("loose-rhei-flag");
    let wrong = loose.rhei(&["release", "italy.1", "--rhei", "rome"]);
    assert!(!wrong.status.success(), "rome is not the picked rhei");
    assert_reached_italy(&wrong, "unknown rhei 'rome'");
    let right = loose.rhei(&["release", "italy.1", "--rhei", "italy"]);
    assert_picked(&right, "`rhei release italy.1 --rhei italy`");
}

/// An entry whose id cannot be derived — the reserved `basin.rhei.md`, an
/// invalid stem — is a non-match, not an error that aborts the pick.
/// §FS-rhei-panta.6.7
#[test]
fn an_entry_with_no_usable_id_does_not_abort_the_pick() {
    let loose = Loose::new("loose-underivable")
        .with("basin.rhei.md", "# Rhei: Basin\n\n## Tasks\n")
        .with("not.valid.rhei.md", "# Rhei: Not valid\n\n## Tasks\n");
    let result = loose.rhei(&["show", "italy.1"]);
    assert_picked(&result, "`rhei show italy.1` beside basin.rhei.md and not.valid.rhei.md");
}

/// Loose plans in a subdirectory of a project: the invocation directory wins,
/// as rule 4 already makes it win for a lone plan. §FS-rhei-panta.6.7
#[test]
fn loose_plans_below_a_project_win_over_its_rheis() {
    let loose = Loose::new("loose-below-project");
    let project = loose.dir.parent().expect("the loose directory has a parent").to_path_buf();
    let sub = project.join("sub");
    std::fs::rename(&loose.dir, &sub).expect("move the loose plans below the project");
    write_fixture_file(&project, "index.panta.md", "# Panta: Trip\n");
    let ancestor =
        "# Rhei: Italy\n\n## Tasks\n\n### Task 1: The ancestor's italy\n**State:** draft\n";
    write_fixture_file(&project, "italy.rhei.md", ancestor);

    let result = loose.rhei_in(&sub, &["show", "italy.1"], &[]);
    assert_picked(&result, "`rhei show italy.1` in sub/");
    assert!(result.stdout.contains("Book the trains"), "sub/italy.rhei.md, not the ancestor's");
}
