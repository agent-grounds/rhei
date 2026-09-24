//! The bounded synthetic cycle: a ticket that would ping-pong forever stops at
//! its travel bound, where it stands, and says what stopped it.
//!
//! This is the failure `agent-grounds/rhei#232` paid $66 for — a review/fix
//! loop that nothing outside the loop bounded.

// §FS-rhei-budgets.4.1 §FS-rhei-budgets.8 §FS-rhei-run.3.4

use std::fs;

use super::budget_support::*;
use super::*;

const FOUR_MOVES: &str = r#", "transition_limit": 4"#;

/// Four travel units buys four moves. The fifth spawn is the one that must not
/// happen: admission is checked *before* the work it bounds, so a ticket at its
/// bound never starts a process it cannot pay for.
// §FS-rhei-budgets.6.1
#[test]
fn a_ping_pong_ticket_halts_before_the_spawn_that_would_exceed_its_travel_bound() {
    let (dir, plan, machine) = setup("budget-travel-halt", PING_PONG_MACHINE, FOUR_MOVES);

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 4, "four travel units buys four moves and no fifth spawn");
    assert_eq!(ledger(&dir).len(), 4, "one ledger entry per applied edge: {:?}", ledger(&dir));
    assert!(!result.status.success(), "a run that ends with a halted ticket exits non-zero");
}

/// The ticket keeps the state it reached and the artifacts it earned. An engine
/// that advanced it, cancelled it, or wrote a result in its place would be
/// recording a verdict on work it never watched.
// §FS-rhei-budgets.8
#[test]
fn the_halted_ticket_keeps_its_state_its_artifacts_and_its_silence() {
    let (dir, plan, machine) = setup("budget-travel-preserved", PING_PONG_MACHINE, FOUR_MOVES);

    run_plan(&plan, &machine, None);

    // Four moves from `work`: work -> review -> work -> review -> work. The
    // count is asserted first on purpose: an unbounded loop ends on the same
    // state name every even number of moves, so the state alone would let this
    // pass on a build with no bound at all.
    assert_spawn_count(&dir, 4, "the ticket was stopped by its travel bound");
    assert_task_state(&plan, &machine, "1", "work");
    assert_eq!(
        fs::read_to_string(dir.join("runtime/work.md")).expect("the earned artifact survives"),
        "work\n"
    );
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "no terminal result is invented for a ticket that was stopped rather than finished"
    );
}

/// A halt that does not say what to do is a halt someone has to
/// reverse-engineer. Every field of §FS-rhei-budgets.8 is pinned, including the
/// one that is easiest to get wrong: the remedy names the key that actually
/// limits, never an inner value the ceiling would clamp.
// §FS-rhei-budgets.8
#[test]
fn the_halt_names_the_dimension_the_numbers_the_mode_and_one_remedy() {
    let (_dir, plan, machine) = setup("budget-travel-message", PING_PONG_MACHINE, FOUR_MOVES);

    let result = run_plan(&plan, &machine, None);

    assert_halt_mentions(&result, "ticket travel");
    assert_halt_mentions(&result, "4 (machine)");
    assert_halt_mentions(&result, "consumed:    4  outstanding: 0  remaining: 0");
    assert_halt_mentions(&result, "per ticket identity");
    assert_halt_mentions(&result, "set `defaults.transition_limit` in the machine settings file");
}

/// Travel outlives the run, and it outlives `rhei reset`. "Once per run,
/// forever" is exactly the unbounded case the bound exists for, and reset is
/// the other door into it: a count that reset returned would make reset the way
/// to buy more.
// §FS-rhei-budgets.4.1 §FS-rhei-reset
#[test]
fn reset_and_rerun_converges_on_the_travel_bound_instead_of_escaping_it() {
    let (dir, plan, machine) = setup("budget-travel-reset", PING_PONG_MACHINE, FOUR_MOVES);

    run_plan(&plan, &machine, None);
    assert_spawn_count(&dir, 4, "the first run spends the ticket's whole travel bound");

    // Reset deletes `runtime/`, and with it the spawn log: the rerun's own
    // count starts from nothing, which is why zero is the assertion below.
    assert_success(&run_at("reset", &plan, &machine, None, &["--yes"]));
    let after = run_plan(&plan, &machine, None);

    assert_spawn_count(
        &dir,
        0,
        "the ticket's identity kept its spent travel, so the rerun buys no move at all",
    );
    assert!(
        !after.status.success(),
        "a ticket whose identity has spent its travel is halted again on the next run"
    );
    assert_halt_mentions(&after, "ticket travel");
}
