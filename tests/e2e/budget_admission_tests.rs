//! §FS-rhei-budgets.4 §FS-rhei-budgets.5 §FS-rhei-budgets.9

use super::budget_test_support::*;
use super::*;

fn assert_halts_after_permitted_work(
    case: &BudgetCase,
    expected_spawns: u64,
    expected_state: &str,
    reason: &str,
) {
    let run = run_case(case, &[]);
    let output = combined(&run);
    assert_eq!(
        spawn_count(case),
        expected_spawns,
        "the engine must permit work below the bound and refuse the next spawn; got:\n{output}"
    );
    assert!(!run.status.success(), "a budget-halted non-terminal run exits non-zero");
    assert!(output.contains(reason), "the halt must name its exact bound; got:\n{output}");
    assert_task_state(&case.plan, &case.machine, "1", expected_state);
}

/// Port of the triage cycle: attempts reset on every state visit, so only the
/// persistent project invocation allowance can stop spawn three.
#[test]
fn invocation_allowance_halts_before_the_next_forbidden_spawn() {
    let case = cycle_case(
        "budget-invocation-cycle",
        Limits {
            invocations: 2,
            spend_micro: 100_000,
            transition_limit: 20,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    assert_halts_after_permitted_work(
        &case,
        2,
        "a",
        "Panta invocation allowance exhausted (2 consumed + 0 reserved / 2)",
    );
}

/// Invocation and money have ample headroom, so the third cross-state move can
/// be refused only by lifetime ticket travel.
#[test]
fn ticket_travel_halts_before_the_next_forbidden_spawn() {
    let case = cycle_case(
        "budget-travel-cycle",
        Limits {
            invocations: 20,
            spend_micro: 100_000,
            transition_limit: 2,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    assert_halts_after_permitted_work(
        &case,
        2,
        "a",
        "ticket travel exhausted (2 applied + 0 reserved / 2)",
    );
}

/// Two fixed-cost fixture calls fit exactly; invocation and travel limits are
/// deliberately high so neither can conceal missing monetary enforcement.
#[test]
fn provider_spend_halts_before_the_next_forbidden_spawn() {
    let case = cycle_case(
        "budget-spend-cycle",
        Limits {
            invocations: 20,
            spend_micro: 4_000,
            transition_limit: 20,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    assert_halts_after_permitted_work(
        &case,
        2,
        "a",
        "Panta spend allowance exhausted (4000 consumed + 0 reserved / 4000 micro-USD)",
    );
}

/// Two scheduler slots race for the last invocation. Exactly one may own the
/// durable reservation; a check without an atomic reservation starts both.
#[test]
fn competing_parallel_admissions_cannot_double_spend_the_last_unit() {
    let case = parallel_case("budget-parallel-atomic", false);
    let run = run_case(&case, &["--parallel", "2"]);
    let output = combined(&run);
    assert_eq!(spawn_count(&case), 1, "only one competitor may start; got:\n{output}");
    assert!(!run.status.success(), "the other non-terminal ticket is budget-halted");
    assert!(output.contains("Panta invocation allowance exhausted"));
}

/// A two-arm fanout cannot start just one reviewer when only one invocation
/// remains: reserving every arm is one all-or-none transaction.
#[test]
fn fanout_reserves_every_arm_or_starts_none() {
    let case = parallel_case("budget-fanout-atomic", true);
    let run = run_case(&case, &["--parallel", "2"]);
    let output = combined(&run);
    assert_eq!(spawn_count(&case), 0, "partial fanout admission is forbidden; got:\n{output}");
    assert!(!run.status.success());
    assert!(output.contains("fanout requires 2 invocation units; only 1 remains"));
    assert_task_state(&case.plan, &case.machine, "1", "work");
}
