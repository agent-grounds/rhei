//! A plan the end-to-end harness runs is admitted under no reservation but its
//! own, whatever `RHEI_*` the process that started `cargo test` exported.
//! §REQ-test-isolation.7

#[cfg(unix)]
use super::inherited_rhei_env::{
    assert_verdict_ignores_inherited_rhei_env, UNKNOWN_PARENT_RESERVATION,
};

/// The end-to-end route of agent-grounds/rhei#635: every `rhei` this harness
/// spawns goes through `rhei_process_at`, so one test that runs a plan stands
/// for the rest. Under a parent reservation the child cannot resolve, a plan
/// run that inherits it is refused with `nested admission cannot use ancestor`
/// (§FS-rhei-budgets.7.1) — a refusal that is right, and is not the test's.
///
/// Unix only, like the test it reruns: on Windows that test is not compiled,
/// so the rerun would match nothing and the clean control could never pass.
#[cfg(unix)]
#[test]
fn an_e2e_plan_run_reaches_its_clean_verdict_under_an_inherited_parent_reservation() {
    assert_verdict_ignores_inherited_rhei_env(
        "accounting_line_tests::one_accounting_line_carries_the_invocations_final_cost",
        &[UNKNOWN_PARENT_RESERVATION],
    );
}
