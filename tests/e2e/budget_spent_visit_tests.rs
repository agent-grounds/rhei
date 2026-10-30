//! A spent visit must not hide a damaged budget account. On every spawn path,
//! the first run on a damaged account names the account refusal, spent visit or
//! not, because no `rhei retry` gets past it.
// §FS-rhei-budgets.5.4 §FS-rhei-agents.3.2.3

use super::budget_spent_visit_support::{changed, Kind, SpentWorld};
use super::*;

const PATHS: [(Kind, usize); 4] =
    [(Kind::Program, 1), (Kind::Program, 2), (Kind::Agent, 1), (Kind::Agent, 2)];

fn offers_retry(result: &CliRun) -> bool {
    let all = format!("{}\n{}", result.stdout, result.stderr);
    all.contains("attempts spent on this visit") || all.contains("rhei retry")
}

/// The refusal an unspent visit on the same path gives: the diagnosis, its
/// remedy, the inspection help, exit 1, and nothing else to do.
fn assert_account_refused(result: &CliRun, what: &str) {
    assert_eq!(result.status.code(), Some(1), "{what}: {}", result.stderr);
    for wanted in [
        "budget journal is untrustworthy",
        "restore the journal by copying the witness back over it",
        "inspect the project's account with: rhei budget show",
    ] {
        assert!(result.stderr.contains(wanted), "{what}: no {wanted:?} in:\n{}", result.stderr);
    }
    assert!(!offers_retry(result), "{what}: offers a retry:\n{}{}", result.stdout, result.stderr);
}

fn damaged_account_named_over_spent_visit(kind: Kind, parallel: usize) {
    let world = SpentWorld::new(kind, parallel);
    let spending = world.run();
    assert!(!spending.status.success(), "the ticket never finishes");
    world.assert_spawned_on_its_path(&spending);
    let spent = world.spawns();
    assert_eq!(spent, world.tickets().len(), "one attempt spends each ticket's visit");
    let premise = world.run();
    assert!(offers_retry(&premise), "premise: a spent visit\n{}", premise.stderr);
    assert_eq!(world.spawns(), spent, "a spent visit is not spawned again");

    world.truncate_journal();
    let before = world.snapshot();
    let refused = world.run();
    let what = format!("{kind:?} --parallel {parallel}, visit spent, account damaged");
    assert!(
        !offers_retry(&refused) && refused.stderr.contains("budget journal is untrustworthy"),
        "{what}: the first run reported the spent visit, not the damaged account:\n{}{}",
        refused.stdout,
        refused.stderr
    );
    assert_account_refused(&refused, &what);
    assert_eq!(world.spawns(), spent, "{what}: the refused run spawned");
    let after = world.snapshot();
    assert!(after == before, "{what}: the refused run wrote {:?}", changed(&before, &after));
}

#[test]
#[ignore = "red until #537: a spent visit hides a damaged account; implement removes this"]
fn issue537_program_sequential_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Program, 1);
}

#[test]
#[ignore = "red until #537: a spent visit hides a damaged account; implement removes this"]
fn issue537_program_worker_pool_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Program, 2);
}

#[test]
#[ignore = "red until #537: a spent visit hides a damaged account; implement removes this"]
fn issue537_agent_sequential_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Agent, 1);
}

#[test]
#[ignore = "red until #537: a spent visit hides a damaged account; implement removes this"]
fn issue537_agent_worker_pool_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Agent, 2);
}

/// What already holds, so the fixture is the report's shape: with the visit
/// unspent, the same damage is refused on the first run and nothing spawns.
/// §FS-rhei-budgets.5.4
#[test]
fn issue537_unspent_visit_on_damaged_account_is_refused_on_every_path() {
    for (kind, parallel) in PATHS {
        let world = SpentWorld::new(kind, parallel);
        world.truncate_journal();
        let before = world.snapshot();
        let refused = world.run();
        let what = format!("{kind:?} --parallel {parallel}, visit unspent, account damaged");
        assert_account_refused(&refused, &what);
        assert_eq!(world.spawns(), 0, "{what}: the refused run spawned");
        let after = world.snapshot();
        assert!(after == before, "{what}: the refused run wrote {:?}", changed(&before, &after));
    }
}
