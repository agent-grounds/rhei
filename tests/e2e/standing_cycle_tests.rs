//! A dependency cycle the plan already had is not the fault of the command that
//! found it: `rhei new`, `rhei remove` and `rhei instantiate --into` answer for the
//! errors they introduced, and that only works while a cycle's error says the same
//! thing about the same fault (agent-grounds/rhei#582).
//! §FS-rhei-plan-language.3.3 §FS-rhei-new.5.2 §FS-rhei-remove.4.1 §FS-rhei-library.2
//!
//! Where a defect is the list's hash order, the plan's cycle has nine tasks, so a
//! fresh process lands on the other validation's order about once in 360 thousand:
//! these fail every run today rather than on a coin. Where it is a task named that
//! is on no cycle, two tasks are enough, and that failure takes no luck at all.

use std::path::Path;

use super::into_support::{read, run_into, write_review_template, HOST_MACHINE};
use super::new_tests::{flattened_output, new_run};
use super::*;

const PLAN: &str = "release.rhei.md";

/// The sentence a cycle of these tasks gets. §FS-rhei-plan-language.3.3
fn cycle_text(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("{name:?}")).collect();
    format!("Circular dependency detected among tasks: [{}]", quoted.join(", "))
}

/// A top-level ticket, as a plan holds it.
fn ticket(id: u32, prior: Option<&str>) -> String {
    let prior = prior.map(|prior| format!("**Prior:** {prior}\n")).unwrap_or_default();
    format!("### Task {id}: Ticket {id}\n**State:** pending\n{prior}\n")
}

/// `release.1` and `release.2`, each other's prior.
fn two_task_cycle() -> String {
    ticket(1, Some("release.2")) + &ticket(2, Some("release.1"))
}

/// `release.1` to `release.9` in one ring.
fn ring_of_nine() -> String {
    (1..=9).map(|id| ticket(id, Some(&format!("release.{}", id % 9 + 1)))).collect()
}

const RING_OF_NINE: [&str; 9] = [
    "release.1",
    "release.2",
    "release.3",
    "release.4",
    "release.5",
    "release.6",
    "release.7",
    "release.8",
    "release.9",
];

/// A single-file plan under the host machine holding `tickets`, refused by
/// `rhei validate` for its cycle, or this test proves nothing.
fn plan_with_a_standing_cycle(prefix: &str, tickets: &str) -> TestDir {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "states.yaml", HOST_MACHINE);
    write_fixture_file(&dir, PLAN, &format!("# Rhei: Release\n\n## Tasks\n\n{tickets}"));
    let validate = new_run(&["validate", PLAN], &dir);
    assert!(
        !validate.status.success() && validate.stderr.contains("Circular dependency"),
        "fixture: `rhei validate` refuses the plan for its cycle:\n{}",
        validate.stderr
    );
    dir
}

/// Exited 0 and said the failure was not its own. §FS-rhei-new.5.2
#[track_caller]
fn assert_kept_with_the_inherited_warning(run: &CliRun) {
    assert!(run.status.success(), "the write is kept:\n{}{}", run.stdout, run.stderr);
    assert!(
        flattened_output(run).contains("already failing validation"),
        "and warns that the failure was there already:\n{}",
        run.stderr
    );
}

/// `rhei validate` says exactly these errors about cycles, and says them whole.
#[track_caller]
fn assert_cycle_errors(dir: &Path, expected: &[String]) {
    let validate = new_run(&["validate", PLAN], dir);
    let flattened = flattened_output(&validate);
    for text in expected {
        assert!(flattened.contains(text.as_str()), "expected {text:?} in:\n{flattened}");
    }
    assert_eq!(
        flattened.matches("Circular dependency detected").count(),
        expected.len(),
        "exactly these cycles are reported:\n{flattened}"
    );
}

/// The whole text of the report, for a ring with a task waiting on it.
#[test]
#[ignore = "red until #582: validate names a task downstream of the cycle, in hash order; implement removes this"]
fn validate_names_only_the_tasks_on_the_cycle_sorted() {
    let tickets = ring_of_nine() + &ticket(10, Some("release.1"));
    let dir = plan_with_a_standing_cycle("cycle-validate-whole-text", &tickets);
    assert_cycle_errors(&dir, &[cycle_text(&RING_OF_NINE)]);
}

/// The issue's case 2, on a ring long enough not to be a coin.
#[test]
#[ignore = "red until #582: rhei new refuses over a standing cycle reworded by hash order; implement removes this"]
fn new_of_an_unrelated_ticket_over_a_standing_cycle_is_kept() {
    let dir = plan_with_a_standing_cycle("cycle-new-unrelated", &ring_of_nine());

    let created = new_run(&["new", "Unrelated", "--project", PLAN, "--under", "release"], &dir);

    assert_kept_with_the_inherited_warning(&created);
    assert!(read(&dir.join(PLAN)).contains("Unrelated"), "the ticket is in the plan");
}

/// The new ticket waits on a cycle member and so can never run, which is the
/// cycle's doing: today it is named in the error and takes the blame for it.
#[test]
#[ignore = "red until #582: rhei new --prior on a cycle member is refused for joining the list; implement removes this"]
fn new_with_a_prior_on_a_cycle_member_is_kept() {
    let dir = plan_with_a_standing_cycle("cycle-new-prior", &two_task_cycle());

    let created = new_run(
        &["new", "Waits", "--project", PLAN, "--under", "release", "--prior", "release.1"],
        &dir,
    );

    assert_kept_with_the_inherited_warning(&created);
    assert!(read(&dir.join(PLAN)).contains("Waits"), "the ticket is in the plan");
    assert_cycle_errors(&dir, &[cycle_text(&["release.1", "release.2"])]);
}

/// The issue's case 3, on a ring long enough not to be a coin.
#[test]
#[ignore = "red until #582: rhei remove refuses over a standing cycle reworded by hash order; implement removes this"]
fn remove_of_an_unrelated_ticket_over_a_standing_cycle_goes_through() {
    let tickets = ring_of_nine() + &ticket(10, None);
    let dir = plan_with_a_standing_cycle("cycle-remove-unrelated", &tickets);

    let removed = new_run(&["remove", "release.10"], &dir);

    assert!(removed.status.success(), "the removal goes through:\n{}", removed.stderr);
    assert!(!read(&dir.join(PLAN)).contains("Ticket 10"), "the ticket is gone");
}

/// A leaf that nothing names and that only waits on the cycle leaves the cycle as
/// it was, and today the error shrinks by its name, so the removal is refused and
/// its dry run is not. §FS-rhei-remove.4.1 §FS-rhei-remove.7
#[test]
#[ignore = "red until #582: rhei remove of a leaf on a cycle is refused for leaving the list; implement removes this"]
fn remove_of_a_leaf_waiting_on_a_cycle_member_goes_through() {
    let tickets = two_task_cycle() + &ticket(3, Some("release.1"));
    let dir = plan_with_a_standing_cycle("cycle-remove-leaf", &tickets);

    let previewed = new_run(&["remove", "release.3", "--dry-run"], &dir);
    assert!(previewed.status.success(), "the dry run agrees:\n{}", previewed.stderr);

    let removed = new_run(&["remove", "release.3"], &dir);

    assert!(removed.status.success(), "the removal goes through:\n{}", removed.stderr);
    assert!(!read(&dir.join(PLAN)).contains("Ticket 3"), "the ticket is gone");
    assert_cycle_errors(&dir, &[cycle_text(&["release.1", "release.2"])]);
}

/// The issue read this path and did not run it. §FS-rhei-library.2
#[test]
#[ignore = "red until #582: --into refuses over a standing cycle reworded by hash order; implement removes this"]
fn a_placement_over_a_standing_cycle_is_placed() {
    let dir = plan_with_a_standing_cycle("cycle-into-unrelated", &ring_of_nine());
    write_review_template(&dir);

    let placed =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);

    assert!(placed.status.success(), "the placement goes through:\n{}", placed.stderr);
    assert!(read(&dir.join(PLAN)).contains("Coordinate review"), "the tickets are placed");
}

/// The guard against a fix too generous: a write that really does add a cycle is
/// still refused, wrote nothing, and names the new cycle and not the standing one.
/// §FS-rhei-new.5.2, "The report lists only those new errors".
#[test]
#[ignore = "red until #582: the refusal lists the new and the standing cycle as one; implement removes this"]
fn a_placement_that_adds_a_cycle_is_refused_naming_only_that_cycle() {
    let dir = plan_with_a_standing_cycle("cycle-into-adds", &two_task_cycle());
    let template = write_review_template(&dir);
    let coordinate = template.join("tasks/001-coordinate.md");
    let waits_on_record =
        read(&coordinate).replace("**State:** review\n", "**State:** review\n**Prior:** record\n");
    write_fixture_file(&template.join("tasks"), "001-coordinate.md", &waits_on_record);
    let before = read(&dir.join(PLAN));

    let placed =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);

    assert!(!placed.status.success(), "a new cycle is the placement's:\n{}", placed.stdout);
    let said = flattened_output(&placed);
    assert!(
        said.contains(&cycle_text(&["release.coordinate", "release.record"])),
        "the refusal names the new cycle whole:\n{said}"
    );
    assert!(!said.contains("\"release.1\""), "and not the one already there:\n{said}");
    assert_eq!(read(&dir.join(PLAN)), before, "nothing is written");
}
