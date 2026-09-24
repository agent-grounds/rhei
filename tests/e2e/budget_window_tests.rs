//! The window: a project's invocation capacity is a rate, not a total, and the
//! only thing that renews it is the passage of the day.
//!
//! A finite lifetime default would eventually stop every healthy long-lived
//! project and ask an operator to top it up, which is a block by another route.
//! A window stops a project that is spinning and never meets one that is
//! working — so long as it cannot be renewed by anything but time.

// §FS-rhei-budgets.3.1 §FS-rhei-budgets.3.3 §REQ-bounded-neural-work.3

use super::budget_support::*;

const DAY_ONE_NOON: &str = "2026-09-24T12:00:00Z";
const DAY_ONE_LATER: &str = "2026-09-24T23:59:00Z";
const DAY_TWO_EARLY: &str = "2026-09-25T01:00:00Z";
const DAY_ONE_RENEWAL: &str = "2026-09-25T00:00:00Z";

/// Two invocations a day buys two spawns, and the third is refused. The halt
/// names the renewal instant rather than a settings key, because nothing an
/// operator does is needed: sending them to a file for a wait is the wrong
/// remedy even when it would work.
// §FS-rhei-budgets.8
#[test]
fn a_project_that_spends_the_day_halts_and_names_when_the_window_renews() {
    let (dir, plan, machine) =
        setup("budget-window-spent", PING_PONG_MACHINE, r#", "invocations_per_day": 2"#);

    let result = run_plan(&plan, &machine, Some(DAY_ONE_NOON));

    assert_eq!(spawns(&dir).len(), 2, "two units buys two spawns; spawns={:?}", spawns(&dir));
    assert!(!result.status.success(), "a run halted on the window exits non-zero");
    assert_halt_mentions(&result, "project invocations");
    assert_halt_mentions(&result, "2 (machine)");
    assert_halt_mentions(&result, "window (2026-09-24Z)");
    assert_halt_mentions(&result, DAY_ONE_RENEWAL);
}

/// Nothing inside the day renews it. A second `rhei run` is the operator's most
/// natural next move and the one that must buy nothing: "once per run, forever"
/// is the unbounded case the bound exists for.
// §REQ-bounded-neural-work.4
#[test]
fn a_second_run_inside_the_same_day_renews_nothing() {
    let (dir, plan, machine) =
        setup("budget-window-same-day", PING_PONG_MACHINE, r#", "invocations_per_day": 2"#);

    run_plan(&plan, &machine, Some(DAY_ONE_NOON));
    assert_eq!(spawns(&dir).len(), 2);

    let again = run_plan(&plan, &machine, Some(DAY_ONE_LATER));

    assert_eq!(
        spawns(&dir).len(),
        2,
        "a fresh run later the same day spends nothing further; spawns={:?}",
        spawns(&dir)
    );
    assert!(!again.status.success());
    assert_halt_mentions(&again, "project invocations");
}

/// Past midnight the same plan proceeds. The day is a key rather than a refill:
/// nothing was granted, the new day simply has capacity of its own.
// §FS-rhei-budgets.3.3
#[test]
fn the_same_plan_proceeds_once_the_clock_is_past_the_window_boundary() {
    let (dir, plan, machine) =
        setup("budget-window-next-day", PING_PONG_MACHINE, r#", "invocations_per_day": 2"#);

    run_plan(&plan, &machine, Some(DAY_ONE_NOON));
    assert_eq!(spawns(&dir).len(), 2);

    run_plan(&plan, &machine, Some(DAY_TWO_EARLY));

    assert_eq!(
        spawns(&dir).len(),
        4,
        "the next UTC day has its own two units; spawns={:?}",
        spawns(&dir)
    );
}

/// A clock moved backwards mints nothing. The highest day key the journal has
/// ever recorded is the floor, so reaching for yesterday keeps drawing on the
/// day that was already spent.
// §FS-rhei-budgets.3.3
#[test]
fn a_clock_moved_backwards_keeps_drawing_on_the_day_already_spent() {
    let (dir, plan, machine) =
        setup("budget-window-backwards", PING_PONG_MACHINE, r#", "invocations_per_day": 2"#);

    run_plan(&plan, &machine, Some(DAY_TWO_EARLY));
    assert_eq!(spawns(&dir).len(), 2);

    let backwards = run_plan(&plan, &machine, Some(DAY_ONE_NOON));

    assert_eq!(spawns(&dir).len(), 2, "an earlier day is not a new day; spawns={:?}", spawns(&dir));
    assert!(!backwards.status.success(), "the run is halted, not quietly re-funded");
    assert_halt_mentions(&backwards, "window (2026-09-25Z)");
}
