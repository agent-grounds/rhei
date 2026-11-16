//! The row a ticket gets when only its own poll deadline holds it.
//!
//! A plain poll — no `waiting_on` — that is waiting out its interval is a
//! deliberate wait: rhei resumes it by itself once `pollNextAttemptAt` passes,
//! and the run's own idle line already counts it `waiting on poll`. Its row
//! fell through to `not scheduled before the run halted`, whose remedies sent
//! the reader to rerun, claim, or debug a ticket that was fine, under a `!`.
//! These cases pin the calm row on every surface (#736) and the precedence that
//! keeps a claim and a prior ahead of it.
// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1

use super::until_idle_support::*;

const GENERIC: &str = "not scheduled before the run halted";

/// One ticket in the plain `poll` state, its next attempt seeded in the future.
fn polled_plan(prefix: &str, tasks: &str) -> IdlePlan {
    IdlePlan::new(
        prefix,
        &format!(
            "---\nmetadata:\n  tasks:\n    1:\n      pollNextAttemptAt:\n        \
             poll: {FUTURE_POLL}\n---\n\n## Tasks\n\n{tasks}"
        ),
    )
}

fn poll_row() -> String {
    format!(
        "Task plan.1 (poll): waiting on its poll until {} \u{2014} nothing to do on \
         Task plan.1; rhei resumes it itself",
        instant(FUTURE_POLL)
    )
}

/// The issue's own transcript: the console row, the report's `## Waiting` row
/// and its Final States line all read as a calm wait, never as a ticket the
/// run failed to reach.
// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1
#[test]
fn until_idle_explains_a_plain_poll_wait_as_waiting_on_its_poll() {
    let plan = polled_plan("poll-wait-row", "### Task 1: Retries later\n**State:** poll\n");

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "a future poll is a deliberate wait");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains(&poll_row()), "the poll wait is not named in:\n{combined}");
    assert!(!combined.contains(GENERIC), "the generic row is still printed in:\n{combined}");

    let report = plan.report();
    let next = instant(FUTURE_POLL);
    assert!(report.contains("## Waiting"), "no Waiting section in:\n{report}");
    assert!(
        report.contains(&format!(
            "| plan.1 | poll | waiting on its poll until {next} | \
             nothing to do on Task plan.1; rhei resumes it itself |"
        )),
        "the Waiting row is not the poll's in:\n{report}"
    );
    assert!(
        report.contains(&format!(
            "- \u{23f8} `plan.1` (poll) \u{2014} waiting on its poll until {next}"
        )),
        "the Final States line does not carry the calm marker in:\n{report}"
    );
    assert!(!report.contains("- ! `plan.1`"), "the ticket is still marked `!` in:\n{report}");
    assert!(!report.contains(GENERIC), "the generic row is still in the report:\n{report}");
}

/// `--dry-run` predicts the same row the run prints.
// §FS-rhei-run-report.3.1 §FS-rhei-run.4
#[test]
fn dry_run_predicts_the_plain_poll_wait_row() {
    let plan = polled_plan("poll-wait-row-dry", "### Task 1: Retries later\n**State:** poll\n");

    let result = plan.run(&["--until-idle", "--dry-run", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "a future poll is a deliberate wait");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains(&poll_row()), "the poll wait is not predicted in:\n{combined}");
    assert!(!combined.contains(GENERIC), "the generic row is still predicted in:\n{combined}");
}

/// A live claim stops the poll from ever reaching its next attempt, so the
/// claim keeps the row and the poll row would be a false promise. Holds today;
/// it guards the new rung's place behind the claim.
// §FS-rhei-run-report.3.1 §FS-rhei-run.3
#[test]
fn a_claim_on_a_plain_poll_keeps_the_claim_row() {
    let plan = polled_plan(
        "poll-wait-row-claim",
        "### Task 1: Retries later, and claimed\n**State:** poll\n**Assignee:** alice\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, 1, "a claim stops a poll from reaching its next attempt");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("Task plan.1 (poll): claimed by alice"), "{combined}");
    assert!(!combined.contains("waiting on its poll"), "{combined}");
    assert!(!plan.report().contains("waiting on its poll"), "{}", plan.report());
}

/// An unsatisfied prior outranks the poll for the same reason. Holds today; it
/// guards the new rung's place behind the prior.
// §FS-rhei-run-report.3.1 §FS-rhei-run.3
#[test]
fn a_prior_on_a_plain_poll_keeps_the_prior_row() {
    let plan = IdlePlan::new(
        "poll-wait-row-prior",
        &format!(
            "---\nmetadata:\n  tasks:\n    2:\n      pollNextAttemptAt:\n        \
             poll: {FUTURE_POLL}\n---\n\n## Tasks\n\n\
             ### Task 1: Waiting on a reviewer\n**State:** gate\n\n\
             ### Task 2: Retries, once the gate clears\n**State:** poll\n**Prior:** Task 1\n"
        ),
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("Task plan.2 (poll): waiting on Task plan.1 (gate)"),
        "the prior no longer explains the ticket in:\n{combined}"
    );
    assert!(!combined.contains("waiting on its poll"), "{combined}");
    assert!(!plan.report().contains("waiting on its poll"), "{}", plan.report());
}
