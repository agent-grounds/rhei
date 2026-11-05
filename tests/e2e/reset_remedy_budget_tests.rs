//! A finished ticket whose way back runs into a spent `visits:` budget. The forced path
//! refuses a hop into a spent state before its terminal check, so a line `rhei complete`
//! prints into one never runs. Every line read here is run as printed, with no terminal,
//! and has to get past that refusal to the terminal check; where none can, the help has
//! to say which budget is spent rather than print one (agent-grounds/rhei#570).
//!
//! The refusal itself is right and is not under test, and neither is the help under it,
//! which belongs to agent-grounds/rhei#557.

// §FS-rhei-reset.5.1 §FS-rhei-complete.4 §FS-rhei-transition-cmd.6 §FS-rhei-errors.1.5

use std::fs;
use std::path::Path;

use super::reset_remedy_support::*;

/// The report's machine, with a `visits:` budget on each state `budgets` names.
fn machine(budgets: &[(&str, u32)]) -> String {
    let visits = |state: &str| {
        budgets
            .iter()
            .find(|(name, _)| *name == state)
            .map(|(_, limit)| format!(", visits: {limit}"))
            .unwrap_or_default()
    };
    format!(
        r#"name: reset-remedy-budget
version: 1
states:
  open: {{ initial: true{open}, description: Not started }}
  work: {{ description: Working{work} }}
  done: {{ final: true, description: Finished }}
  cancelled: {{ final: true, description: Dropped }}
transitions:
  - {{ from: open, to: work, description: Start }}
  - {{ from: open, to: done, description: Finish at once }}
  - {{ from: work, to: done, description: Finish }}
  - {{ from: "*", to: cancelled, description: Dropped }}
"#,
        open = visits("open"),
        work = visits("work"),
    )
}

/// One top-level ticket authored in `state`.
fn ticket_at(state: &str) -> String {
    format!("# Rhei: Project\n\n## Tasks\n\n### Task 1: A ticket\n**State:** {state}\n")
}

/// A parent authored in `state` over a child authored `done`, the ticket under test.
fn child_under(state: &str) -> String {
    format!(
        "# Rhei: Chain\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
         ### Task 1: A parent\n**State:** {state}\n\n#### Task 1.1: A child\n**State:** done\n"
    )
}

fn plan_world(name: &str, machine: &str, plan: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "plans");
    world.write("states.yaml", machine);
    world.write("plan.rhei.md", plan);
    RemedyWorld { target: world.root.join("plan.rhei.md"), ..world }
}

/// Ticket 1 leaves `work` for `cancelled`, so the ledger's first departure is `work`.
fn drop_from_work(world: &RemedyWorld) {
    let cancel = ["--task", "1", "--from", "work", "--to", "cancelled"];
    world.step("transition", &[&cancel[..], &["--result", "dropped", "--no-callbacks"]].concat());
}

fn finish(world: &RemedyWorld, ticket: &str) {
    world.step("complete", &["--task", ticket, "--result", "finished", "--no-callbacks"]);
}

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

/// What running `offer` as printed, from elsewhere and with no terminal, said.
fn refusal_of(offer: &str, home: &Path) -> String {
    let run = run_offer(offer, "", home);
    format!("{}{}", run.stdout, run.stderr)
}

/// The one line the help prints, which runs as printed past the forced path's budget
/// check to its terminal check: the hop, its destination and its budget were accepted.
/// §FS-rhei-reset.5.1 §FS-rhei-transition-cmd.6
#[track_caller]
fn assert_line_gets_past_the_budget(said: &str, home: &Path) -> String {
    let offers = offered_transitions(said);
    let [offer] = offers.as_slice() else {
        panic!("one `rhei transition` must be offered, found {}, in:\n{said}", offers.len());
    };
    let refusal = refusal_of(offer, home);
    assert!(
        !refusal.contains("budget for state"),
        "the printed line must not go to a state whose visit budget is spent. As printed, \
         with no terminal, it was refused for that budget.\n{offer} said:\n{refusal}\nin \
         the help:\n{said}"
    );
    assert_offers_forced_line(said, home)
}

/// No state the help could name gets past the budget, so it names none. Any line it
/// prints anyway is run, so the failure says what the forced path made of it.
/// §FS-rhei-reset.5.1 §FS-rhei-errors.6
#[track_caller]
fn assert_offers_no_line(said: &str, home: &Path) {
    let refused: Vec<String> = offered_transitions(said)
        .iter()
        .map(|offer| format!("{offer}\n  was refused with: {}", refusal_of(offer, home).trim()))
        .collect();
    assert!(
        refused.is_empty(),
        "no forced line can reopen this ticket, so the help prints none. It printed:\n{}\nin \
         the help:\n{said}",
        refused.join("\n")
    );
}

/// The reach of every reset the help offers, as the shared text words it.
const REACH: &str =
    "which returns every ticket in that rhei to its authored state and deletes their results and logs";

/// The one reset `said` offers, as printed.
#[track_caller]
fn the_one_reset(said: &str) -> String {
    let resets = offered_resets(said);
    let [reset] = resets.as_slice() else {
        panic!("one `rhei reset` must be offered, found {}, in:\n{said}", resets.len());
    };
    reset.clone()
}

/// The premise, and a guard for the fix: with a visit to spare the line goes back to
/// `work`, as today, gets as far as the terminal check, and says nothing of a budget.
#[test]
fn with_a_visit_to_spare_the_line_still_goes_back_to_its_reset_destination() {
    let world =
        plan_world("reset-remedy-budget-spare", &machine(&[("work", 2)]), &ticket_at("work"));
    drop_from_work(&world);

    let said = refused(&world, "1");

    let offer = assert_offers_forced_reopen(&said, &world.home);
    assert_eq!(offered_to(&offer), Some("work"), "its reset destination:\n{offer}");
    let reset = the_one_reset(&said);
    let whole = format!(
        "nothing to do — the task is finished. An operator at a terminal can reopen this ticket \
         alone with `{offer}`, giving their own reason; an agent may not run that. Otherwise, run \
         `{reset}`, {REACH}"
    );
    assert!(said.contains(&whole), "the message is unchanged:\n{said}");
    let prose: String = said.split('`').step_by(2).collect();
    assert!(!prose.contains("budget"), "no budget is spent here:\n{said}");
}

/// The report's shape: authored at `work`, its one visit spent, then dropped. The line
/// goes to the initial state instead and says why, and the reset, which clears the
/// count, is still offered after it.
#[test]
fn a_spent_reset_destination_sends_the_line_to_the_initial_state_and_says_why() {
    let world =
        plan_world("reset-remedy-budget-dest", &machine(&[("work", 1)]), &ticket_at("work"));
    drop_from_work(&world);

    let said = refused(&world, "1");

    let offer = assert_line_gets_past_the_budget(&said, &world.home);
    assert_eq!(offered_to(&offer), Some("open"), "the profile's initial state:\n{offer}");
    let reset = the_one_reset(&said);
    let whole = format!(
        "nothing to do — the task is finished. An operator at a terminal can reopen this ticket \
         alone with `{offer}`, giving their own reason; an agent may not run that. That command \
         goes to 'open' rather than 'work': visit budget for state 'work' is exhausted (1/1 \
         visits). Otherwise, run `{reset}`, {REACH}"
    );
    assert!(said.contains(&whole), "it says why the line does not go back to work:\n{said}");
    assert_offers_runnable_reset(&said, &world.home, "plan", &["plan.1"]);
}

/// The budget is on the initial state the ticket started in, so the reset destination
/// and the fallback are one spent state. No line, the budget named, and the reset, the
/// one way back, led in as that rather than as the other way.
#[test]
fn a_spent_initial_state_offers_no_line_and_names_the_budget() {
    let world =
        plan_world("reset-remedy-budget-initial", &machine(&[("open", 1)]), &ticket_at("open"));
    finish(&world, "1");

    let said = refused(&world, "1");

    assert_offers_no_line(&said, &world.home);
    let reset = the_one_reset(&said);
    let whole = format!(
        "nothing to do — the task is finished. It cannot be forced back into 'open': visit budget \
         for state 'open' is exhausted (1/1 visits). A reset reopens it with its visits cleared: \
         run `{reset}`, {REACH}"
    );
    assert!(said.contains(&whole), "it names the spent budget and the way back:\n{said}");
    assert_offers_runnable_reset(&said, &world.home, "plan", &["plan.1"]);

    let reopened = run_offer(&reset, "--yes", &world.home);
    assert!(reopened.status.success(), "{reset} --yes:\n{}{}", reopened.stdout, reopened.stderr);
    assert_eq!(world.state_of("plan.1"), "open", "the reset reopens it");
    let plan = fs::read_to_string(&world.target).expect("read the plan");
    assert!(!plan.contains("stateVisits"), "the reset clears the count:\n{plan}");
}

/// The same ticket with its ledger gone, so a reset would leave it finished and the only
/// line the help could build is the one to the spent initial state.
#[test]
fn with_no_ledger_a_spent_initial_state_offers_no_line_and_no_reset() {
    let world =
        plan_world("reset-remedy-budget-ledger", &machine(&[("open", 1)]), &ticket_at("open"));
    finish(&world, "1");
    fs::remove_file(world.root.join("runtime/state-transitions.log")).expect("drop the ledger");

    let said = refused(&world, "1");

    assert_offers_no_line(&said, &world.home);
    assert!(offered_resets(&said).is_empty(), "no reset would reopen it:\n{said}");
    let whole = "nothing to do — the task is finished. It cannot be forced back into 'open': visit \
                 budget for state 'open' is exhausted (1/1 visits). A reset would not reopen it: it \
                 would leave this ticket in 'done'";
    assert!(said.contains(whole), "it names the spent budget:\n{said}");
}

/// Two spent states, the reset destination and the initial state, are both named, the
/// reset destination first.
#[test]
fn two_spent_states_are_both_named() {
    let machine = machine(&[("open", 1), ("work", 1)]).replace(
        "transitions:\n",
        "transitions:\n  - { from: work, to: open, description: Back }\n",
    );
    let world = plan_world("reset-remedy-budget-both", &machine, &ticket_at("work"));
    world.step("transition", &["--task", "1", "--from", "work", "--to", "open", "--no-callbacks"]);
    let cancel = ["--task", "1", "--from", "open", "--to", "cancelled", "--result", "dropped"];
    world.step("transition", &[&cancel[..], &["--no-callbacks"]].concat());

    let said = refused(&world, "1");

    assert_offers_no_line(&said, &world.home);
    let reset = the_one_reset(&said);
    let whole = format!(
        "nothing to do — the task is finished. It cannot be forced back into 'work' or 'open': \
         visit budget for state 'work' is exhausted (1/1 visits); visit budget for state 'open' \
         is exhausted (1/1 visits). A reset reopens it with its visits cleared: run `{reset}`, \
         {REACH}"
    );
    assert!(said.contains(&whole), "it names both spent budgets:\n{said}");
}

const NO_RESET: &str = "A reset would not reopen it: it would leave this ticket in 'done'";

/// A child under a parent dropped from `work`, whose one visit is spent. The line is the
/// parent's, and it goes to the parent's initial state and says why.
#[test]
#[ignore = "red until #570: the shared helper wants a capital 'An operator', which the ancestor's line has mid-sentence; the next commit corrects it"]
fn a_spent_ancestor_destination_sends_its_line_to_its_initial_state() {
    let world =
        plan_world("reset-remedy-budget-ancestor", &machine(&[("work", 1)]), &child_under("work"));
    drop_from_work(&world);

    let said = refused(&world, "1.1");

    let offer = assert_line_gets_past_the_budget(&said, &world.home);
    let word = |flag| offered_flag(&offer, flag);
    assert_eq!(word("--task"), Some("plan.1"), "the topmost finished ancestor's line:\n{offer}");
    assert_eq!(word("--from"), Some("cancelled"), "its base state:\n{offer}");
    assert_eq!(word("--to"), Some("open"), "its profile's initial state:\n{offer}");
    let whole = format!(
        "nothing to do — the task is finished. Its ancestor plan.1 is finished too and has to be \
         reopened first, then each finished ticket below it down to this one: an operator at a \
         terminal starts with `{offer}`, giving their own reason; an agent may not run that. That \
         command goes to 'open' rather than 'work': visit budget for state 'work' is exhausted \
         (1/1 visits). {NO_RESET}"
    );
    assert!(said.contains(&whole), "it says why the ancestor's line does not go to work:\n{said}");
}

/// The parent started in the initial state that carries the budget and finished from
/// there, so nothing reopens it by force; the child's own line is refused under it.
#[test]
fn an_ancestor_whose_every_state_is_spent_is_named_with_its_budget() {
    let world =
        plan_world("reset-remedy-budget-stuck", &machine(&[("open", 1)]), &child_under("open"));
    finish(&world, "1");

    let said = refused(&world, "1.1");

    assert_offers_no_line(&said, &world.home);
    let whole = format!(
        "nothing to do — the task is finished. Its ancestor plan.1 is finished too and has to be \
         reopened first, and it cannot be forced back into 'open': visit budget for state 'open' \
         is exhausted (1/1 visits). {NO_RESET}"
    );
    assert!(said.contains(&whole), "it names the ancestor and its spent budget:\n{said}");
}
