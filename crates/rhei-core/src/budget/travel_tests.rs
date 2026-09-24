//! What spends a travel unit and what spends an invocation.
//!
//! These two counts bound different things and are easy to conflate, so each
//! case below asserts *both* numbers even when it is about one of them: a retry
//! that quietly spent travel, or a self-loop that quietly spent an invocation,
//! would otherwise pass a test that only looked where it expected the change.
//! §FS-rhei-budgets.4

use super::test_support::*;
use super::*;

/// An edge a person applied by hand: no reservation behind it, so the bound is
/// checked at the charge rather than before the spawn. §FS-rhei-budgets.4.1
fn manual<'a>(ticket: &'a str, from: &'a str, to: &'a str, limit: u64) -> AppliedEdge<'a> {
    AppliedEdge {
        ticket,
        display_id: "plan.1",
        from,
        to,
        reservation: None,
        transition_limit: limit,
    }
}

fn travel_of(journal: &Journal, ticket: &str) -> Counter {
    journal.snapshot().expect("snapshot").travel_for(ticket)
}

fn invocations_of(journal: &Journal) -> Counter {
    journal.snapshot().expect("snapshot").invocations
}

/// The ordinary shape: admit, start, apply the edge. One of each.
/// §FS-rhei-budgets.4.1 §FS-rhei-budgets.4.2
#[test]
fn an_applied_edge_spends_one_travel_unit_and_one_invocation() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-1");

    let group = journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    journal
        .charge_travel(
            &AppliedEdge {
                ticket: &ticket,
                display_id: "plan.1",
                from: "work",
                to: "review",
                reservation: group.travel_reservation_id.as_deref(),
                transition_limit: 80,
            },
            &audit(),
        )
        .expect("charge");

    assert_eq!(travel_of(&journal, &ticket), Counter { consumed: 1, reserved: 0 });
    assert_eq!(invocations_of(&journal), Counter { consumed: 1, reserved: 0 });
}

/// An applied self-loop is an applied edge. It costs exactly what any other
/// edge costs, because the ticket moved and `work -> work` twenty times is the
/// loop the bound exists for. §FS-rhei-transitions.4.3
#[test]
fn an_applied_self_loop_spends_one_travel_unit() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-1");

    let group = journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    journal
        .charge_travel(
            &AppliedEdge {
                ticket: &ticket,
                display_id: "plan.1",
                from: "work",
                to: "work",
                reservation: group.travel_reservation_id.as_deref(),
                transition_limit: 80,
            },
            &audit(),
        )
        .expect("charge");

    assert_eq!(travel_of(&journal, &ticket).consumed, 1);
}

/// A poll wait applies no edge, so it gives the held travel unit back. The
/// invocation is not given back: the process ran.
/// §FS-rhei-budgets.4.1 §FS-rhei-transitions.4.3
#[test]
fn a_poll_wait_releases_its_travel_unit_and_keeps_its_invocation() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = arm("attempt-1");

    let group = journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    journal
        .release_travel(group.travel_reservation_id.as_deref().expect("held"), &audit())
        .expect("release travel");

    assert_eq!(travel_of(&journal, &ticket), Counter { consumed: 0, reserved: 0 });
    assert_eq!(invocations_of(&journal), Counter { consumed: 1, reserved: 0 });
}

/// A person moving a ticket by hand spends its travel like anything else. This
/// is the one place a habit changes, and it is deliberate: a manual path that
/// moved for free would be the bypass. §FS-rhei-budgets.4.1
#[test]
fn a_manual_transition_spends_travel_without_any_reservation() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();

    journal.charge_travel(&manual(&ticket, "work", "review", 80), &audit()).expect("charge");

    assert_eq!(travel_of(&journal, &ticket), Counter { consumed: 1, reserved: 0 });
    assert_eq!(invocations_of(&journal), Counter { consumed: 0, reserved: 0 });
}

/// And it is refused at the bound, because `consumed + outstanding <= bound` is
/// an invariant at every durable boundary rather than a rule the run loop
/// happens to keep. §FS-rhei-budgets.1
#[test]
fn a_manual_transition_is_refused_once_the_travel_bound_is_spent() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    journal.charge_travel(&manual(&ticket, "a", "b", 1), &audit()).expect("first");

    let refused = journal
        .charge_travel(&manual(&ticket, "b", "a", 1), &audit())
        .expect_err("the second move is past the bound");

    assert_eq!(refused.reason_code, "travel_exhausted");
    let spent = refused.exhaustion.expect("a spent bound says so");
    assert_eq!(spent.dimension, Dimension::Travel);
    assert_eq!(spent.counter.consumed, 1);
}

/// A retry is a fresh invocation and the same edge: zero travel, one
/// invocation. The ticket has not moved, and charging it for standing still is
/// how a bound on travel would start bounding effort instead.
/// §FS-rhei-budgets.4.2
#[test]
fn a_retry_spends_one_invocation_and_no_travel() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let first = arm("attempt-1");
    let group = journal
        .reserve(&request(&case, &ticket, &first, true), bounds(80, 200), &audit())
        .expect("reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    journal
        .release_travel(group.travel_reservation_id.as_deref().expect("held"), &audit())
        .expect("the first attempt applied no edge");

    // The retry: a new attempt identity, no travel unit, because the edge the
    // first attempt did not take is still the edge this one is trying for.
    let retry = arm("attempt-2");
    let group = journal
        .reserve(&request(&case, &ticket, &retry, true), bounds(80, 200), &audit())
        .expect("retry reserve");
    journal.record_start(&group.reservation_ids[0], true, &audit()).expect("retry start");

    assert_eq!(invocations_of(&journal).consumed, 2, "both attempts are invocations");
    assert_eq!(travel_of(&journal, &ticket).consumed, 0, "neither applied an edge");
}

/// A fanout is one move of one ticket, started several ways. One travel unit
/// for the edge, one invocation per arm. §FS-rhei-budgets.4.2
#[test]
fn a_fanout_spends_one_travel_unit_and_one_invocation_per_arm() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = [
        Arm { attempt_identity: "arm-a", descendant_envelope: 0 },
        Arm { attempt_identity: "arm-b", descendant_envelope: 0 },
        Arm { attempt_identity: "arm-c", descendant_envelope: 0 },
    ];

    let group = journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 200), &audit())
        .expect("reserve");

    assert_eq!(group.reservation_ids.len(), 3);
    assert_eq!(invocations_of(&journal), Counter { consumed: 0, reserved: 3 });
    assert_eq!(travel_of(&journal, &ticket), Counter { consumed: 0, reserved: 1 });
}

/// All-or-none. A fanout that cannot pay for every arm starts none of them:
/// two of three agents on a question is not the question that was asked, and a
/// partial group would leave the ledger saying it was. §FS-rhei-budgets.4.2
#[test]
fn a_fanout_with_fewer_units_than_arms_reserves_nothing_at_all() {
    let case = Case::new();
    let mut journal = case.open();
    let ticket = case.ticket();
    let arms = [
        Arm { attempt_identity: "arm-a", descendant_envelope: 0 },
        Arm { attempt_identity: "arm-b", descendant_envelope: 0 },
        Arm { attempt_identity: "arm-c", descendant_envelope: 0 },
    ];

    let refused = journal
        .reserve(&request(&case, &ticket, &arms, true), bounds(80, 2), &audit())
        .expect_err("three arms against two units");

    assert_eq!(refused.reason_code, "invocation_exhausted");
    assert_eq!(
        invocations_of(&journal),
        Counter::default(),
        "a refused group appends nothing, so no arm is left half-reserved"
    );
    assert_eq!(travel_of(&journal, &ticket), Counter::default());
}

/// Two writers, one unit. The account is held exclusively, so the loser sees
/// the winner's receipt rather than an empty ledger — which is what makes a
/// second `rhei run` unable to spend the same capacity twice.
/// §FS-rhei-budgets.6.1
#[test]
fn two_transactions_racing_for_one_unit_admit_exactly_one() {
    let case = Case::new();
    let ticket = case.ticket();
    {
        // Bind once up front so the race is about capacity and not about which
        // writer got to install the identity.
        let _ = case.open();
    }
    let root = case.root().to_path_buf();
    let account = case.account.clone();

    let outcomes: Vec<bool> = std::thread::scope(|scope| {
        let handles: Vec<_> = ["racer-a", "racer-b"]
            .into_iter()
            .map(|attempt| {
                let account = account.clone();
                let ticket = ticket.clone();
                let root = root.clone();
                scope.spawn(move || {
                    let mut journal = account.open(true).expect("open");
                    let arms = [Arm { attempt_identity: attempt, descendant_envelope: 0 }];
                    let request = AdmissionRequest {
                        ticket_identity: &ticket,
                        display_id: "plan.1",
                        project_label: "case",
                        execution_root: root.to_str().expect("utf-8"),
                        arms: &arms,
                        parent_reservation: None,
                        travel: false,
                    };
                    journal.reserve(&request, bounds(80, 1), &audit()).is_ok()
                })
            })
            .collect();
        handles.into_iter().map(|handle| handle.join().expect("racer")).collect()
    });

    assert_eq!(
        outcomes.iter().filter(|admitted| **admitted).count(),
        1,
        "exactly one of two racing transactions takes the last unit"
    );
    let journal = case.account.open(false).expect("reopen");
    assert_eq!(invocations_of(&journal).exposure().expect("exposure"), 1);
}
