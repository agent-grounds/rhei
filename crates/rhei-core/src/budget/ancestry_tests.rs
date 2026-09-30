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
    parent: Option<AncestryDescriptor<'a>>,
) -> AdmissionRequest<'a> {
    AdmissionRequest { parent_reservation: parent, ..request(case, ticket, arms, false) }
}

/// A descriptor as a caller too old to name its account sends one: the
/// reservation alone, which is taken as this project's. §FS-rhei-budgets.7.1
fn unscoped(reservation: &str) -> Option<AncestryDescriptor<'_>> {
    Some(AncestryDescriptor { reservation, account: None, origin: None })
}

/// A descriptor that says whose reservation it is. §FS-rhei-budgets.7.1
fn minted_by<'a>(reservation: &'a str, account: &'a str) -> Option<AncestryDescriptor<'a>> {
    Some(AncestryDescriptor { reservation, account: Some(account), origin: None })
}

/// An account uuid no `Case` can have, so a descriptor carrying it is somebody
/// else's by construction rather than by a coincidence of the fixture.
const ANOTHER_ACCOUNT: &str = "99999999-9999-4999-8999-999999999999";

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
            &descendant(&case, &ticket, &arms, unscoped("reservation:absent")),
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
        .reserve(&descendant(&case, &ticket, &arms, unscoped(&parent)), bounds(80, 200), &audit())
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
        .reserve(&descendant(&case, &ticket, &nested, unscoped(&parent)), bounds(80, 200), &audit())
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
            .reserve(
                &descendant(&case, &ticket, &arms, unscoped(&parent)),
                bounds(80, 200),
                &audit(),
            )
            .expect("inside the envelope");
        journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    }

    let after = journal.snapshot().expect("snapshot").invocations.exposure().expect("exposure");
    assert_eq!(after, before + 2, "each descendant charges the shared account exactly once");

    let arms = arm("nested-3");
    let refused = journal
        .reserve(&descendant(&case, &ticket, &arms, unscoped(&parent)), bounds(80, 200), &audit())
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
            .reserve(
                &descendant(&case, &ticket, &arms, unscoped(&parent)),
                bounds(80, 200),
                &audit(),
            )
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
            .reserve(
                &descendant(&case, &ticket, &arms, unscoped(&parent)),
                bounds(80, 200),
                &audit(),
            )
            .expect("the envelope's one unit");
        parent
    };

    case.set_clock("2026-09-25T01:00:00Z");
    let mut journal = case.account.open(true).expect("reopen the next day");
    assert_eq!(journal.day(), "2026-09-25", "the new day is the one capacity is drawn against");

    let arms = arm("nested-2");
    let refused = journal
        .reserve(&descendant(&case, &ticket, &arms, unscoped(&parent)), bounds(80, 200), &audit())
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

/// A descriptor minted for another account is **not** a dead ancestor, and the
/// refusal is the wrong answer to it: the caller is opening a balance in a
/// ledger the minting project has no claim on, so there is nothing to forge and
/// no envelope to escape. It is admitted unparented.
/// §FS-rhei-budgets.7.2
///
/// Fails before the fix: today the name is looked up in this journal, is not
/// found, and the admission is refused `no such reservation`.
#[test]
#[ignore = "pins agent-grounds/rhei#354 and fails until it is fixed; \
    the fix removes this attribute. Run with `cargo test -- --ignored`."]
fn a_descriptor_minted_for_another_account_opens_its_own_balance() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("nested");

    let group = journal
        .reserve(
            &descendant(&case, &ticket, &arms, minted_by("reservation:elsewhere", ANOTHER_ACCOUNT)),
            bounds(80, 200),
            &audit(),
        )
        .expect("another project's descriptor names no ancestor here, so nothing refuses it");

    let snapshot = journal.snapshot().expect("snapshot");
    assert_eq!(
        snapshot.reservations[&group.reservation_ids[0]]["parent_reservation"],
        serde_json::Value::Null,
        "a downgraded admission must record no parent, or replay reads the chain as corrupt"
    );
}

/// The same rule where the name *does* resolve, which is the case replay
/// depends on. A reservation id is a uuid and two accounts will not collide in
/// practice, but the receipt must record what was **decided** rather than what
/// was asked: an admission the identity test downgraded may not name a parent
/// merely because the journal happens to hold that name, and it may not draw
/// against that ancestor's envelope either. §FS-rhei-budgets.7.2
///
/// Fails before the fix: today the descriptor is placed under the live ancestor,
/// so the receipt names it and the envelope of one is spent.
#[test]
#[ignore = "pins agent-grounds/rhei#354 and fails until it is fixed; \
    the fix removes this attribute. Run with `cargo test -- --ignored`."]
fn a_descriptor_minted_for_another_account_draws_no_envelope_here() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let parent = live_ancestor(&case, &mut journal, 1);

    let mut unparented = Vec::new();
    for attempt in ["nested-1", "nested-2"] {
        let arms = arm(attempt);
        let group = journal
            .reserve(
                &descendant(&case, &ticket, &arms, minted_by(&parent, ANOTHER_ACCOUNT)),
                bounds(80, 200),
                &audit(),
            )
            .expect("neither child is under that ancestor, so its envelope of one bounds neither");
        unparented.push(group.reservation_ids[0].clone());
    }

    let snapshot = journal.snapshot().expect("snapshot");
    for reservation in &unparented {
        assert_eq!(
            snapshot.reservations[reservation]["parent_reservation"],
            serde_json::Value::Null,
            "the receipt records what was decided, not what was asked"
        );
    }
}

/// The other half of the contract, and the half that must not move: inside
/// **one** account a name the journal does not hold still buys nothing.
/// §FS-rhei-budgets.7.1
///
/// A guard: this passes before the fix and must keep passing after it.
#[test]
fn a_descriptor_of_this_account_naming_nothing_is_still_refused() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("nested");
    let own = case.account.uuid().to_owned();

    let refused = journal
        .reserve(
            &descendant(&case, &ticket, &arms, minted_by("reservation:absent", &own)),
            bounds(80, 200),
            &audit(),
        )
        .expect_err("forging a name in one's own ledger buys nothing");

    assert_eq!(refused.reason_code, "ancestor_unavailable");
    assert!(refused.message.contains("no such reservation"), "{}", refused.message);
}

/// The released, unstarted and envelope-less arms of the same rule, each with
/// the account named, so that scoping the descriptor does not quietly take the
/// other three refusals with it. §FS-rhei-budgets.7.1
///
/// A guard: this passes before the fix and must keep passing after it.
#[test]
fn a_descriptor_of_this_account_keeps_every_other_refusal() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let own = case.account.uuid().to_owned();

    // Reserved and then given back as never-started, which is the one lawful
    // release. §FS-rhei-budgets.6.2
    let arms = [Arm { attempt_identity: "released", descendant_envelope: 4 }];
    let released = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve the ancestor to be released")
        .reservation_ids[0]
        .clone();
    journal.release_unstarted(&released, &audit()).expect("release it");

    // Reserved and never started at all.
    let arms = [Arm { attempt_identity: "unstarted", descendant_envelope: 4 }];
    let unstarted = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve the unstarted ancestor")
        .reservation_ids[0]
        .clone();

    let envelope_less = live_ancestor(&case, &mut journal, 0);

    for (parent, why) in [
        (&released, "has already been released"),
        (&unstarted, "has not started"),
        (&envelope_less, "permits no nested neural work"),
    ] {
        let arms = arm("nested");
        let refused = journal
            .reserve(
                &descendant(&case, &ticket, &arms, minted_by(parent, &own)),
                bounds(80, 200),
                &audit(),
            )
            .expect_err("an ancestor of this account that is not outstanding lends nothing");
        assert_eq!(refused.reason_code, "ancestor_unavailable", "{}", refused.message);
        assert!(refused.message.contains(why), "expected {why:?} in: {}", refused.message);
    }
}

/// Provenance is the caller's to supply and this layer's only to interpolate:
/// the refusal says where the value came from without admission learning that
/// any caller reads an environment. §FS-rhei-budgets.7.1
///
/// Fails before the fix: the phrase is nowhere in the message.
#[test]
#[ignore = "pins agent-grounds/rhei#354 and fails until it is fixed; \
    the fix removes this attribute. Run with `cargo test -- --ignored`."]
fn a_refusal_names_the_origin_its_caller_supplied() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("nested");
    let own = case.account.uuid().to_owned();
    let descriptor = Some(AncestryDescriptor {
        reservation: "reservation:absent",
        account: Some(&own),
        origin: Some("from RHEI_BUDGET_PARENT_RESERVATION"),
    });

    let refused = journal
        .reserve(&descendant(&case, &ticket, &arms, descriptor), bounds(80, 200), &audit())
        .expect_err("the name still buys nothing");

    assert!(
        refused.message.contains("from RHEI_BUDGET_PARENT_RESERVATION"),
        "a refusal that names only the value sends a reader to the account directory: {}",
        refused.message
    );
}

/// And a caller that supplies none leaves the message byte-identical to the one
/// this ledger has always printed. §FS-rhei-budgets.7.1
///
/// A guard: this passes before the fix and must keep passing after it.
#[test]
fn a_refusal_with_no_origin_reads_exactly_as_it_always_has() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("nested");

    let refused = journal
        .reserve(
            &descendant(&case, &ticket, &arms, unscoped("reservation:absent")),
            bounds(80, 200),
            &audit(),
        )
        .expect_err("the name buys nothing");

    assert_eq!(
        refused.message,
        "nested admission cannot use ancestor reservation:absent: no such reservation"
    );
}
