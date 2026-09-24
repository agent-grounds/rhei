//! Moving to a lifetime allowance, and the one direction an adjustment cannot
//! go.
//!
//! Allowances change; consumption does not. Everything below is a consequence
//! of that one sentence.
//! §FS-rhei-budgets.3.2 §FS-rhei-budgets.10

use super::test_support::*;
use super::*;

fn spend(case: &Case, journal: &mut Journal, attempt: &str) {
    let ticket = case.ticket();
    let arms = arm(attempt);
    let group = journal
        .reserve(&request(case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
}

/// A project that never runs `init` is under the window contract, and the one
/// that does is under a lifetime total. The two bound different quantities, so
/// the account says which it holds rather than leaving a reader to infer it.
/// §FS-rhei-budgets.3
#[test]
fn an_account_moves_from_the_window_contract_to_a_lifetime_allowance() {
    let case = Case::new();
    let mut journal = case.open();
    assert_eq!(journal.contract().expect("contract"), Contract::Window);

    journal.adjust(500, &audit()).expect("init a lifetime allowance");

    assert_eq!(journal.contract().expect("contract"), Contract::Lifetime { allowance: 500 });
    assert_eq!(
        journal.snapshot().expect("snapshot").invocation_bound(200),
        500,
        "the ledger's own allowance is the bound, not the day's rate"
    );
}

/// Every adjustment is audited: the actor, the instant, the old and new
/// values, the reason, and the exact argv. An allowance that changed without a
/// record of who changed it is a number nobody can argue with.
/// §FS-rhei-budgets.10
#[test]
fn an_adjustment_records_who_changed_what_and_why() {
    let case = Case::new();
    let mut journal = case.open();

    journal
        .adjust(
            400,
            &Audit {
                actor: "operator".into(),
                written_at: "2026-09-24T13:00:00Z".into(),
                reason: "the triage loop needs a lifetime total".into(),
                argv: vec!["rhei".into(), "budget".into(), "init".into()],
            },
        )
        .expect("adjust");

    let receipt = journal.receipts().last().expect("the adjustment is a receipt");
    assert_eq!(receipt.kind, "adjust");
    assert_eq!(receipt.actor, "operator");
    assert_eq!(receipt.payload["old"], serde_json::json!({"mode": "window"}));
    assert_eq!(receipt.payload["new"]["invocations"], 400);
    assert_eq!(receipt.payload["audit"]["reason"], "the triage loop needs a lifetime total");
    assert_eq!(receipt.payload["audit"]["argv"][2], "init");
}

/// An allowance below what the project has already spent is refused, because
/// it would put it in deficit for work already done — and a deficit is a
/// number no later adjustment can honestly resolve.
/// §FS-rhei-budgets.10
#[test]
fn an_allowance_below_consumed_plus_outstanding_is_refused() {
    let case = Case::new();
    let mut journal = case.open();
    spend(&case, &mut journal, "attempt-1");
    spend(&case, &mut journal, "attempt-2");
    // One more that is outstanding rather than consumed: the floor counts both.
    let ticket = case.ticket();
    let arms = arm("attempt-3");
    journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve");

    let refused = journal.adjust(2, &audit()).expect_err("two is below the three already claimed");

    assert_eq!(refused.reason_code, "missing_bound");
    assert!(refused.message.contains('3'), "{}", refused.message);
    assert_eq!(journal.contract().expect("contract"), Contract::Window, "and nothing changed");
}

/// Exactly `consumed + outstanding` is lawful: it is a project with no
/// headroom, not a project in deficit. §FS-rhei-budgets.10
#[test]
fn an_allowance_exactly_at_the_floor_is_accepted() {
    let case = Case::new();
    let mut journal = case.open();
    spend(&case, &mut journal, "attempt-1");

    journal.adjust(1, &audit()).expect("a project with no headroom is still a project");

    assert_eq!(journal.snapshot().expect("snapshot").invocations.remaining(1).expect("left"), 0);
}

/// An allowance is never renewed by the passage of time. That is the whole
/// difference between the two contracts, and the one a surface must not blur:
/// a lifetime account on a new day is a lifetime account.
/// §FS-rhei-budgets.3.2
#[test]
fn a_lifetime_allowance_is_not_renewed_by_the_next_day() {
    let case = Case::at("2026-09-24T12:00:00Z");
    let ticket = case.ticket();
    {
        let mut journal = case.open();
        journal.adjust(1, &audit()).expect("a lifetime total of one");
        spend(&case, &mut journal, "attempt-1");
    }

    case.set_clock("2026-09-25T01:00:00Z");
    let mut journal = case.account.open(true).expect("reopen the next day");

    let arms = arm("attempt-2");
    let refused = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect_err("a lifetime total does not renew");
    assert_eq!(refused.reason_code, "invocation_exhausted");
    let spent = refused.exhaustion.expect("a spent bound says so");
    assert!(spent.contract.is_lifetime(), "and it says which contract stopped it");
}

/// A window account, by contrast, has the next day's capacity and has spent
/// none of it — while still reporting everything it has ever spent, so the two
/// numbers are visible side by side rather than one standing in for the other.
/// §FS-rhei-budgets.3.1 §FS-rhei-budgets.10
#[test]
fn a_window_account_reports_the_day_and_the_lifetime_separately() {
    let case = Case::at("2026-09-24T12:00:00Z");
    {
        let mut journal = case.open();
        spend(&case, &mut journal, "attempt-1");
        spend(&case, &mut journal, "attempt-2");
    }

    case.set_clock("2026-09-25T01:00:00Z");
    let journal = case.account.open(false).expect("reopen the next day");
    let snapshot = journal.snapshot().expect("snapshot");

    assert_eq!(snapshot.invocations, Counter::default(), "the new day has spent nothing");
    assert_eq!(snapshot.lifetime_invocations.consumed, 2, "the project has spent two");
}
