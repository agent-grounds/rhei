//! `rhei complete` over a finished ticket does not offer a reset that would be
//! refused for leaving a plan `rhei validate` refuses. The question is about the reset
//! it would offer, which is the whole rhei, so another tree in the same rhei decides it
//! as much as the ticket's own (agent-grounds/rhei#569).

// §FS-rhei-reset.5.1 §FS-rhei-complete.4 §FS-rhei-reset.2.3

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;

/// What `rhei complete` says about `ticket`, once the fixture has shown it is the
/// refusal under test.
#[track_caller]
fn refused(world: &RemedyWorld, ticket: &str) -> String {
    let run = world.rhei("complete", &["--task", ticket, "--result", "again", "--no-callbacks"]);
    assert!(
        !run.status.success() && run.stderr.contains("already in terminal state"),
        "fixture: this is the refusal under test:\n{}{}",
        run.stdout,
        run.stderr
    );
    run.stderr
}

/// The third ending: no reset command, and why the reset would be refused.
#[track_caller]
fn assert_says_the_reset_would_be_refused(said: &str) {
    assert!(offered_resets(said).is_empty(), "a refused reset is no way back:\n{said}");
    let tail = format!(
        "A reset of its rhei would be refused: the plan it would leave fails validation, \
         because {PARENT_OVER_CHILD}"
    );
    assert!(said.contains(&tail), "it says the reset would be refused, and why:\n{said}");
}

/// Route H's child: the help still starts at the finished parent, as #566 made it,
/// and ends without the reset.
#[test]
#[ignore = "red until #569: the help offers a reset that leaves the plan invalid; implement removes this"]
fn a_finished_child_under_a_parent_finished_by_hand_is_not_offered_the_reset() {
    let world = parent_finished_by_hand("reset-invalid-offer-child");

    let said = refused(&world, "1.1");

    assert_says_the_reset_would_be_refused(&said);
    let offers = offered_transitions(&said);
    let [offer] = offers.as_slice() else {
        panic!("one `rhei transition` is offered, found {}, in:\n{said}", offers.len());
    };
    assert_eq!(offered_flag(offer, "--task"), Some("plan.1"), "the parent's line:\n{offer}");
    assert!(
        said.contains(
            "Its ancestor plan.1 is finished too and has to be reopened first, then each \
             finished ticket below it down to this one"
        ),
        "the way back still starts at the parent:\n{said}"
    );
}

/// The parent's profile starts in `done`, so neither a reset nor its initial state
/// reopens it, while the child's ledger row would reopen the child.
const TERMINAL_INITIAL: &str = r#"name: reset-invalid-offer-initial
version: 1
profiles:
  parent:
    initial: done
    allowed: [draft, done]
  child:
    initial: open
    allowed: [open, done]
node_policy:
  root: child
  default: child
  overrides:
    - match: { level: 1 }
      profile: parent
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

/// The contradiction #566 left for this fix: an ancestor the help names with no
/// command is one a reset leaves finished, so the reset that followed it is refused.
#[test]
#[ignore = "red until #569: the help says no reset reopens the ancestor, then offers one; implement removes this"]
fn an_ancestor_nothing_reopens_is_not_followed_by_an_offered_reset() {
    let plan = OPEN_PARENT.replace(&parent_line("open"), &parent_line("draft"));
    let world = plan_world("reset-invalid-offer-stuck", TERMINAL_INITIAL, &plan);
    finish(&world, "1.1", &[]);
    edit(&world, "plan.rhei.md", &parent_line("draft"), &parent_line("done"));
    assert_validates(&world);

    let said = refused(&world, "1.1");

    assert_says_the_reset_would_be_refused(&said);
    assert!(offered_transitions(&said).is_empty(), "no line can start this:\n{said}");
    assert!(
        said.contains("neither a reset nor its profile's initial state would do that"),
        "it still says why the ancestor stays:\n{said}"
    );
}

/// Ticket 2 finished cleanly and has no ancestor, but the reset it would be offered
/// is the rhei's, and ticket 1's tree gets that reset refused.
#[test]
#[ignore = "red until #569: the help offers a rhei-wide reset another tree gets refused; implement removes this"]
fn a_ticket_whose_own_tree_is_fine_is_not_offered_a_reset_another_tree_gets_refused() {
    let plan = format!("{OPEN_PARENT}\n### Task 2: Beside it\n**State:** open\n");
    let world = plan_world("reset-invalid-offer-beside", MACHINE, &plan);
    finish(&world, "1.1", &[]);
    finish(&world, "2", &[]);
    edit(&world, "plan.rhei.md", &parent_line("open"), &parent_line("done"));
    assert_validates(&world);

    let said = refused(&world, "2");

    assert_says_the_reset_would_be_refused(&said);
    let offer = assert_offers_forced_line(&said, &world.home);
    assert_eq!(offered_flag(&offer, "--task"), Some("plan.2"), "its own line:\n{offer}");
    assert!(said.contains("can reopen this ticket alone"), "it reopens alone:\n{said}");
}
