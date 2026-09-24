//! The machine's value is a ceiling, and asking for more than it is not an
//! error.
//!
//! Both halves matter and they pull against each other. If a high request were
//! a refusal, a template written on one laptop would be invalid on the next. If
//! it were honored, a template written elsewhere would raise the cap of the
//! machine that pays for it. The answer is to accept, clamp, and say so.

// §FS-rhei-budgets.2 §FS-rhei-budgets.2.3 §FS-rhei-states.8

use std::path::PathBuf;

use super::budget_support::*;
use super::*;

/// The ping-pong machine with a profile that asks for `<N>` moves.
fn asking_machine(requested: u32) -> String {
    format!(
        r#"name: budget-ceiling
version: 1
states:
  work:
    description: Do a round of work
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: work
        path: runtime/work.md
  review:
    description: Send it back for another round
    agent: mock
    agent_timeout: 30s
    outputs:
      - name: review
        path: runtime/review.md
  cancelled:
    description: Stop
    final: true
profiles:
  greedy:
    initial: work
    allowed: [work, review, cancelled]
    transition_limit: {requested}
node_policy:
  root: greedy
  default: greedy
transitions:
  - {{ from: work, to: review, description: Round done }}
  - {{ from: review, to: work, description: Another round }}
  - {{ from: work, to: cancelled, description: Stop }}
  - {{ from: review, to: cancelled, description: Stop }}
"#
    )
}

fn setup_ceiling(prefix: &str, requested: u32, ceiling: u32) -> (TestDir, PathBuf, PathBuf) {
    setup(prefix, &asking_machine(requested), &format!(r#", "transition_limit": {ceiling}"#))
}

/// Asking above the ceiling is not a validation refusal, and the report says
/// both things a reader needs: who asked, and who limited. Collapsing those two
/// into one source is what makes a clamped bound look like a mystery.
///
/// Validation spawns nothing, so this is the one place the scenario's own
/// numbers — 500 asked, 100 allowed — are cheap to state exactly.
// §FS-rhei-budgets.2.3 §FS-rhei-validate.4
#[test]
fn a_plan_asking_above_the_ceiling_validates_and_reports_both_sources() {
    let (_dir, plan, machine) = setup_ceiling("budget-ceiling-validate", 500, 100);

    let result = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&result);
    assert!(
        result.stdout.contains("Validation succeeded"),
        "a plan that asks for more than the machine allows is valid; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(
            "transition_limit: 100 (requested 500 by the plan, limited by machine settings)"
        ),
        "the clamped bound names the requester and the limiter in one line; got:\n{}",
        result.stdout
    );
}

/// The effective bound is the machine's, whatever the plan asked. A run that
/// honored 500 would be the failure this bound exists to prevent, arriving
/// through a template rather than through a loop.
///
/// The ceiling here is 4 rather than 100 for one reason: each move is a real
/// subprocess, and a hundred of them per test buys no assurance the fourth does
/// not already give.
// §FS-rhei-budgets.2
#[test]
fn the_run_uses_the_machines_value_and_reports_that_it_limited_the_plans() {
    let (dir, plan, machine) = setup_ceiling("budget-ceiling-run", 500, 4);

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(
        &dir,
        4,
        "the ticket gets the machine's four moves rather than the plan's 500",
    );
    assert!(!result.status.success(), "the ticket is halted at the ceiling");
    assert_halt_mentions(&result, "4 (requested 500 by the plan, limited by machine settings)");
    assert_halt_mentions(&result, "set `defaults.transition_limit` in the machine settings file");
}

/// A lower inner value is honored, because the ceiling is a maximum and not a
/// value. A clamp that also raised would make the machine's number the only
/// number anyone could have, and a plan's own restraint would mean nothing.
// §FS-rhei-budgets.2
#[test]
fn a_plan_asking_below_the_ceiling_is_honored_rather_than_raised_to_it() {
    let (dir, plan, machine) = setup_ceiling("budget-ceiling-lower", 3, 100);

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 3, "the plan's lower value wins over the machine's ceiling");
    assert_halt_mentions(&result, "3 (plan)");
}
