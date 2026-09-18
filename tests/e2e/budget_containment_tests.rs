//! §FS-rhei-budgets.6.3: each missing containment obligation refuses before
//! the fixture process can start.

use super::budget_test_support::*;

fn assert_missing_obligation_refuses(obligations: [bool; 4], reason: &str, prefix: &str) {
    let case = cycle_case(
        prefix,
        Limits {
            invocations: 10,
            spend_micro: 100_000,
            transition_limit: 10,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        0,
    );
    write_fixture_qualification(&case.root, obligations);

    let run = run_case(&case, &[]);
    let output = combined(&run);
    assert_eq!(spawn_count(&case), 0, "unqualified work must not start; got:\n{output}");
    assert!(output.contains(reason), "refusal must name the missing proof; got:\n{output}");
}

#[test]
fn missing_capture_barrier_refuses_before_spawn() {
    assert_missing_obligation_refuses(
        [false, true, true, true],
        "qualification missing capture latency or durable capture barrier",
        "budget-no-capture-proof",
    );
}

#[test]
fn missing_committed_and_in_flight_maximum_refuses_before_spawn() {
    assert_missing_obligation_refuses(
        [true, false, true, true],
        "qualification missing committed/in-flight request maximum",
        "budget-no-request-proof",
    );
}

#[test]
fn missing_post_kill_exposure_refuses_before_spawn() {
    assert_missing_obligation_refuses(
        [true, true, false, true],
        "qualification missing post-kill provider exposure",
        "budget-no-post-kill-proof",
    );
}

#[test]
fn missing_nested_and_fallback_bound_refuses_before_spawn() {
    assert_missing_obligation_refuses(
        [true, true, true, false],
        "qualification missing nested/fallback exposure bound",
        "budget-no-nested-proof",
    );
}
