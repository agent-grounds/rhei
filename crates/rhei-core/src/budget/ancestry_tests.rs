//! Nested and embedded work is placed under a live ancestor of this very
//! project, or it is refused.
//!
//! An ancestry token names a reservation; the ledger decides whether that name
//! buys anything. These are the cases carved from the provider-spend work, with
//! the monetary assertions replaced by invocation counts — the shape of the
//! laundering route is the same whether what is laundered is money or a count.
//! §FS-rhei-budgets.7 §AR-neural-admission.6

use super::test_support::*;
use super::*;

/// An ancestor that has started and permits `envelope` descendant invocations.
fn live_ancestor(case: &Case, journal: &mut Journal, envelope: u64) -> String {
    let ticket = case.ticket();
    let arms = [Arm { attempt_identity: "ancestor", descendant_envelope: envelope }];
    let group = journal
        .reserve(&request(case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve the ancestor");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start the ancestor");
    group.reservation_ids[0].clone()
}

fn descendant<'a>(
    case: &'a Case,
    ticket: &'a str,
    arms: &'a [Arm<'a>],
    parent: Option<&'a str>,
) -> AdmissionRequest<'a> {
    AdmissionRequest { parent_reservation: parent, ..request(case, ticket, arms, false) }
}

/// A nested runtime that names an ancestor this ledger does not hold gets a
/// typed refusal rather than a balance of its own — the laundering route the
/// ancestry rule closes. §FS-rhei-budgets.7
#[test]
fn a_descendant_of_an_unknown_ancestor_cannot_open_an_account() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("nested");

    let refused = journal
        .reserve(
            &descendant(&case, &ticket, &arms, Some("reservation:absent")),
            bounds(80, 200),
            &audit(),
        )
        .expect_err("an unknown ancestor buys nothing");

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("no such reservation"), "{}", refused.message);
}

/// An envelope of zero means what it says: this invocation may start no nested
/// neural work at all, even though the ancestor itself is live.
/// §FS-rhei-budgets.7
#[test]
fn an_ancestor_whose_envelope_is_zero_lends_nothing() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let parent = live_ancestor(&case, &mut journal, 0);
    let arms = arm("nested");

    let refused = journal
        .reserve(&descendant(&case, &ticket, &arms, Some(&parent)), bounds(80, 200), &audit())
        .expect_err("a zero envelope permits nothing");

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("permits no nested neural work"), "{}", refused.message);
}

/// An ancestor that has not started yet lends nothing either: a reservation is
/// not an invocation, and a descendant of a spawn that never happened has no
/// parent to be under. §AR-neural-admission.6
#[test]
fn an_ancestor_that_has_not_started_lends_nothing() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = [Arm { attempt_identity: "ancestor", descendant_envelope: 4 }];
    let group = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve");
    let parent = group.reservation_ids[0].clone();
    let nested = arm("nested");

    let refused = journal
        .reserve(&descendant(&case, &ticket, &nested, Some(&parent)), bounds(80, 200), &audit())
        .expect_err("an unstarted ancestor is not outstanding work");

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("has not started"), "{}", refused.message);
}

/// With a live ancestor the descendant is an ordinary ledger entry: it consumes
/// its own invocation unit against the same account, and what its siblings may
/// together draw is bounded by the envelope the ancestor reserved.
/// §AR-neural-admission.6
#[test]
fn a_descendant_fits_inside_its_ancestors_envelope_and_no_further() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let parent = live_ancestor(&case, &mut journal, 2);
    let before = journal.snapshot().expect("snapshot").invocations.exposure().expect("exposure");

    for attempt in ["nested-1", "nested-2"] {
        let arms = arm(attempt);
        let group = journal
            .reserve(&descendant(&case, &ticket, &arms, Some(&parent)), bounds(80, 200), &audit())
            .expect("inside the envelope");
        journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    }

    let after = journal.snapshot().expect("snapshot").invocations.exposure().expect("exposure");
    assert_eq!(after, before + 2, "each descendant charges the shared account exactly once");

    let arms = arm("nested-3");
    let refused = journal
        .reserve(&descendant(&case, &ticket, &arms, Some(&parent)), bounds(80, 200), &audit())
        .expect_err("a third would draw three against an envelope of two");
    assert_eq!(refused.reason_code, "ancestor_envelope_exhausted");
    assert!(refused.message.contains('2'), "{}", refused.message);
}

/// The whole chain is re-derived on every open, so a receipt naming an ancestor
/// that is absent or overdrawn is corruption rather than capacity. A cached
/// number would be a number an editor could change. §AR-neural-admission.5
#[test]
fn a_replayed_chain_re_derives_every_ancestry_claim() {
    let case = Case::new();
    let ticket = case.ticket();
    let parent = {
        let mut journal = case.open();
        let parent = live_ancestor(&case, &mut journal, 2);
        let arms = arm("nested-1");
        journal
            .reserve(&descendant(&case, &ticket, &arms, Some(&parent)), bounds(80, 200), &audit())
            .expect("reserve the descendant");
        parent
    };

    let replayed = case.account.open(false).expect("reopen");
    let snapshot = replayed.snapshot().expect("snapshot");

    assert_eq!(snapshot.reservations.len(), 2, "ancestor and descendant are both ledger entries");
    assert_eq!(
        snapshot
            .reservations
            .values()
            .filter(|row| row["parent_reservation"].as_str() == Some(parent.as_str()))
            .count(),
        1,
        "and the descendant survives replay still naming its ancestor"
    );
}

/// The sixth case, which the window contract adds: a renewal at midnight
/// enlarges, revives and extends nothing an ancestor already holds.
///
/// The new day has capacity of its own for *new* admissions. What it must not
/// do is make yesterday's envelope bigger, because an ancestor's envelope is a
/// property of the invocation that was admitted, not of the calendar.
/// §FS-rhei-budgets.7 §FS-rhei-budgets.3.3
#[test]
fn a_midnight_renewal_enlarges_nothing_an_ancestor_already_holds() {
    let case = Case::at("2026-09-24T23:00:00Z");
    let ticket = case.ticket();
    let parent = {
        let mut journal = case.open();
        let parent = live_ancestor(&case, &mut journal, 1);
        let arms = arm("nested-1");
        journal
            .reserve(&descendant(&case, &ticket, &arms, Some(&parent)), bounds(80, 200), &audit())
            .expect("the envelope's one unit");
        parent
    };

    case.set_clock("2026-09-25T01:00:00Z");
    let mut journal = case.account.open(true).expect("reopen the next day");
    assert_eq!(journal.day(), "2026-09-25", "the new day is the one capacity is drawn against");

    let arms = arm("nested-2");
    let refused = journal
        .reserve(&descendant(&case, &ticket, &arms, Some(&parent)), bounds(80, 200), &audit())
        .expect_err("the envelope is not the day's");
    assert_eq!(refused.reason_code, "ancestor_envelope_exhausted");

    // And the ancestor's own reservation stays stamped with the day it was
    // admitted in: midnight did not revive it into today's ledger.
    let snapshot = journal.snapshot().expect("snapshot");
    assert_eq!(snapshot.reservations[&parent]["window"], "2026-09-24");
    assert_eq!(
        snapshot.invocations,
        Counter::default(),
        "today's window has spent nothing, and yesterday's claims are not today's"
    );
}
