//! A fanout's arms settle one by one, each from its own record.
//!
//! A state that fans out over two reviewers starts two invocations, and the
//! account admits each on a reservation of its own. What the day carries for
//! each is what *that* invocation's record says, written when *that* arm exits.
//! The cases below are the ways a visit can read its arms as one: the first arm
//! to finish settling every arm, a measured amount landing on the other arm's
//! reservation, the shared travel unit going back while an arm still runs, and
//! a start or a descriptor that names an arm other than the one in hand.
//!
//! Every case orders the two exits by a handshake, never by sleeping, so the
//! journal the slow arm copies before it exits is the account as it stood
//! while that arm was still running. See [`super::budget_fan_out_support`].

// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.6.2 §FS-rhei-budgets.4.1

use super::budget_edge_support::account_journal;
use super::budget_fan_out_support::*;
use super::budget_support::*;

/// The report as filed: the codex arm finishes first, the claude arm is still
/// running, and both are measured. Each is charged its own amount at its own
/// exit — two measured receipts, $4.20 in all — and the first arm's exit
/// settles nothing for the arm still running. Charging that arm the reserve as
/// `unmeasurable` would put $20.00 on the day for an invocation whose cost is
/// on record.
// §FS-rhei-budgets.6.2.1
#[test]
fn each_fan_out_arm_settles_at_its_own_exit_from_its_own_record() {
    let fan = fan_out("budget-fan-out-settle", [CODEX, CLAUDE], "cld");
    fan.run(true);
    fan.assert_premise("cld");
    let reservations = fan.reservations();
    let receipts = account_journal(&fan.dir);

    assert_eq!(
        spends_on(&receipts, &reservations["cld"]),
        measured(CLAUDE_COST),
        "the claude arm is charged its own measured $3.00, not the reserve; journal:\n{receipts:#?}"
    );
    assert_eq!(
        spends_on(&receipts, &reservations["cdx"]),
        measured(CODEX_COST),
        "the codex arm is charged its own measured $1.20; journal:\n{receipts:#?}"
    );
    let at_exit = fan.journal_at_slow_exit();
    assert_eq!(
        (spends_on(&at_exit, &reservations["cdx"]), spends_on(&at_exit, &reservations["cld"])),
        (measured(CODEX_COST), Vec::new()),
        "the fast arm settled at its own exit, and nothing settled the slow arm before it exited; \
         journal then:\n{at_exit:#?}"
    );
    let shown = budget_show(&fan.plan, Some(SPEND_DAY));
    assert!(
        shown.stdout.contains("project spend: $4.20 consumed + $0.00 outstanding / $400.00"),
        "the day carries the two measured amounts; got:\n{}",
        shown.stdout
    );
}

/// Variant (a) of the report: the arm admitted second finishes first. Its
/// amount belongs to its own reservation, not to the first one admitted — the
/// first record to land is not the first arm's.
// §FS-rhei-budgets.6.2.1
#[test]
fn an_arm_that_finishes_first_is_booked_to_its_own_reservation() {
    let fan = fan_out("budget-fan-out-swapped", [CODEX, CLAUDE], "cdx");
    fan.run(true);
    fan.assert_premise("cdx");
    let reservations = fan.reservations();
    let receipts = account_journal(&fan.dir);

    assert_eq!(
        spends_on(&receipts, &reservations["cdx"]),
        measured(CODEX_COST),
        "the codex reservation carries the codex arm's $1.20, not the claude arm's $3.00; \
         journal:\n{receipts:#?}"
    );
    assert_eq!(
        spends_on(&receipts, &reservations["cld"]),
        measured(CLAUDE_COST),
        "the claude reservation carries the claude arm's $3.00; journal:\n{receipts:#?}"
    );
}

/// Variant (b) of the report: the visit's one travel unit, held on the first
/// arm's reservation, stays held while the other arm runs, and the edge the
/// last arm applies converts it. Giving it back at the first exit and charging
/// the edge a fresh unit spends the same count by a path that is checked
/// against the bound a second time, mid-visit.
// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.4.1
#[test]
fn a_fan_out_holds_its_one_travel_unit_until_no_arm_is_still_running() {
    let fan = fan_out("budget-fan-out-travel", [CODEX, CLAUDE], "cld");
    fan.run(true);
    fan.assert_premise("cld");
    let reservations = fan.reservations();
    let receipts = account_journal(&fan.dir);
    let releases = |receipts: &[serde_json::Value]| -> Vec<serde_json::Value> {
        receipts.iter().filter(|receipt| receipt["kind"] == "release").cloned().collect()
    };

    assert_eq!(
        releases(&fan.journal_at_slow_exit()),
        Vec::<serde_json::Value>::new(),
        "nothing is given back while the claude arm is still running"
    );
    assert_eq!(
        releases(&receipts),
        Vec::<serde_json::Value>::new(),
        "an edge was applied, so nothing goes back"
    );
    let edges: Vec<_> = receipts.iter().filter(|receipt| receipt["kind"] == "transition").collect();
    assert_eq!(edges.len(), 1, "one applied edge; journal:\n{receipts:#?}");
    assert_eq!(
        edges[0]["payload"]["reservation_id"], reservations["cdx"],
        "the edge converts the travel unit admission held on the first arm's reservation"
    );
}

/// Variant (c) of the report: no `--parallel` at all. `--parallel` counts
/// tickets and a fanout's arms stay together, so the arms still run at once and
/// each still settles from its own record.
// §FS-rhei-budgets.6.2.1
#[test]
fn a_fan_out_run_without_parallel_settles_each_arm_from_its_own_record() {
    let fan = fan_out("budget-fan-out-unflagged", [CODEX, CLAUDE], "cld");
    fan.run(false);
    fan.assert_premise("cld");
    let reservations = fan.reservations();
    let receipts = account_journal(&fan.dir);

    assert_eq!(
        (spends_on(&receipts, &reservations["cdx"]), spends_on(&receipts, &reservations["cld"])),
        (measured(CODEX_COST), measured(CLAUDE_COST)),
        "each arm is charged its own measured amount; journal:\n{receipts:#?}"
    );
}

/// The third row of the settle table survives the per-arm settle: an arm that
/// ends with no accounting record is charged the reserve as `unmeasurable`,
/// at its own exit, while its sibling is charged what it measured. Green before
/// the fix too: this guards the row the fix must not lose.
// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.6.2
#[test]
fn a_fan_out_arm_with_no_accounting_record_is_still_charged_the_reserve() {
    let fan = fan_out("budget-fan-out-unrecorded", [CODEX, UNRECORDED], "plain");
    fan.run(true);
    fan.assert_premise("plain");
    let reservations = fan.reservations();
    let receipts = account_journal(&fan.dir);

    assert_eq!(
        spends_on(&receipts, &reservations["plain"]),
        vec![(RESERVE, "unmeasurable".to_owned())],
        "an arm with no record is charged the reserve, never nothing; journal:\n{receipts:#?}"
    );
    assert_eq!(
        spends_on(&receipts, &reservations["cdx"]),
        measured(CODEX_COST),
        "its sibling is charged what it measured; journal:\n{receipts:#?}"
    );
}

/// An arm's descendants are handed the reservation that arm was admitted on,
/// so a nested run started by the second arm is placed under the second arm.
// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.7.1
#[test]
fn each_fan_out_arm_hands_its_descendants_its_own_reservation() {
    let fan = fan_out("budget-fan-out-ancestry", [CODEX, CLAUDE], "cld");
    fan.run(true);
    fan.assert_premise("cld");
    let reservations = fan.reservations();
    let reports = fan.arm_reports();

    for arm in ["cdx", "cld"] {
        assert_eq!(
            reports[arm]["parent_reservation"], reservations[arm],
            "the {arm} arm's RHEI_BUDGET_PARENT_RESERVATION names its own reservation; \
             reservations {reservations:?}, reports {reports:?}"
        );
    }
}

/// A start is confirmed when the process it records returns. The first arm's
/// return confirms the first arm's start and no other.
// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.6.2
#[test]
fn a_fan_out_arm_s_start_is_confirmed_only_once_its_own_process_returns() {
    let fan = fan_out("budget-fan-out-start", [CODEX, CLAUDE], "cld");
    fan.run(true);
    fan.assert_premise("cld");
    let reservations = fan.reservations();
    let confirmed = |receipts: &[serde_json::Value], arm: &str| {
        receipts_on(receipts, "start", &reservations[arm])
            .into_iter()
            .filter(|receipt| receipt["payload"]["status"] == "confirmed")
            .count()
    };
    let at_exit = fan.journal_at_slow_exit();

    assert_eq!(
        (confirmed(&at_exit, "cdx"), confirmed(&at_exit, "cld")),
        (1, 0),
        "while the claude arm runs only the codex arm's start is confirmed; journal then:\n{at_exit:#?}"
    );
    let receipts = account_journal(&fan.dir);
    assert_eq!(
        (confirmed(&receipts, "cdx"), confirmed(&receipts, "cld")),
        (1, 1),
        "each arm's start is confirmed once, when it returns; journal:\n{receipts:#?}"
    );
}
