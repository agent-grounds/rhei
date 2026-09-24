//! What a selected run *decides*: which stop it reports, and what defeats it.
//!
//! Idle is a property of the whole in-scope plan. One ticket whose classified
//! blocker is actionable defeats it however many tickets wait beside it, which
//! is what keeps a real problem from hiding under a waiting result.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1 §FS-rhei-run-json.5

use super::until_idle_support::*;
use super::*;

/// (1) Gate-only. The one open ticket is waiting on a person, so the run has
/// nothing to do and nothing to complain about: it returns `3`, names the
/// gate, reports no timed retry at all, leaves `## Attention` empty, and does
/// not move the ticket.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn until_idle_returns_three_on_a_gate_only_plan() {
    let plan = IdlePlan::new(
        "until-idle-gate-only",
        "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "a gate-only plan is idle, not complete and not halted");
    assert_idle_line(&result, "gate", "none");
    let report = plan.report();
    assert!(
        report.contains("idle — waiting on a person"),
        "the report's result gains the idle phrase; got:\n{report}"
    );
    assert!(
        !report.contains("## Attention"),
        "a non-empty Attention means the run was not idle; got:\n{report}"
    );
    assert!(report.contains("## Waiting"), "the gated ticket belongs in Waiting; got:\n{report}");
    assert_task_state(&plan.plan, &plan.machine, "1", "gate");
}

/// (3) Provider-limit only. This is the case that catches the divergence: the
/// plan-wide deliberate-wait judgment never tested provider limits while the
/// deadline scan did, so the one wait the contract calls calm would have been
/// reported as attention and exited in the failure category. It exits `3`, not
/// `1`, and spends no attempt doing it.
// §FS-rhei-run.3.3 §FS-rhei-run-report.3.1
#[test]
fn until_idle_reports_a_provider_limit_only_wait_as_idle_rather_than_failure() {
    let plan = IdlePlan::new(
        "until-idle-provider-only",
        &format!(
            r#"---
metadata:
  tasks:
    1:
      {}
---

## Tasks

### Task 1: Parked on a provider limit
**State:** parked
"#,
            provider_limit("parked", FUTURE_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(
        &result,
        EXIT_IDLE,
        "a recognized provider limit is a deliberate wait, not a failure",
    );
    assert_idle_line(&result, "provider_limit", &instant(FUTURE_POLL));
    assert!(
        plan.records().is_empty(),
        "the attempt budget is preserved: nothing was spawned while the limit holds"
    );
    assert!(
        !plan.plan_text().contains("stateVisits"),
        "no visit was spent; got:\n{}",
        plan.plan_text()
    );
    assert_task_state(&plan.plan, &plan.machine, "1", "parked");
}

/// (7) A failure beside a wait is a failure. The gated ticket is deliberately
/// waiting and the other ticket's worker exited `0` without the artifact it
/// owed — an actionable blocker, so the run exits `1` and never `3`.
/// `--continue-on-error` governs how far independent work drains inside a
/// pass, never this precedence.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn an_actionable_blocker_beside_a_gate_defeats_idle() {
    for extra in [vec![], vec!["--continue-on-error"]] {
        let plan = IdlePlan::new(
            "until-idle-failure-beside-wait",
            "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n\n\
             ### Task 2: Owes an artifact\n**State:** silent\n",
        );

        let mut args = vec!["--until-idle", "--no-dashboard"];
        args.extend(extra.iter().copied());
        let result = plan.run(&args);

        assert_exit(&result, 1, &format!("attention outranks idle (args {args:?})"));
        let combined = format!("{}{}", result.stdout, result.stderr);
        assert!(
            !combined.contains("Run idle:"),
            "a run with work that needs a person is not idle; got:\n{combined}"
        );
    }
}

/// (8) A live claim beside a wait is the same answer for a different reason: a
/// claim is something an operator repairs, so the ticket is actionable and the
/// run says whose it is and what clears it.
// §FS-rhei-run-report.3.1
#[test]
fn a_live_claim_beside_a_gate_defeats_idle() {
    let plan = IdlePlan::new(
        "until-idle-claim-beside-wait",
        "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n\n\
         ### Task 2: Held by someone\n**State:** work\n**Assignee:** alice\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, 1, "a claim an operator must release is not a deliberate wait");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("rhei release"), "the remedy for a claim is named; got:\n{combined}");
    assert!(!combined.contains("Run idle:"), "got:\n{combined}");
}

/// (9) The control that matters most to everyone who never types the flag: a
/// finished plan exits `0` with the option and without it, and the option adds
/// nothing to what it prints.
// §FS-rhei-run-json.5
#[test]
fn a_finished_plan_exits_zero_with_the_option_and_without_it() {
    let selected = IdlePlan::new(
        "until-idle-terminal-selected",
        "## Tasks\n\n### Task 1: Already done\n**State:** done\n",
    );
    let unselected = IdlePlan::new(
        "until-idle-terminal-unselected",
        "## Tasks\n\n### Task 1: Already done\n**State:** done\n",
    );

    let with = selected.run(&["--until-idle", "--no-dashboard"]);
    let without = unselected.run(&["--no-tui", "--no-dashboard"]);

    assert_exit(&with, 0, "a finished plan is complete, not idle");
    assert_exit(&without, 0, "and is complete without the option too");
    assert!(
        !with.stdout.contains("Run idle:"),
        "nothing is waiting, so there is no idle stop to report; got:\n{}",
        with.stdout
    );
    let states = |text: &str| {
        text.lines()
            .find(|line| line.starts_with("Final states:"))
            .unwrap_or_else(|| panic!("the line-oriented summary is preserved; got:\n{text}"))
            .to_string()
    };
    assert_eq!(
        states(&with.stdout),
        states(&without.stdout),
        "the option changes nothing about a plan it has nothing to wait for"
    );
}
