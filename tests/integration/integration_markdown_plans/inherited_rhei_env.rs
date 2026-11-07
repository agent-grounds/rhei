/// The integration route of agent-grounds/rhei#635: every `rhei` this harness
/// spawns goes through `rhei_command`, so one test that runs a plan stands for
/// the rest. Under a parent reservation the child cannot resolve, a plan run that
/// inherits it is refused with `nested admission cannot use ancestor`.
/// §REQ-test-isolation.7
#[test]
#[ignore = "red until #635: an inherited RHEI_BUDGET_PARENT_RESERVATION reaches the rhei the integration harness spawns; implement removes this"]
fn an_integration_plan_run_reaches_its_clean_verdict_under_an_inherited_parent_reservation() {
    inherited_rhei_env::assert_verdict_ignores_inherited_rhei_env(
        "run_agent_exit_zero_without_transition_emits_always_snapshots",
        &[inherited_rhei_env::UNKNOWN_PARENT_RESERVATION],
    );
}
