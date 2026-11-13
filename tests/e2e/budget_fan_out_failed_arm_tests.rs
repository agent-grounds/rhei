//! A fanout arm that ends on a failure exit still settles at that exit.
//!
//! Each arm of a fanout settles at its own exit, so an exit that skips the
//! settle leaves that arm's reservation outstanding for a run that has gone,
//! and nothing afterwards settles it. The cases below are the two failure exits
//! a fixture can reach from outside the process: an arm whose command cannot be
//! spawned, which has a start and no record, and an arm stopped at the tooling
//! gate after it was admitted, which never started. The third, a worker that
//! panics, needs a fault no fixture can inject. Each case reads the account
//! once the run has exited, the way `rhei budget show` does.

// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.6.2 §FS-rhei-budgets.4.1

use super::budget_edge_support::account_journal;
use super::budget_fan_out_support::*;
use super::budget_support::*;

/// The report as filed (#672): the arm admitted first cannot be spawned, its
/// sibling runs. The failed arm has a start and no accounting record, so it is
/// charged the reserve as `unmeasurable` at its own exit — while its sibling is
/// still running — and once neither runs, the visit's one travel unit is either
/// converted by an applied edge or given back. Nothing is left outstanding.
// §FS-rhei-budgets.6.2.1
#[test]
fn a_fan_out_arm_whose_spawn_fails_is_charged_the_reserve_at_its_own_exit() {
    let fan = fan_out("budget-fan-out-spawn-fails", [BROKEN, UNRECORDED], "plain");
    let run = fan.run_continuing_on_error();
    assert!(
        run.stderr.contains("failed to spawn agent 'broken'"),
        "the premise is that the broken arm's spawn fails; stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    let reservations = fan.reservations();
    let receipts = account_journal(&fan.dir);
    assert!(
        !receipts_on(&receipts, "start", &reservations["broken"]).is_empty(),
        "the premise is that the broken arm's start was recorded before its spawn; \
         journal:\n{receipts:#?}"
    );

    let unmeasurable = vec![(RESERVE, "unmeasurable".to_owned())];
    assert_eq!(
        spends_on(&receipts, &reservations["broken"]),
        unmeasurable,
        "the arm whose spawn failed is charged the reserve as unmeasurable; journal:\n{receipts:#?}"
    );
    let at_exit = fan.journal_at_slow_exit();
    assert_eq!(
        spends_on(&at_exit, &reservations["broken"]),
        unmeasurable,
        "it is charged at its own exit, while its sibling still runs; journal then:\n{at_exit:#?}"
    );
    assert_eq!(
        spends_on(&receipts, &reservations["plain"]),
        unmeasurable,
        "its sibling, which wrote no record either, is charged the reserve; journal:\n{receipts:#?}"
    );
    assert_settled(&receipts);
    assert_shown(&fan, "project spend: $40.00 consumed + $0.00 outstanding / $400.00");
}

/// An arm admitted and then stopped at the tooling gate has no start, so its
/// whole reservation goes back at that exit. The edge that gate fires is an
/// applied edge, so the travel unit it converted stays converted rather than
/// being given back as well.
// §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.4.1
#[test]
fn a_fan_out_arm_stopped_at_the_tooling_gate_gives_its_reservation_back() {
    let fan = fan_out_without_tooling("budget-fan-out-tooling-gate", [CLAUDE, UNRECORDED]);
    let run = fan.run_continuing_on_error();
    assert!(
        run.stdout.contains("Tooling-unavailable transition: Task plan.1 'review' -> 'blocked'"),
        "the premise is that the gate stops the arms and fires `mcp_unavailable`; \
         stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    let receipts = account_journal(&fan.dir);
    let kinds = |kind: &str| receipts.iter().filter(|receipt| receipt["kind"] == kind).count();
    assert_eq!(
        (kinds("start"), kinds("transition")),
        (0, 1),
        "the premise is that no arm started and one edge was applied; journal:\n{receipts:#?}"
    );

    assert_settled(&receipts);
    assert_shown(&fan, "project spend: $0.00 consumed + $0.00 outstanding / $400.00");
    assert_shown(&fan, "project invocations: 0 consumed + 0 outstanding / ");
}

/// Every reservation the run admitted ended: a started one carries a `spend`,
/// an unstarted one was released, and a travel unit admission held was either
/// converted by an edge or given back.
fn assert_settled(receipts: &[serde_json::Value]) {
    let admitted: Vec<&serde_json::Value> = receipts
        .iter()
        .filter(|receipt| receipt["kind"] == "reserve")
        .flat_map(|receipt| receipt["payload"]["reservations"].as_array().into_iter().flatten())
        .collect();
    let id = |row: &serde_json::Value| row["reservation_id"].as_str().expect("an id").to_owned();
    let released = |rid: &str, travel_only: bool| {
        receipts_on(receipts, "release", rid)
            .into_iter()
            .any(|receipt| (receipt["payload"]["travel_only"] == true) == travel_only)
    };
    let outstanding: Vec<String> = admitted
        .iter()
        .map(|row| id(row))
        .filter(|rid| spends_on(receipts, rid).is_empty() && !released(rid, false))
        .collect();
    assert_eq!(
        outstanding,
        Vec::<String>::new(),
        "no reservation is left with neither a spend nor a release; journal:\n{receipts:#?}"
    );
    let held: Vec<String> = admitted
        .iter()
        .filter(|row| row["travel_units"] == 1)
        .map(|row| id(row))
        .filter(|rid| {
            receipts_on(receipts, "transition", rid).is_empty()
                && !released(rid, true)
                && !released(rid, false)
        })
        .collect();
    assert_eq!(
        held,
        Vec::<String>::new(),
        "the travel unit is converted by an edge or given back; journal:\n{receipts:#?}"
    );
}

/// `rhei budget show`, run after the run has exited, prints `line`.
fn assert_shown(fan: &FanOut, line: &str) {
    let shown = budget_show(&fan.plan, Some(SPEND_DAY));
    assert!(
        shown.stdout.contains(line),
        "the account the run left reads `{line}`; got:\n{}\nstderr:\n{}",
        shown.stdout,
        shown.stderr
    );
}
