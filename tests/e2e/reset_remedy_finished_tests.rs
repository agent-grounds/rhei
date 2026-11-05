//! A finished ticket a reset would not reopen. That reset still deletes the rhei's
//! results and logs, so `rhei complete` must not offer it as a way back. Every plan
//! here holds a sibling that ran, so the reset has something to delete, and every
//! test reads the reset's own preview, so the message is held to what the reset does
//! rather than to a string (agent-grounds/rhei#549).

// §FS-rhei-reset.5.1 §FS-rhei-reset.2.2 §FS-rhei-complete.4

use std::fs;

use super::reset_remedy_support::*;
use super::*;

/// `open` is the initial state, and `done` and `cancelled` are the two a reset can
/// leave the ticket in here, both final.
const MACHINE: &str = r#"name: reset-remedy-finished
version: 1
states:
  open:
    initial: true
    description: Not started
  work:
    description: Working
  done:
    final: true
    description: Finished
  cancelled:
    final: true
    description: Dropped
transitions:
  - { from: open, to: work, description: Start }
  - { from: work, to: done, description: Finish }
  - { from: "*", to: cancelled, description: Dropped }
"#;

const AUTHORED_FINISHED: &str = r#"# Rhei: Finished

## Tasks

### Task 1: The ticket the refusal is about, authored finished
**State:** done

### Task 2: A sibling that ran
**State:** open
"#;

/// The ledger rows of a ticket that left `first` and ended in `done`, written by hand
/// because they are about what a reset reads, not how a run came to leave them.
fn ledger_rows_leaving(first: &str) -> String {
    format!("plan.1 {first}@open\nplan.1 open@work\nplan.1 work@done\n")
}

/// `plan` as a lone rhei under `machine`, with the command pointed at the plan's file.
fn finished_beside_a_sibling(
    name: &str,
    relative_root: &str,
    machine: &str,
    plan: &str,
) -> RemedyWorld {
    let world = RemedyWorld::at(name, relative_root);
    world.write("states.yaml", machine);
    world.write("plan.rhei.md", plan);
    RemedyWorld { target: world.root.join("plan.rhei.md"), ..world }
}

fn finish_the_sibling(world: &RemedyWorld, from: &str) {
    let finish = ["--task", "2", "--from", from, "--to", "done"];
    world.step("transition", &[&finish[..], &["--result", "ran", "--no-callbacks"]].concat());
    assert!(
        world.root.join("runtime/results/plan.2.md").exists(),
        "fixture: the sibling left a result for the reset to take"
    );
}

/// Ticket 1 `done` beside a sibling that went `open` → `work` → `done`, under the
/// machine that has a state to reopen ticket 1 into.
fn world_with_a_reopenable_ticket(name: &str, relative_root: &str) -> RemedyWorld {
    let world = finished_beside_a_sibling(name, relative_root, MACHINE, AUTHORED_FINISHED);
    let start = ["--task", "2", "--from", "open", "--to", "work", "--no-callbacks"];
    world.step("transition", &start);
    finish_the_sibling(&world, "work");
    world
}

fn append_to_ledger(world: &RemedyWorld, rows: &str) {
    let path = world.root.join("runtime/state-transitions.log");
    let ledger = fs::read_to_string(&path).expect("fixture: the sibling left a ledger");
    fs::write(&path, format!("{ledger}{rows}")).expect("write the ledger");
}

/// What `rhei complete` says about ticket 1, once the fixture has shown it is the
/// refusal under test.
#[track_caller]
fn refused(world: &RemedyWorld) -> String {
    let run = world.rhei("complete", &["--task", "1", "--result", "again", "--no-callbacks"]);
    assert!(
        !run.status.success() && run.stderr.contains("already in terminal state"),
        "fixture: this is the refusal under test:\n{}{}",
        run.stdout,
        run.stderr
    );
    run.stderr
}

/// The message says a reset would leave ticket 1 in `dest`, and the reset's own
/// preview agrees: it either moves ticket 1 there from `done` or leaves it there.
/// Every `dest` here is final, so this is what shows the reset does not reopen it.
#[track_caller]
fn assert_a_reset_leaves_it_in(world: &RemedyWorld, said: &str, dest: &str) {
    assert!(
        said.contains(&format!(
            "A reset would not reopen it: it would leave this ticket in '{dest}'"
        )),
        "the message names where a reset would leave the ticket:\n{said}"
    );
    assert!(
        offered_resets(said).is_empty(),
        "a reset that leaves it finished is no way back:\n{said}"
    );

    let preview = world.rhei("reset", &["--rhei", "plan", "--dry-run"]);
    assert_success(&preview);
    let moved =
        preview.stdout.lines().find_map(|line| line.trim().strip_prefix("Task plan.1: done → "));
    assert_eq!(
        moved.unwrap_or("done"),
        dest,
        "the message says what the reset does, and the reset's preview said:\n{}",
        preview.stdout
    );
}

/// The operator's way back is the machine's initial state, and no reset sits beside it.
#[track_caller]
fn assert_reopens_only_by_force(world: &RemedyWorld, dest: &str) {
    let said = refused(world);
    assert_a_reset_leaves_it_in(world, &said, dest);
    let offer = assert_offers_forced_line(&said, &world.home);
    assert_eq!(offered_to(&offer), Some("open"), "it goes to the initial state:\n{offer}");
}

/// A ledger with no line for the ticket means it never moved, so reset leaves it.
#[test]
fn a_ticket_authored_finished_is_not_offered_a_reset() {
    let world = world_with_a_reopenable_ticket("reset-remedy-finished-authored", "lone");

    assert_reopens_only_by_force(&world, "done");
}

/// No ledger means nothing records where the ticket came from, so reset leaves it.
#[test]
fn a_finished_ticket_with_no_ledger_is_not_offered_a_reset() {
    let world = world_with_a_reopenable_ticket("reset-remedy-finished-no-ledger", "lone");
    fs::remove_file(world.root.join("runtime/state-transitions.log")).expect("remove the ledger");

    assert_reopens_only_by_force(&world, "done");
}

/// The ledger says the ticket started where it stands.
#[test]
fn a_ticket_that_started_where_it_stands_is_not_offered_a_reset() {
    let world = world_with_a_reopenable_ticket("reset-remedy-finished-same", "lone");
    append_to_ledger(&world, &ledger_rows_leaving("done"));

    assert_reopens_only_by_force(&world, "done");
}

/// The ledger says the ticket started in another final state, so reset moves it,
/// from one finished state to another.
#[test]
fn a_ticket_that_started_in_another_final_state_is_not_offered_a_reset() {
    let world = world_with_a_reopenable_ticket("reset-remedy-finished-other", "lone");
    append_to_ledger(&world, &ledger_rows_leaving("cancelled"));

    assert_reopens_only_by_force(&world, "cancelled");
}

/// The ledger says the ticket started in a state the machine no longer declares, and
/// reset will not write that back.
#[test]
fn a_ticket_that_started_in_an_undeclared_state_is_not_offered_a_reset() {
    let world = world_with_a_reopenable_ticket("reset-remedy-finished-undeclared", "lone");
    append_to_ledger(&world, &ledger_rows_leaving("renamed"));

    assert_reopens_only_by_force(&world, "done");
}

/// With no reset beside it, the forced line still arrives whole.
/// §FS-rhei-reset.5.1 §FS-rhei-errors.2
#[test]
fn the_forced_line_with_no_reset_beside_it_stays_on_one_line() {
    let world = world_with_a_reopenable_ticket("reset-remedy-finished-wide", &"w".repeat(90));

    assert_finished_tickets_commands_stay_whole(&world, 1);
}

const WORKED_SIBLING: &str = r#"# Rhei: Finished

## Tasks

### Task 1: The ticket the refusal is about, authored finished
**State:** done

### Task 2: A sibling that ran
**State:** work
"#;

/// Nothing a forced transition could go to: the message offers no command at all,
/// and still says why a reset is none.
#[track_caller]
fn assert_offers_no_command(name: &str, machine: &str) {
    let world = finished_beside_a_sibling(name, "lone", machine, WORKED_SIBLING);
    finish_the_sibling(&world, "work");

    let said = refused(&world);

    assert_a_reset_leaves_it_in(&world, &said, "done");
    assert!(offered_transitions(&said).is_empty(), "no state to reopen it into:\n{said}");
}

#[test]
fn a_machine_with_no_initial_state_offers_no_command() {
    assert_offers_no_command(
        "reset-remedy-finished-no-initial",
        r#"name: reset-remedy-no-initial
version: 1
states:
  work:
    description: Working
  done:
    final: true
    description: Finished
transitions:
  - { from: work, to: done, description: Finish }
"#,
    );
}

#[test]
fn a_machine_with_two_initial_states_offers_no_command() {
    assert_offers_no_command(
        "reset-remedy-finished-two-initials",
        r#"name: reset-remedy-two-initials
version: 1
states:
  first:
    initial: true
    description: One way in
  second:
    initial: true
    description: Another way in
  work:
    description: Working
  done:
    final: true
    description: Finished
transitions:
  - { from: first, to: work, description: Start }
  - { from: second, to: work, description: Start }
  - { from: work, to: done, description: Finish }
"#,
    );
}

#[test]
fn a_machine_whose_initial_state_is_final_offers_no_command() {
    assert_offers_no_command(
        "reset-remedy-finished-final-initial",
        r#"name: reset-remedy-final-initial
version: 1
states:
  done:
    initial: true
    final: true
    description: Finished
  work:
    description: Working
transitions:
  - { from: work, to: done, description: Finish }
"#,
    );
}

#[test]
fn a_profile_whose_initial_state_is_final_offers_no_command() {
    assert_offers_no_command(
        "reset-remedy-finished-profile",
        r#"name: reset-remedy-profile
version: 1
profiles:
  record:
    initial: done
    allowed: [work, done, cancelled]
node_policy:
  root: record
  default: record
states:
  work:
    description: Working
  done:
    final: true
    description: Finished
  cancelled:
    final: true
    description: Dropped
transitions:
  - { from: work, to: done, description: Finish }
  - { from: "*", to: cancelled, description: Dropped }
"#,
    );
}
