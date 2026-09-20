//! Nested and embedded work is placed under a live ancestor of this very
//! project, or it is refused. An ancestry token names a reservation; the
//! ledger decides whether that name buys anything.
//! §FS-rhei-budgets.5 §AR-neural-admission.7

use super::evidence_tests::{audit, Case};
use crate::budget::*;

/// One descendant arm against its ancestor, worth `threshold` plus the
/// qualification's residual. A descendant of its own is never permitted here.
fn arm<'a>(
    attempt: &'a str,
    threshold: &'a Money,
    qualification: &'a QualifiedLaunch,
) -> [Arm<'a>; 1] {
    [Arm { attempt_identity: attempt, threshold, qualification, descendant_envelope_micro: 0 }]
}

fn nested<'a>(
    ticket: &'a str,
    parent: Option<&'a str>,
    arms: &'a [Arm<'a>],
    deadline_unix: u64,
) -> AdmissionRequest<'a> {
    AdmissionRequest {
        ticket_identity: ticket,
        transition_limit: 3,
        deadline_unix,
        arms,
        parent_reservation: parent,
    }
}

fn money(amount_micro: u64) -> Money {
    Money { currency: "USD".into(), amount_micro }
}

/// The finite deadline the ancestor was admitted with; a descendant's own
/// deadline may reach it and no further. §FS-rhei-budgets.5
fn ancestor_deadline(case: &Case, parent: &str) -> u64 {
    case.journal.state.reservations[parent].payload["deadline_unix"].as_u64().unwrap()
}

/// A nested runtime that names an ancestor this ledger does not hold gets a
/// typed refusal rather than a balance of its own — the laundering route the
/// ancestry rule closes. §FS-rhei-budgets.5
#[test]
fn a_descendant_of_an_unknown_ancestor_cannot_open_an_allowance() {
    let mut case = Case::new();
    let ticket = case.journal.state.reservations[&case.reservation].ticket.clone();
    let threshold = money(1000);
    let deadline = ancestor_deadline(&case, &case.reservation.clone());
    let arms = arm("attempt-N", &threshold, &case.qualification);
    let request = nested(&ticket, Some("reservation:absent"), &arms, deadline);

    let refused = case.journal.reserve(&request, &audit()).unwrap_err();

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("no such reservation"), "{}", refused.message);
}

/// The initial contained qualification disables nested work, so an ancestor
/// admitted under it lends nothing even though it is live.
/// §FS-rhei-budgets.6.3
#[test]
fn an_ancestor_whose_qualification_forbids_nesting_lends_nothing() {
    let mut case = Case::new();
    let parent = case.reservation.clone();
    let ticket = case.journal.state.reservations[&parent].ticket.clone();
    let threshold = money(1000);
    let deadline = ancestor_deadline(&case, &parent);
    let arms = arm("attempt-N", &threshold, &case.qualification);
    let request = nested(&ticket, Some(&parent), &arms, deadline);

    let refused = case.journal.reserve(&request, &audit()).unwrap_err();

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("permits no nested neural work"), "{}", refused.message);
}

/// With a proven delegation graph the descendant is an ordinary ledger entry:
/// it consumes its own invocation unit and its own money, and what it may draw
/// is bounded by the envelope its ancestor reserved. §AR-neural-admission.7
#[test]
fn a_descendant_fits_inside_its_ancestors_envelope_and_no_further() {
    let mut case = Case::with_descendant_envelope(4000);
    let parent = case.reservation.clone();
    let ticket = case.journal.state.reservations[&parent].ticket.clone();
    let before = case.journal.snapshot().unwrap();
    let threshold = money(1000);
    let deadline = ancestor_deadline(&case, &parent);

    // 1000 threshold + 2000 residual = 3000, inside the 4000 envelope.
    let first_arms = arm("attempt-N1", &threshold, &case.qualification);
    let first = nested(&ticket, Some(&parent), &first_arms, deadline);
    let group = case.journal.reserve(&first, &audit()).unwrap();
    case.journal.record_start(&group.reservation_ids[0], false, &audit()).unwrap();

    let after = case.journal.snapshot().unwrap();
    assert_eq!(after.consumed.invocations, before.consumed.invocations + 1);
    assert_eq!(
        after.reserved.spend.amount_micro,
        before.reserved.spend.amount_micro + 3000,
        "a descendant adds its own exposure and never its ancestor's a second time"
    );

    // A second one would draw 6000 against a 4000 envelope.
    let second_arms = arm("attempt-N2", &threshold, &case.qualification);
    let second = nested(&ticket, Some(&parent), &second_arms, deadline);
    let refused = case.journal.reserve(&second, &audit()).unwrap_err();
    assert_eq!(refused.reason_code, "ancestor_envelope_exhausted");
    assert!(refused.message.contains("4000"), "{}", refused.message);
}

/// A descendant may not outlive the invocation that authorized it: its
/// deadline is its ancestor's ceiling. §FS-rhei-budgets.5
#[test]
fn a_descendant_cannot_outlive_its_ancestors_deadline() {
    let mut case = Case::with_descendant_envelope(4000);
    let parent = case.reservation.clone();
    let ticket = case.journal.state.reservations[&parent].ticket.clone();
    let threshold = money(1000);
    let arms = arm("attempt-N", &threshold, &case.qualification);
    let request = nested(&ticket, Some(&parent), &arms, ancestor_deadline(&case, &parent) + 1);

    let refused = case.journal.reserve(&request, &audit()).unwrap_err();

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("outlive"), "{}", refused.message);
}

/// The whole chain is re-derived on every open, so a receipt naming an
/// ancestor that is absent or overdrawn is corruption rather than capacity.
/// §FS-rhei-budgets.3.1
#[test]
fn a_replayed_chain_re_derives_every_ancestry_claim() {
    let mut case = Case::with_descendant_envelope(4000);
    let parent = case.reservation.clone();
    let ticket = case.journal.state.reservations[&parent].ticket.clone();
    let threshold = money(1000);
    let deadline = ancestor_deadline(&case, &parent);
    let arms = arm("attempt-N1", &threshold, &case.qualification);
    let request = nested(&ticket, Some(&parent), &arms, deadline);
    case.journal.reserve(&request, &audit()).unwrap();

    let replayed = case.replayed();
    let row = replayed
        .reservations
        .values()
        .find(|row| row.payload["parent_reservation"].as_str() == Some(parent.as_str()))
        .expect("the descendant survives replay");
    assert_eq!(row.fwc, 3000);
    assert_eq!(replayed.reservations.len(), 2, "ancestor and descendant are both ledger entries");
}
