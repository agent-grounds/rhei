//! A finished ticket under a finished ancestor. The forced path refuses that ticket
//! until every terminal ancestor has been reopened, so the line `rhei complete` prints
//! has to be the topmost finished ancestor's. Every test reads the line out of what
//! rhei printed and runs it, so the message is held to what the forced path does
//! rather than to a string (agent-grounds/rhei#559).
//!
//! Only the forced path's `×` line is read here. The `help:` line under it belongs to
//! agent-grounds/rhei#557.

// §FS-rhei-reset.5.1 §FS-rhei-complete.4 §FS-rhei-transition-cmd.6 §FS-rhei-errors.6

use std::fs;
use std::path::Path;

#[cfg(unix)]
use super::operator_force_support::{force_fixture, run_force_in_terminal};
use super::reset_remedy_support::*;
use super::*;

/// `work` is a state a ticket can be authored in and leave, so the ledger can record a
/// reset destination that is not the initial state.
const MACHINE: &str = r#"name: reset-remedy-ancestor
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
  - { from: open, to: done, description: Finish at once }
  - { from: work, to: done, description: Finish }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// Top-level tasks and subtasks under profiles of their own, so what a subtask
/// resolves says nothing about its parent.
fn machine_with_initials(parent: &str, child: &str) -> String {
    format!(
        r#"name: reset-remedy-ancestor-profiles
version: 1
profiles:
  parent:
    initial: {parent}
    allowed: [draft, done, cancelled]
  child:
    initial: {child}
    allowed: [open, done, cancelled]
node_policy:
  root: child
  default: child
  overrides:
    - match: {{ level: 1 }}
      profile: parent
states:
  draft:
    description: Being drawn up
  open:
    description: Not started
  done:
    final: true
    description: Finished
  cancelled:
    final: true
    description: Dropped
transitions:
  - {{ from: draft, to: done, description: Finish }}
  - {{ from: open, to: done, description: Finish }}
  - {{ from: "*", to: cancelled, description: Dropped }}
"#
    )
}

/// One task per level in `states`, top first, each the parent of the next. The ticket
/// is last, so what is appended to the plan lands on it.
fn chain(states: &[&str]) -> String {
    let mut plan =
        String::from("# Rhei: Chain\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n");
    for (level, state) in states.iter().enumerate() {
        let heading = "#".repeat(level + 3);
        let id = vec!["1"; level + 1].join(".");
        plan.push_str(&format!("\n{heading} Task {id}: A ticket\n**State:** {state}\n"));
    }
    plan
}

fn plan_world(name: &str, machine: &str, plan: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "plans");
    world.write("states.yaml", machine);
    world.write("plan.rhei.md", plan);
    RemedyWorld { target: world.root.join("plan.rhei.md"), ..world }
}

/// The arguments of the `rhei complete` a finished ticket refuses.
fn finish_again(ticket: &str) -> [&str; 5] {
    ["--task", ticket, "--result", "again", "--no-callbacks"]
}

/// What `rhei complete` says about `ticket`, once the fixture has shown it is the
/// refusal under test.
#[track_caller]
fn refused(world: &RemedyWorld, ticket: &str) -> String {
    let run = world.rhei("complete", &finish_again(ticket));
    assert!(
        !run.status.success() && run.stderr.contains("already in terminal state"),
        "fixture: this is the refusal under test:\n{}{}",
        run.stdout,
        run.stderr
    );
    run.stderr
}

/// What running `offer` as printed, from elsewhere and with no terminal, was refused with.
#[track_caller]
fn refusal_of(offer: &str, home: &Path) -> String {
    let run = run_offer(offer, "", home);
    assert!(!run.status.success(), "with no terminal {offer} must be refused:\n{}", run.stdout);
    format!("{}{}", run.stdout, run.stderr)
}

/// The one `rhei transition` the text offers, as printed.
#[track_caller]
fn the_one_offer(said: &str) -> String {
    let offers = offered_transitions(said);
    let [offer] = offers.as_slice() else {
        panic!("one `rhei transition` must be offered, found {}, in:\n{said}", offers.len());
    };
    offer.clone()
}

/// The word "alone" is about a ticket that can be reopened without anything else.
#[track_caller]
fn assert_does_not_say_alone(said: &str) {
    let prose: String = said.split('`').step_by(2).collect();
    assert!(!prose.contains("alone"), "this ticket cannot be reopened alone:\n{said}");
}

/// The help under a finished ancestor, with its one line returned. The terminal check
/// comes after the forced path's ancestor refusal, so reaching it shows the line gets
/// past that refusal, on every platform and with no pseudo-terminal.
/// §FS-rhei-reset.5.1 §FS-rhei-transition-cmd.6
#[track_caller]
fn assert_starts_at(said: &str, home: &Path, ancestor: &str) -> String {
    let offer = the_one_offer(said);
    let refusal = refusal_of(&offer, home);
    assert!(
        refusal.contains("--force requires an interactive terminal and fresh typed confirmation"),
        "the line the help prints must get as far as the terminal check.\n{offer} said:\n{refusal}"
    );
    assert_eq!(offered_flag(&offer, "--task"), Some(ancestor), "whose line it is:\n{offer}");
    let opening = format!(
        "nothing to do — the task is finished. Its ancestor {ancestor} is finished too and has to \
         be reopened first, then each finished ticket below it down to this one: an operator at \
         a terminal starts with `{offer}`, giving their own reason; an agent may not run that."
    );
    assert!(said.contains(&opening), "it says what comes first, and who may run it:\n{said}");
    assert_does_not_say_alone(said);
    offer
}

const NO_RESET: &str = "A reset would not reopen it: it would leave this ticket in 'done'";

/// The report's own shape: parent and child authored finished, so a reset is no way back.
#[test]
#[ignore = "red until #559: the help offers the ticket's line, which the forced path refuses for the parent; implement removes this"]
fn a_parent_authored_finished_comes_first() {
    let world = plan_world("reset-remedy-ancestor-authored", MACHINE, &chain(&["done", "done"]));

    let said = refused(&world, "1.1");

    assert_starts_at(&said, &world.home, "plan.1");
    assert!(said.contains(NO_RESET), "the ending is still about this ticket:\n{said}");
    assert!(offered_resets(&said).is_empty(), "no reset would reopen it:\n{said}");
}

/// Parent and child finished by a run, so the reset that was already offered still is,
/// after the cheaper way back.
#[test]
#[ignore = "red until #559: the ticket's line is offered beside the reset and refused for the parent; implement removes this"]
fn a_parent_finished_through_complete_comes_first_and_the_reset_stays_offered() {
    let world = plan_world("reset-remedy-ancestor-ran", MACHINE, &chain(&["open", "open"]));
    for ticket in ["1.1", "1"] {
        world.step("complete", &["--task", ticket, "--result", "finished", "--no-callbacks"]);
    }

    let said = refused(&world, "1.1");

    assert_starts_at(&said, &world.home, "plan.1");
    assert!(
        said.contains("an agent may not run that. Otherwise, run `rhei reset "),
        "the reset follows the way back:\n{said}"
    );
    assert_offers_runnable_reset(&said, &world.home, "plan", &["plan.1", "plan.1.1"]);
}

/// The nearest ancestor is the one the forced path's own refusal names, and it is not
/// where an operator can start.
#[test]
#[ignore = "red until #559: under three finished levels the line is the ticket's, and the refusal names only the nearest; implement removes this"]
fn under_three_finished_levels_the_line_is_the_topmosts() {
    let plan = chain(&["done", "done", "done"]);
    let world = plan_world("reset-remedy-ancestor-three", MACHINE, &plan);

    let said = refused(&world, "1.1.1");

    let offer = assert_starts_at(&said, &world.home, "plan.1");
    let nearest = offer.replace("--task plan.1 ", "--task plan.1.1 ");
    assert_ne!(nearest, offer, "fixture: the printed line names the task as written here");
    let refusal = refusal_of(&nearest, &world.home);
    assert!(
        refusal.contains("terminal ancestor plan.1 must be reopened"),
        "a line for the nearest would not get past the refusal.\n{nearest} said:\n{refusal}"
    );
}

/// Topmost finished is not topmost: the root here is open, and its line would reopen
/// nothing the ticket is waiting on.
#[test]
#[ignore = "red until #559: under an open root and a finished parent the line is the ticket's; implement removes this"]
fn an_open_root_is_not_what_comes_first() {
    let plan = chain(&["open", "done", "done"]);
    let world = plan_world("reset-remedy-ancestor-open-root", MACHINE, &plan);

    let said = refused(&world, "1.1.1");

    assert_starts_at(&said, &world.home, "plan.1.1");
}

/// The parent left `work` for `cancelled`, while the ticket was authored `done` and
/// never moved. Nothing about the ticket's own line would be right for the parent.
#[test]
#[ignore = "red until #559: the line carries the ticket's states rather than its ancestor's; implement removes this"]
fn the_line_carries_the_ancestors_own_states() {
    let world = plan_world("reset-remedy-ancestor-states", MACHINE, &chain(&["work", "done"]));
    let cancel = ["--task", "1", "--from", "work", "--to", "cancelled"];
    world.step("transition", &[&cancel[..], &["--result", "dropped", "--no-callbacks"]].concat());

    let said = refused(&world, "1.1");

    let offer = assert_starts_at(&said, &world.home, "plan.1");
    assert_eq!(offered_flag(&offer, "--from"), Some("cancelled"), "its base state:\n{offer}");
    assert_eq!(offered_flag(&offer, "--to"), Some("work"), "its reset destination:\n{offer}");
    assert!(said.contains(NO_RESET), "the ending is still about this ticket:\n{said}");
}

/// The parent's reset would leave it `done`, so it goes to its own profile's initial
/// state, which its child's profile does not allow and the forced path would refuse.
#[test]
#[ignore = "red until #559: the line goes to the ticket's initial state rather than its ancestor's; implement removes this"]
fn the_line_goes_to_the_initial_state_of_the_ancestors_own_profile() {
    let machine = machine_with_initials("draft", "open");
    let world = plan_world("reset-remedy-ancestor-profile", &machine, &chain(&["done", "done"]));

    let said = refused(&world, "1.1");

    let offer = assert_starts_at(&said, &world.home, "plan.1");
    assert_eq!(offered_flag(&offer, "--to"), Some("draft"), "its own initial state:\n{offer}");
}

/// A counted visit on the ancestor's state is not part of its base state, and neither
/// finding it finished nor naming the hop may read it as one.
#[test]
#[ignore = "red until #559: the line is the ticket's, whatever the ancestor's visit count; implement removes this"]
fn the_line_carries_the_ancestors_base_state_past_a_counted_visit() {
    let machine = MACHINE.replace("    final: true\n", "    final: true\n    visits: 5\n");
    let world = plan_world("reset-remedy-ancestor-visit", &machine, &chain(&["done-2", "done"]));

    let said = refused(&world, "1.1");

    let offer = assert_starts_at(&said, &world.home, "plan.1");
    assert_eq!(offered_flag(&offer, "--from"), Some("done"), "its base state:\n{offer}");
}

/// There is nothing for the operator to run, and the reader is told why rather than
/// left with this ticket's line, which the forced path would refuse.
#[test]
#[ignore = "red until #559: the ticket's line is offered though nothing can reopen its parent; implement removes this"]
fn an_ancestor_with_no_state_to_reopen_into_is_named_with_no_command() {
    let machine = machine_with_initials("done", "open");
    let world = plan_world("reset-remedy-ancestor-stuck", &machine, &chain(&["done", "done"]));

    let said = refused(&world, "1.1");

    assert!(offered_transitions(&said).is_empty(), "no line can start this:\n{said}");
    let whole = format!(
        "nothing to do — the task is finished. Its ancestor plan.1 is finished too and has to be \
         reopened first, and there is no state to reopen it into. {NO_RESET}"
    );
    assert!(said.contains(&whole), "it names the ancestor and says why:\n{said}");
    assert_does_not_say_alone(&said);
}

/// A member named relatively under a machine named relatively: the ancestor's line
/// runs from elsewhere only if it carries both, absolute, and the machine is not where
/// the member would find one. The help keeps its one line, in the bytes as written.
/// §FS-rhei-reset.5.1 §FS-rhei-errors.2
#[test]
#[ignore = "red until #559: the machine is passed on in a line that is the ticket's rather than its ancestor's; implement removes this"]
fn the_ancestors_line_passes_the_machine_on_and_stays_on_one_line() {
    let world = RemedyWorld::at("reset-remedy-ancestor-member", &"w".repeat(90));
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", &chain(&["done", "done"]));
    world.write("beta.rhei.md", &chain(&["open"]));
    let machines = world.dir.join("machines");
    fs::create_dir_all(&machines).expect("create the machine's directory");
    fs::write(machines.join("states.yaml"), MACHINE).expect("write the machine");
    let world = RemedyWorld {
        cwd: world.root.clone(),
        target: "alpha.rhei.md".into(),
        machine: Some("../machines/states.yaml".into()),
        ..world
    };

    let said = refused(&world, "1.1");

    let offer = assert_starts_at(&said, &world.home, "alpha.1");
    let machine = offered_flag(&offer, "--state-machine").expect("the machine is passed on");
    let machine = Path::new(machine.trim_matches(['\'', '"']));
    assert!(machine.is_absolute(), "it is read from another directory:\n{offer}");
    assert!(offer.len() > 80, "fixture: {offer} must not fit the render");
    let rendered = raw_stderr(&world.rhei_output("complete", &finish_again("1.1")));
    assert!(
        rendered.lines().any(|line| {
            line.contains("Its ancestor alpha.1 is finished")
                && line.contains(&format!("`{offer}`"))
        }),
        "the help must arrive on one line, and was rendered:\n{rendered}"
    );
}

/// A claim below the ancestor stops its line too, and that refusal names the holder and
/// the remedy, so the message still does not predict it.
#[test]
#[ignore = "red until #559: under a finished parent the line is the ticket's, not the parent's; implement removes this"]
fn under_a_finished_parent_a_claim_is_left_to_the_forced_paths_refusal() {
    let plan = format!("{}**Assignee:** someone\n", chain(&["done", "done"]));
    let world = plan_world("reset-remedy-ancestor-claimed", MACHINE, &plan);

    let said = refused(&world, "1.1");

    let offer = the_one_offer(&said);
    assert_eq!(offered_flag(&offer, "--task"), Some("plan.1"), "whose line it is:\n{offer}");
    assert!(!said.contains("someone"), "the message does not predict a claim:\n{said}");
    let refusal = refusal_of(&offer, &world.home);
    assert!(
        refusal.contains("Task plan.1.1 is assigned to someone; release the claim"),
        "the refusal names the holder and the remedy.\n{offer} said:\n{refusal}"
    );
}

/// What the help promises, at the terminal it is for: each line it prints reopens the
/// ticket it names, the next `rhei complete` prints the next one down, and the last is
/// this ticket's own. The reason is replaced, as the help says to.
#[cfg(unix)]
#[test]
#[ignore = "red until #559: at a terminal the printed line is refused for a finished ancestor; implement removes this"]
fn at_a_terminal_each_printed_line_reopens_its_ticket_down_to_this_one() {
    let plan = chain(&["done", "done", "done"]);
    let fixture = force_fixture("reset-remedy-ancestor-walk", &plan, MACHINE);

    for expected in ["plan.1", "plan.1.1", "plan.1.1.1"] {
        let said =
            run_cli("complete", &fixture.plan, &fixture.machine, &finish_again("1.1.1")).stderr;
        let offer = the_one_offer(&said);
        let word = |flag| offered_flag(&offer, flag).expect("the line names its hop");
        let task = word("--task");
        let local = task.strip_prefix("plan.").expect("the line names a qualified task");

        let hop = run_force_in_terminal(
            &fixture,
            local,
            word("--from"),
            word("--to"),
            "walk down the chain",
            None,
        );

        assert!(hop.status.success(), "at a terminal, {offer} said:\n{}", hop.transcript);
        assert_eq!(task, expected, "the help starts at the topmost finished ticket:\n{said}");
    }
    assert_task_state(&fixture.plan, &fixture.machine, "1.1.1", "open");
}

/// Today's message, word for word: this ticket's own line, reopening it alone.
#[track_caller]
fn assert_keeps_its_own_line(said: &str, home: &Path, ticket: &str, tail: &str) {
    let offer = assert_offers_forced_line(said, home);
    assert_eq!(offered_flag(&offer, "--task"), Some(ticket), "whose line it is:\n{offer}");
    let whole = format!(
        "nothing to do — the task is finished. An operator at a terminal can reopen this ticket \
         alone with `{offer}`, giving their own reason; an agent may not run that. {tail}"
    );
    assert!(said.contains(&whole), "the message is unchanged:\n{said}");
}

/// The control: under an open parent the line is a true one-ticket way back.
#[test]
fn under_an_open_parent_the_line_stays_this_tickets_alone() {
    let world = plan_world("reset-remedy-ancestor-open", MACHINE, &chain(&["open", "done"]));

    let said = refused(&world, "1.1");

    assert_keeps_its_own_line(&said, &world.home, "plan.1.1", NO_RESET);
}

/// The control: a top-level ticket has no ancestor at all, and its reset is unchanged.
#[test]
fn a_top_level_ticket_keeps_its_own_line_and_the_reset_beside_it() {
    let world = plan_world("reset-remedy-ancestor-top", MACHINE, &chain(&["open"]));
    world.step("complete", &["--task", "1", "--result", "finished", "--no-callbacks"]);

    let said = refused(&world, "1");

    let [reset] = &offered_resets(&said)[..] else {
        panic!("one reset is offered in:\n{said}");
    };
    let tail = format!(
        "Otherwise, run `{reset}`, which returns every ticket in that rhei to its authored state \
         and deletes their results and logs"
    );
    assert_keeps_its_own_line(&said, &world.home, "plan.1", &tail);
}

/// A ticket nothing can reopen offers no line, so nothing in its message is false, and
/// its finished parent changes none of it.
#[test]
fn a_ticket_with_no_state_of_its_own_keeps_its_message_under_a_finished_parent() {
    let machine = machine_with_initials("draft", "done");
    let world = plan_world("reset-remedy-ancestor-record", &machine, &chain(&["done", "done"]));

    let said = refused(&world, "1.1");

    assert!(offered_transitions(&said).is_empty(), "no state to reopen it into:\n{said}");
    let whole = format!("nothing to do — the task is finished. {NO_RESET}");
    assert!(said.contains(&whole), "the message is unchanged:\n{said}");
    assert!(!said.contains("ancestor"), "it says nothing about the parent:\n{said}");
}

/// The control for what stays unpredicted: nothing else has to be reopened, so the
/// line stays this ticket's, and the forced path names the holder and the remedy.
#[test]
fn under_an_open_parent_a_claim_is_left_to_the_forced_paths_refusal() {
    let plan = format!("{}**Assignee:** someone\n", chain(&["open", "done"]));
    let world = plan_world("reset-remedy-ancestor-open-claimed", MACHINE, &plan);

    let said = refused(&world, "1.1");

    let offer = the_one_offer(&said);
    assert_eq!(offered_flag(&offer, "--task"), Some("plan.1.1"), "whose line it is:\n{offer}");
    assert!(said.contains("can reopen this ticket alone"), "it reopens alone:\n{said}");
    assert!(!said.contains("someone"), "the message does not predict a claim:\n{said}");
    let refusal = refusal_of(&offer, &world.home);
    assert!(
        refusal.contains("Task plan.1.1 is assigned to someone; release the claim"),
        "the refusal names the holder and the remedy.\n{offer} said:\n{refusal}"
    );
}
