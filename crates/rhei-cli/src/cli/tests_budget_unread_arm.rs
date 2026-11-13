// A worker's message that nothing else reads still ends the arm it names.
//
// The pool's drop drains what its loop never read, and an unread message is an
// arm's exit all the same: a worker that panicked, or a sibling's completion
// left behind when the pool stopped early. Each is settled there, from its own
// arm alone. What that settle writes to the account is the e2e suite's to show;
// these read the claim map, which decides which arm it writes for.

// §FS-rhei-budgets.6.2.1

/// A claim under `ticket` whose arms have all started, as admission and the
/// starts leave a fanout's claim while its workers run.
fn hold_started_arms(ticket: &str, reservations: &[&str]) {
    let arms = reservations
        .iter()
        .map(|reservation| HeldArm {
            reservation: (*reservation).to_owned(),
            started: true,
            measured: None,
        })
        .collect();
    let claim = HeldClaim {
        travel: reservations.first().map(|reservation| (*reservation).to_owned()),
        arms,
        account: "unit-test-account".to_owned(),
        ticket: format!("ticket:{ticket}"),
        bounds: resolve_count_bounds(&RheiSettings::default(), None),
    };
    with_claims(|claims| claims.insert(ticket.to_owned(), claim));
}

/// The arms `ticket`'s claim still holds, or `None` once the claim has ended.
fn held_arms(ticket: &str) -> Option<Vec<String>> {
    with_claims(|claims| {
        let claim = claims.get(ticket)?;
        Some(claim.arms.iter().map(|arm| arm.reservation.clone()).collect())
    })
}

/// A worker's panic, as its thread reports it.
fn panicked(ticket: &str, budget_arm: Option<&str>) -> ParallelAgentThreadMessage {
    ParallelAgentThreadMessage::Panicked {
        task_id_str: ticket.to_owned(),
        state_name: "review".to_owned(),
        slot: 0,
        budget_arm: budget_arm.map(str::to_owned),
    }
}

/// The claim map is process-global, so each case keys its own ticket.
fn unread_ticket(case: &str) -> String {
    format!("unread-arm-{case}-{}", uuid::Uuid::new_v4())
}

/// An unread panic ends the arm its worker was admitted on and no other: the
/// sibling still running stays held, so its own exit settles it later; and
/// once that sibling is read too, the claim ends with its last arm.
/// §FS-rhei-budgets.6.2.1
#[test]
fn an_unread_panic_settles_its_own_arm_and_leaves_a_running_sibling_held() {
    let ticket = unread_ticket("panic");
    hold_started_arms(&ticket, &["reservation:panicked", "reservation:running"]);
    let dir = tempfile::tempdir().expect("tmpdir");
    let sink: Arc<dyn rhei_tui::EventSink> = Arc::new(rhei_tui::NullSink);

    settle_unread_arm(dir.path(), panicked(&ticket, Some("reservation:panicked")), &sink);
    assert_eq!(
        held_arms(&ticket),
        Some(vec!["reservation:running".to_owned()]),
        "the panicked arm settled at its own exit, and only it"
    );

    settle_unread_arm(dir.path(), panicked(&ticket, Some("reservation:running")), &sink);
    assert_eq!(held_arms(&ticket), None, "the claim ends with its last live arm");
}

/// A program's worker is admitted on no arm, so its panic settles nothing, and
/// an agent arm of the same ticket stays held for its own exit.
/// §FS-rhei-budgets.6.2.1
#[test]
fn an_unread_panic_naming_no_arm_settles_nothing() {
    let ticket = unread_ticket("program");
    hold_started_arms(&ticket, &["reservation:agent"]);
    let dir = tempfile::tempdir().expect("tmpdir");
    let sink: Arc<dyn rhei_tui::EventSink> = Arc::new(rhei_tui::NullSink);

    settle_unread_arm(dir.path(), panicked(&ticket, None), &sink);
    let held = held_arms(&ticket);
    with_claims(|claims| claims.remove(&ticket));
    assert_eq!(held, Some(vec!["reservation:agent".to_owned()]));
}
