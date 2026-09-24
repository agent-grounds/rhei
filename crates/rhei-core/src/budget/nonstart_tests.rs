//! The one lawful release, and the three ways a caller tries to get a second.
//!
//! Reserve-then-settle only bounds anything if the settle is one-way. Every
//! case below is a plausible reason to want an invocation back; exactly one of
//! them is engine-side proof that no process could have started.
//! §FS-rhei-budgets.6.2

use super::test_support::*;
use super::*;

/// A spawn that failed before the start record was written released no
/// capability, so its unit comes back. This is the release, and it is the only
/// one. §FS-rhei-budgets.6.2
#[test]
fn a_spawn_that_failed_before_the_start_record_releases_its_unit() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-1");
    let group = journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 200), &audit())
        .expect("reserve");

    journal.release_unstarted(&group.reservation_ids[0], &audit()).expect("release");

    let snapshot = journal.snapshot().expect("snapshot");
    assert_eq!(snapshot.invocations, Counter::default(), "nothing consumed, nothing outstanding");
    assert_eq!(
        snapshot.travel_for(&ticket),
        Counter::default(),
        "the travel unit it was holding comes back with it"
    );
}

/// Once a start is recorded the unit is consumed, ambiguous or not. A run that
/// was interrupted, timed out, or killed presents no proof of non-start, and
/// letting it claim one would make every crash a refund.
/// §FS-rhei-budgets.6.2
#[test]
fn a_reservation_with_an_ambiguous_start_can_never_be_released() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-1");
    let group = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], false, &audit()).expect("ambiguous start");

    let refused = journal
        .release_unstarted(&group.reservation_ids[0], &audit())
        .expect_err("a possible start is not a non-start");

    assert_eq!(refused.reason_code, "untrustworthy_ledger");
    assert_eq!(journal.snapshot().expect("snapshot").invocations.consumed, 1);
}

/// A confirmed start is the same answer. Confirmation refines the record; it
/// does not create a second window in which a refund is possible.
/// §FS-rhei-budgets.6.2
#[test]
fn a_reservation_with_a_confirmed_start_can_never_be_released() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-1");
    let group = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("confirmed start");

    assert!(journal.release_unstarted(&group.reservation_ids[0], &audit()).is_err());
    assert_eq!(journal.snapshot().expect("snapshot").invocations.consumed, 1);
}

/// A reservation a crashed run left outstanding, with no start record and an
/// execution root whose lock is free, is released by the next admission on the
/// same account. Recovery needs no command and no new lock: it is one step
/// inside a transaction that already holds the account exclusively.
/// §FS-rhei-budgets.6.2
#[test]
fn a_crashed_runs_unstarted_reservation_is_released_by_the_next_admission() {
    let case = Case::new();
    let ticket = case.ticket();
    let abandoned = {
        let mut journal = case.open();
        let arms = arm("attempt-crashed");
        let group = journal
            .reserve(&request(&case, &ticket, &arms, true), bounds(80, 1), &audit())
            .expect("reserve");
        group.reservation_ids[0].clone()
    };
    // The whole day's one unit is outstanding, so the next admission cannot
    // proceed until the dead run's claim is given back.
    {
        let mut journal = case.account.open(true).expect("reopen");
        let arms = arm("attempt-next");
        assert!(
            journal
                .reserve(&request(&case, &ticket, &arms, true), bounds(80, 1), &audit())
                .is_err(),
            "the outstanding claim is real while the run might still be alive"
        );
    }

    let mut journal = case.account.open(true).expect("reopen");
    let released = journal.release_abandoned(&|_| true, &audit()).expect("release abandoned");

    assert_eq!(released, 1);
    assert!(journal.snapshot().expect("snapshot").reservations[&abandoned]["released"] == true);
    let arms = arm("attempt-next");
    journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 1), &audit())
        .expect("the recovered unit is spendable again");
}

/// A run that is still alive keeps its claim. The proof is a lock acquisition,
/// so a run that still holds its execution root proves the opposite of what
/// the release needs. §FS-rhei-budgets.6.2
#[test]
fn a_live_runs_unstarted_reservation_is_left_alone() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-live");
    journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 200), &audit())
        .expect("reserve");

    let released = journal.release_abandoned(&|_| false, &audit()).expect("none abandoned");

    assert_eq!(released, 0);
    assert_eq!(journal.snapshot().expect("snapshot").invocations.reserved, 1);
}
