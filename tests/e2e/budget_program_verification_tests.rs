//! Programs must verify account trust before work and report a refusal truthfully.
// §FS-rhei-budgets.5.4 §FS-rhei-budgets.9

use std::fs;

use super::budget_program_verification_support::World;
use super::*;

fn refused_before_work(parallel: usize) {
    let world = World::new(parallel, 0, true);
    let (journal, witness) = world.truncate_journal();
    let journal_before = fs::read(&journal).unwrap();
    let witness_before = fs::read(&witness).unwrap();
    let before = world.attempts();
    let mut outcomes = Vec::new();
    // More runs than max_attempts: refused work must never become an attempt.
    // §FS-rhei-budgets.5.4
    for _ in 0..4 {
        let result = world.run();
        assert!(!result.status.success(), "damaged account was accepted");
        assert!(result.stderr.contains("budget journal is untrustworthy"), "{}", result.stderr);
        assert!(result.stderr.contains("restore the journal by copying the witness"));
        assert!(result.stderr.contains("rhei budget show"));
        assert_eq!(world.state(), "claim");
        eprintln!(
            "refused run {}: parallel={parallel}, calls={:?}, metadata_unchanged={}, pool_spawn={}",
            outcomes.len() + 1,
            world.calls(),
            world.attempts() == before,
            result.stdout.contains("Claim the issue (parallel)")
        );
        outcomes.push((world.calls(), world.attempts()));
    }
    assert_eq!(fs::read(journal).unwrap(), journal_before, "no account repair or charge");
    assert_eq!(fs::read(witness).unwrap(), witness_before, "no witness charge");
    for (index, (calls, attempts)) in outcomes.iter().enumerate() {
        assert!(
            calls.is_empty(),
            "refused run {} executed program side effects: {calls:?}",
            index + 1
        );
        assert_eq!(attempts, &before, "refused run {} changed task/attempt metadata", index + 1);
    }
}

fn report_names_refusal(parallel: usize) {
    let world = World::new(parallel, 0, true);
    world.truncate_journal();
    for _ in 0..2 {
        let before = world.reports();
        let result = world.run();
        assert!(!result.status.success());
        assert!(result.stderr.contains("budget journal is untrustworthy"), "{}", result.stderr);
        let after = world.reports();
        let new: Vec<_> = after.iter().filter(|(path, _)| !before.contains_key(*path)).collect();
        assert_eq!(new.len(), 1, "each refused run owes a durable report");
        let report = String::from_utf8(new[0].1.clone()).unwrap();
        assert!(
            report.contains("budget journal is untrustworthy"),
            "durable report omitted the account refusal:\n{report}"
        );
        assert!(report.contains("restore the journal by copying the witness"), "{report}");
        assert!(report.contains("history.jsonl") && report.contains("journal.jsonl"), "{report}");
        assert!(!report.contains("Result: finished"), "{report}");
        assert!(!report.contains("waiting on the forge"), "{report}");
        assert!(!report.contains("poll resumes itself when the forge answers"), "{report}");
        assert!(report.contains("| program invocations | 0 |"), "{report}");
    }
}

#[test]
#[ignore = "red until #501: sequential programs verify before work; implement removes this"]
fn issue501_sequential_refuses_before_program_work() {
    refused_before_work(1);
}

#[test]
#[ignore = "red until #501: worker-pool programs verify before work; implement removes this"]
fn issue501_worker_pool_refuses_before_program_work() {
    refused_before_work(2);
}

#[test]
#[ignore = "red until #501: sequential reports name the account refusal; implement removes this"]
fn issue501_sequential_report_names_account_refusal() {
    report_names_refusal(1);
}

#[test]
#[ignore = "red until #501: worker-pool reports name the account refusal; implement removes this"]
fn issue501_worker_pool_report_names_account_refusal() {
    report_names_refusal(2);
}

/// Completion still spends travel, with no invocation charge.
/// §FS-rhei-budgets.4.3 §FS-rhei-budgets.4.1
#[test]
fn issue501_intact_program_completes_once_without_invocations() {
    for parallel in [1, 2] {
        let world = World::new(parallel, 0, true);
        let first = world.run();
        assert_success(&first);
        world.assert_pool_dispatch(&first);
        assert_eq!(world.state(), "completed");
        assert_success(&world.run());
        assert_eq!(world.calls(), "attempt=1\n", "one real exit-0 invocation");
        let shown = world.cli(&["budget", "show", "."]);
        assert_success(&shown);
        assert!(shown.stdout.contains("0 consumed + 0 outstanding / 10"), "{}", shown.stdout);
        let ledger = fs::read_to_string(world.root.join("runtime/state-transitions.log")).unwrap();
        assert!(ledger.contains("claim@completed"), "{ledger}");
    }
}

/// Refusing corrupt accounts must not change ordinary polling exhaustion.
/// §FS-rhei-states.2.2
#[test]
fn issue501_intact_program_poll_exhausts_at_two_attempts() {
    for parallel in [1, 2] {
        let world = World::new(parallel, 75, true);
        let result = world.run();
        assert_success(&result);
        world.assert_pool_dispatch(&result);
        assert_eq!(world.calls(), "attempt=1\nattempt=2\n");
        assert_eq!(world.state(), "human-review");
        assert_success(&world.run());
        assert_eq!(world.calls().lines().count(), 2, "no respawn after exhaustion");
    }
}

/// Verification alone creates no account; the applied edge establishes one.
/// §FS-rhei-budgets.5.4 §FS-rhei-budgets.4.3
#[test]
fn issue501_absent_account_stays_absent_until_program_edge() {
    for parallel in [1, 2] {
        let world = World::new(parallel, 0, false);
        let result = world.run();
        assert_success(&result);
        world.assert_pool_dispatch(&result);
        assert_eq!(world.state(), "completed");
        assert_eq!(
            fs::read_to_string(world.root.join("runtime/account-at-start.log")).unwrap(),
            "absent\n",
            "pre-start verification must not establish an account"
        );
        assert!(world.project.join(".agent-grounds/rhei/budgets").is_dir());
        let shown = world.cli(&["budget", "show", "."]);
        assert_success(&shown);
        assert!(shown.stdout.contains("0 consumed + 0 outstanding"), "{}", shown.stdout);
    }
}
