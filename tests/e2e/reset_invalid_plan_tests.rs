//! A reset whose moves, each right for its own task, add up to a plan `rhei validate`
//! refuses. It is refused before it deletes anything, its preview refuses the same
//! way, and where editing a line lets it through the help says so
//! (agent-grounds/rhei#569).
//!
//! A plan validation already refuses is not reset here in either direction: whether
//! reset runs on one is rhei.81's, against §FS-rhei-reset.2 step 1.

// §FS-rhei-reset.2.3 §FS-rhei-reset.3 §FS-rhei-reset.4

use std::fs;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;
use super::*;

const RESET: [&str; 3] = ["--rhei", "plan", "--yes"];

/// The help's edit for the parent, when it is the way on. §FS-rhei-reset.4
#[track_caller]
fn assert_names_the_edit(said: &str, then: &str) {
    let opening = "a reset leaves Task plan.1 where its **State:** line in ";
    let rest = format!(
        "plan.rhei.md puts it, because nothing records a state it could restore: edit that \
         line to 'open'{then}, then run the reset again"
    );
    let line = said.lines().find(|line| line.contains(opening));
    assert!(
        line.is_some_and(|line| line.contains(&rest)),
        "the help names the edit that lets the reset through:\n{said}"
    );
}

/// Route H, the report's own: the child reopens, the parent stays finished.
#[test]
fn a_reset_that_reopens_a_child_under_a_parent_finished_by_hand_is_refused_intact() {
    let world = parent_finished_by_hand("reset-invalid-hand");

    let said = assert_reset_refused_intact(&world, &RESET);

    assert!(said.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{said}");
}

/// Route M: no hand edit. The parent stays because its first state is retired, and
/// the refusal keeps the line that says so.
#[test]
fn a_reset_under_a_machine_that_retired_the_parents_first_state_is_refused_and_says_why() {
    let world = parent_started_in_a_retired_state("reset-invalid-retired");

    let said = assert_reset_refused_intact(&world, &RESET);

    assert!(said.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{said}");
    assert!(
        said.contains(
            "Task plan.1 started in 'review', which this state machine no longer declares; \
             a reset would leave it in 'done'."
        ),
        "it says why the parent stays:\n{said}"
    );
}

/// Route S: no file edited. The run named its machine and the reset does not.
#[test]
fn a_reset_without_the_machine_the_run_used_is_refused_intact() {
    let world = finished_under_another_machine("reset-invalid-machine");

    let said = assert_reset_refused_intact(&world, &RESET);

    assert!(said.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{said}");
}

/// The preview is read as a promise about what the reset does, so it refuses too,
/// in the same words.
#[test]
fn the_preview_of_a_refused_reset_refuses_the_same_way() {
    let world = parent_finished_by_hand("reset-invalid-preview");

    let previewed = assert_reset_refused_intact(&world, &["--rhei", "plan", "--dry-run"]);
    let refused = assert_reset_refused_intact(&world, &RESET);

    assert_eq!(previewed, refused, "the preview and the reset refuse with the same text");
}

/// The reverse shape: the ledger sends the parent back to `done` over a child that
/// never moved. A check of only the ancestors a reset leaves in place misses it, and
/// editing the parent's line changes nothing, so the help does not offer that edit.
#[test]
fn a_ledger_row_that_finishes_a_parent_over_an_open_child_is_refused_and_names_no_edit() {
    let world = plan_world("reset-invalid-reverse", MACHINE, OPEN_PARENT);
    world.write("runtime/state-transitions.log", "plan.1 done@open\n");
    assert_validates(&world);

    let said = assert_reset_refused_intact(&world, &RESET);

    assert!(said.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{said}");
    assert!(!said.contains("edit that line"), "no edit to a line changes this:\n{said}");
    assert!(
        said.contains(
            "a reset moves Task plan.1 to 'done', the state the ledger records it started in, \
             whatever its **State:** line says"
        ),
        "the help says where the ledger sends it:\n{said}"
    );
}

/// Ticket 1 started in `draft`, which the machine still declares and its profile no
/// longer allows.
const PROFILE_MACHINE: &str = r#"name: reset-invalid-profile
version: 1
profiles:
  ticket:
    initial: draft
    allowed: [draft, open, done]
node_policy:
  root: ticket
  default: ticket
states:
  draft:
    description: Being drawn up
  open:
    description: Not started
  done:
    final: true
    description: Finished
transitions:
  - { from: draft, to: done, description: Finish }
  - { from: open, to: done, description: Finish }
"#;

/// A rule that is not about the tree, which is what shows the validator was asked
/// rather than one of its rules copied.
#[test]
fn a_recorded_state_the_profile_no_longer_allows_is_refused() {
    let plan = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: A ticket\n**State:** draft\n";
    let world = plan_world("reset-invalid-profile", PROFILE_MACHINE, plan);
    finish(&world, "1", &[]);
    let narrowed = PROFILE_MACHINE
        .replace("initial: draft", "initial: open")
        .replace("[draft, open, done]", "[open, done]");
    world.write("states.yaml", &narrowed);
    assert_validates(&world);

    let said = assert_reset_refused_intact(&world, &RESET);

    assert!(
        said.contains(
            "Task plan.1 has state 'draft' which is not allowed by its resolved profile. \
             Profile allows: [open, done]"
        ),
        "it names the rule in the validator's words:\n{said}"
    );
}

/// `a` ran cleanly; `b` holds route H's shape.
fn two_rheis(name: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    world.write("index.panta.md", "# Panta: Two\n");
    world.write("states.yaml", MACHINE);
    world.write("a.rhei.md", "# Rhei: A\n\n## Tasks\n\n### Task 1: Alone\n**State:** open\n");
    world.write("b.rhei.md", &OPEN_PARENT.replace("# Rhei: Plan", "# Rhei: B"));
    finish(&world, "a.1", &[]);
    finish(&world, "b.1.1", &[]);
    edit(&world, "b.rhei.md", &parent_line("open"), &parent_line("done"));
    assert_validates(&world);
    world
}

/// The control for scope: a narrowed reset leaves `b` as it stands, so it introduces
/// nothing and goes through. Passes today; it must keep passing.
#[test]
fn a_narrowed_reset_goes_through_when_only_another_rhei_would_be_left_invalid() {
    let world = two_rheis("reset-invalid-narrowed");
    let b_plan = fs::read(world.root.join("b.rhei.md")).expect("read b");

    assert_success(&world.rhei("reset", &["--rhei", "a", "--yes"]));

    assert_eq!(listed_state(&world, "a.1"), "open", "the named rhei goes back");
    assert_eq!(fs::read(world.root.join("b.rhei.md")).expect("read b"), b_plan, "b is untouched");
    assert!(world.root.join("runtime/results/b.1.1.md").exists(), "b keeps its result");
    assert_validates(&world);
}

#[test]
fn a_project_wide_reset_is_refused_and_names_the_other_rheis_tickets() {
    let world = two_rheis("reset-invalid-project");

    let said = assert_reset_refused_intact(&world, &["--yes"]);

    let error = "Task b.1 is in terminal state 'done' but descendant Task b.1.1 ('Child') is in \
         non-terminal state 'open'";
    assert!(said.contains(error), "it names b's tickets:\n{said}");
}

/// The way on for route H: the edit the help names, made, lets the reset through and
/// leaves a plan `rhei validate` accepts before the reset and after it.
#[test]
fn following_the_help_for_a_parent_finished_by_hand_lets_the_reset_through() {
    let world = parent_finished_by_hand("reset-invalid-hand-way-on");
    let said = assert_reset_refused_intact(&world, &RESET);
    assert_names_the_edit(&said, "");

    edit(&world, "plan.rhei.md", &parent_line("done"), &parent_line("open"));
    assert_validates(&world);
    assert_success(&world.rhei("reset", &RESET));

    assert_validates(&world);
    assert_eq!(listed_state(&world, "plan.1.1"), "open", "the child reopened");
}

/// The way on for route M: the parent finished through `rhei complete`, so its result
/// link goes with the edit, or validation would refuse the edited plan.
#[test]
fn following_the_help_under_a_retired_state_lets_the_reset_through() {
    let world = parent_started_in_a_retired_state("reset-invalid-retired-way-on");
    let said = assert_reset_refused_intact(&world, &RESET);
    assert_names_the_edit(&said, " and delete the `> **Result:**` line under it");

    edit(&world, "plan.rhei.md", &parent_line("done"), &parent_line("open"));
    edit(&world, "plan.rhei.md", "> **Result:** [plan.1](runtime/results/plan.1.md)\n", "");
    assert_validates(&world);
    assert_success(&world.rhei("reset", &RESET));

    assert_validates(&world);
    assert_eq!(listed_state(&world, "plan.1.1"), "open", "the child reopened");
}

/// Route H's plan with nodes of kind `ticket` and `step`. The validator's tree
/// sentence still says `Task <id>`, and the help names the parent all the same.
const TICKET_AND_STEP: &str = r#"# Rhei: Plan

---
structure:
  maxLevels: 2
  nodeKinds: [ticket, step]
---

## Tasks

### Ticket 1: Parent
**State:** open

#### Step 1.1: Child
**State:** open
"#;

/// The parent's line on [`TICKET_AND_STEP`].
fn ticket_line(state: &str) -> String {
    format!("### Ticket 1: Parent\n**State:** {state}\n")
}

/// Route H on a plan whose nodes are not of kind `task`: the help names the edit, and
/// following it lets the reset through. §FS-rhei-reset.4
#[test]
fn the_help_names_the_edit_on_a_plan_whose_nodes_are_not_tasks() {
    let world = plan_world("reset-invalid-kinds", MACHINE, TICKET_AND_STEP);
    finish(&world, "1.1", &[]);
    edit(&world, "plan.rhei.md", &ticket_line("open"), &ticket_line("done"));
    assert_validates(&world);

    let said = assert_reset_refused_intact(&world, &RESET);
    assert!(said.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{said}");
    assert_names_the_edit(&said, "");

    edit(&world, "plan.rhei.md", &ticket_line("done"), &ticket_line("open"));
    assert_validates(&world);
    assert_success(&world.rhei("reset", &RESET));
    assert_validates(&world);
    assert_eq!(listed_state(&world, "plan.1.1"), "open", "the child reopened");
}

/// The reverse shape on a plan whose nodes are not of kind `task`: the help says where
/// the ledger sends the parent. §FS-rhei-reset.4
#[test]
fn the_help_says_where_the_ledger_sends_a_parent_whose_kind_is_not_task() {
    let world = plan_world("reset-invalid-kinds-reverse", MACHINE, TICKET_AND_STEP);
    world.write("runtime/state-transitions.log", "plan.1 done@open\n");
    assert_validates(&world);

    let said = assert_reset_refused_intact(&world, &RESET);

    assert!(said.contains(PARENT_OVER_CHILD), "it names the parent and the child:\n{said}");
    assert!(!said.contains("edit that line"), "no edit to a line changes this:\n{said}");
    assert!(
        said.contains(
            "a reset moves Task plan.1 to 'done', the state the ledger records it started in, \
             whatever its **State:** line says"
        ),
        "the help says where the ledger sends it:\n{said}"
    );
}

/// The control: both finished through `rhei complete`, so the reset restores both. A
/// claim and the result links on finished tickets are what a reset strips, and must
/// not read as errors it introduces. Passes today; it must keep passing.
#[test]
fn a_parent_and_child_finished_through_complete_reset_with_their_claims_and_results() {
    let world = plan_world("reset-invalid-control", MACHINE, OPEN_PARENT);
    finish(&world, "1.1", &[]);
    finish(&world, "1", &[]);
    let claimed = format!("{}**Assignee:** someone\n", parent_line("done"));
    edit(&world, "plan.rhei.md", &parent_line("done"), &claimed);
    assert_validates(&world);

    assert_success(&world.rhei("reset", &RESET));

    assert_eq!(listed_state(&world, "plan.1"), "open");
    assert_eq!(listed_state(&world, "plan.1.1"), "open");
    let plan = fs::read_to_string(world.root.join("plan.rhei.md")).expect("read the plan");
    assert!(!plan.contains("**Assignee:**") && !plan.contains("> **Result:**"), "{plan}");
    assert_validates(&world);
}
