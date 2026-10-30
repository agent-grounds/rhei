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
fn issue537_program_sequential_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Program, 1);
}

#[test]
fn issue537_program_worker_pool_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Program, 2);
}

#[test]
fn issue537_agent_sequential_spent_visit_names_damaged_account() {
    damaged_account_named_over_spent_visit(Kind::Agent, 1);
}

#[test]
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

/// No refusal names the account, and the run did not leave the path it is for.
fn assert_no_account_refusal(result: &CliRun, what: &str) {
    let all = format!("{}\n{}", result.stdout, result.stderr);
    for refusal in ["cannot reach the budget path", "untrustworthy"] {
        assert!(!all.contains(refusal), "{what}: refused with {refusal:?}:\n{all}");
    }
    assert!(!all.contains("Falling back to sequential"), "{what}: off its path:\n{all}");
}

/// An account reset to absent is not damaged, so verification passes it
/// without establishing it, and a spent visit is reported with the `rhei
/// retry` that gets past it. §FS-rhei-budgets.5.4
#[test]
fn issue537_spent_visit_on_absent_account_is_reported_on_every_path() {
    for (kind, parallel) in PATHS {
        let world = SpentWorld::new(kind, parallel);
        let spending = world.run();
        assert!(!spending.status.success(), "the ticket never finishes");
        world.assert_spawned_on_its_path(&spending);
        let spent = world.spawns();
        assert_eq!(spent, world.tickets().len(), "one attempt spends each ticket's visit");

        world.reset_account();
        let halted = world.run();
        let what = format!("{kind:?} --parallel {parallel}, visit spent, account absent");
        let all = format!("{}\n{}", halted.stdout, halted.stderr);
        assert!(!halted.status.success(), "{what}: the run halts\n{all}");
        for wanted in ["attempts spent on this visit", "rhei retry"] {
            assert!(all.contains(wanted), "{what}: no {wanted:?} in:\n{all}");
        }
        assert_no_account_refusal(&halted, &what);
        assert_eq!(world.spawns(), spent, "{what}: a spent visit is not spawned again");
        world.assert_account_absent(&what);
    }
}

/// The same absent account with the visit unspent: the start is admitted and
/// spawns. A program establishes nothing until its edge; admission establishes
/// the account for an agent. §FS-rhei-budgets.5.4
#[test]
fn issue537_unspent_visit_on_absent_account_is_admitted_on_every_path() {
    for (kind, parallel) in PATHS {
        let world = SpentWorld::new(kind, parallel);
        world.reset_account();
        let started = world.run();
        let what = format!("{kind:?} --parallel {parallel}, visit unspent, account absent");
        world.assert_spawned_on_its_path(&started);
        assert_no_account_refusal(&started, &what);
        assert_eq!(world.spawns(), world.tickets().len(), "{what}: every ticket spawned");
    }
}
