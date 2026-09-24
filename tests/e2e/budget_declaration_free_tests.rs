//! A plan and a machine that declare nothing about budgets still validate, still
//! run, and still say what they are bounded by.
//!
//! This is the compatibility promise stated as a test: nobody who wrote nothing
//! is refused, and nobody who wrote nothing is left guessing either.

// §REQ-bounded-neural-work.2 §FS-rhei-budgets.2 §FS-rhei-validate.4

use super::budget_support::*;
use super::*;

/// Every plan has all three bounds in force, so every successful validation
/// reports all three. A machine that has configured nothing reports `built_in`
/// three times — which is the honest answer, and is why there is no
/// unconfigured state in which a count dimension is unbounded.
// §FS-rhei-validate.4 §FS-rhei-budgets.2.1
#[test]
fn validate_reports_all_three_bounds_with_their_built_in_source() {
    let (_dir, plan, machine) = setup_with_agent(
        "budget-declaration-free-validate",
        FINISHING_MACHINE,
        FINISHING_AGENT,
        "",
    );

    let result = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&result);
    assert!(
        result.stdout.contains("Validation succeeded"),
        "a plan that declares no bound is valid; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("transition_limit: 80 (built_in)"),
        "the travel bound is reported with its built-in value and source; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("invocations_per_day: 200 (built_in)"),
        "the window bound is reported with its built-in value and source; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("invocation_lifetime_max: 6000 (built_in)"),
        "the lifetime ceiling is reported with its built-in value and source; got:\n{}",
        result.stdout
    );
}

/// The run itself is unchanged for a plan under the defaults: one spawn, one
/// move, a terminal state, exit zero. A first admission establishes the account
/// silently — no command, no prompt, and nothing in the way.
// §FS-rhei-budgets.5.4
#[test]
fn a_plan_with_no_budget_configuration_runs_to_completion_under_the_defaults() {
    let (dir, plan, machine) =
        setup_with_agent("budget-declaration-free-run", FINISHING_MACHINE, FINISHING_AGENT, "");

    let result = run_plan(&plan, &machine, None);

    assert_success(&result);
    assert_task_state(&plan, &machine, "1", "completed");
    assert_eq!(spawns(&dir), ["work"], "the default bounds are far above one spawn");
}

/// Establishing the account is part of the first admission rather than a
/// migration step. A workspace that has never seen `rhei budget` has an account
/// afterwards, and having one is not something anyone had to do.
// §FS-rhei-budgets.5.1 §FS-rhei-budgets.5.4
#[test]
fn the_first_admission_establishes_the_account_without_a_command() {
    let (dir, plan, machine) =
        setup_with_agent("budget-declaration-free-account", FINISHING_MACHINE, FINISHING_AGENT, "");

    assert!(
        !dir.join(".agent-grounds/rhei/budgets").exists(),
        "no account exists before the first run"
    );
    assert_success(&run_plan(&plan, &machine, None));

    assert!(
        dir.join(".agent-grounds/rhei/budgets").exists(),
        "the first admission mints the account under the project root, outside runtime/"
    );
}
