//! The instant a selected run reports, and the one filter that turns a
//! scheduler hint into a published contract.
//!
//! The composition is `§FS-rhei-run.5.1`'s and stays shared — the *later* of a
//! task's applicable poll and provider-limit deadlines, then the *earliest*
//! such instant across waiting tasks. What is added is that a task contributes
//! only when its deadline **alone** makes it eligible for this invocation.
// §FS-rhei-run.5.1 §FS-rhei-run.3

use super::until_idle_support::*;

fn polled(task: &str, state: &str, deadline: u64) -> String {
    format!("    {task}:\n      pollNextAttemptAt:\n        {state}: {deadline}\n")
}

/// (2) A seeded future deadline is reported exactly, as an RFC 3339 UTC
/// instant, on the console and in the stream alike.
// §FS-rhei-run.5.1 §FS-rhei-run-json.2.1
#[test]
fn until_idle_reports_the_seeded_future_deadline_exactly() {
    let plan = IdlePlan::new(
        "until-idle-future-poll",
        &format!(
            "---\nmetadata:\n  tasks:\n{}---\n\n## Tasks\n\n### Task 1: Retries later\n**State:** poll\n",
            polled("1", "poll", FUTURE_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--json", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "a future poll is a deliberate wait");
    let stop = stop_payload(&result);
    assert_eq!(stop["reason"], "idle");
    assert_eq!(stop["idle_blocker"], "poll");
    assert_eq!(stop["next_attempt_at"], serde_json::json!(instant(FUTURE_POLL)));
    assert_eq!(stop["exit_code"], 3);
}

/// (4) Poll and provider on one task compose to the *later* of the two: the
/// task cannot resume until both conditions permit it, and reporting the
/// earlier one would wake a timer for work that still cannot run.
// §FS-rhei-run.5.1
#[test]
fn a_poll_and_a_provider_limit_on_one_task_report_the_later_instant() {
    let plan = IdlePlan::new(
        "until-idle-later-of-two",
        &format!(
            r#"---
metadata:
  tasks:
    1:
      pollNextAttemptAt:
        timed: {LATER_POLL}
      {}
---

## Tasks

### Task 1: Both waits at once
**State:** timed
"#,
            provider_limit("timed", FUTURE_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--json", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "both waits are deliberate");
    let stop = stop_payload(&result);
    assert_eq!(
        stop["next_attempt_at"],
        serde_json::json!(instant(LATER_POLL)),
        "the later condition is the one that governs this task"
    );
}

/// (5) Two waiting tasks report the *earliest* instant across them: that is
/// the first moment a next invocation could find anything to do.
// §FS-rhei-run.5.1
#[test]
fn two_waiting_tasks_report_the_earliest_instant() {
    let plan = IdlePlan::new(
        "until-idle-earliest-of-two",
        &format!(
            "---\nmetadata:\n  tasks:\n{}{}---\n\n## Tasks\n\n\
             ### Task 1: Retries later\n**State:** poll\n\n\
             ### Task 2: Retries sooner\n**State:** poll\n",
            polled("1", "poll", LATER_POLL),
            polled("2", "poll", FUTURE_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--json", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "two future polls are two deliberate waits");
    assert_eq!(
        stop_payload(&result)["next_attempt_at"],
        serde_json::json!(instant(FUTURE_POLL)),
        "the earliest deadline is when a next invocation could first find work"
    );
}

/// (6) A future poll behind a gated prior contributes nothing. This is the
/// case that catches the unfiltered scan: the deadline is real and the ticket
/// is still idle-compatible, but its expiry cannot make the ticket runnable —
/// the gate can — so reporting the poll's instant would wake a timer for
/// nothing. The value is present and null, and the wait named is the gate.
// §FS-rhei-run.5.1 §FS-rhei-run-json.2.1
#[test]
fn a_deadline_behind_a_gated_prior_contributes_no_next_attempt() {
    let plan = IdlePlan::new(
        "until-idle-conditional-deadline",
        &format!(
            "---\nmetadata:\n  tasks:\n{}---\n\n## Tasks\n\n\
             ### Task 1: Waiting on a reviewer\n**State:** gate\n\n\
             ### Task 2: Retries, once the gate clears\n**State:** poll\n**Prior:** Task 1\n",
            polled("2", "poll", FUTURE_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--json", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "both tickets are deliberately waiting");
    let stop = stop_payload(&result);
    assert_eq!(stop["reason"], "idle");
    assert_eq!(
        stop["next_attempt_at"],
        serde_json::Value::Null,
        "present and null: this plan has no timed retry a next invocation could take"
    );
    assert_eq!(stop["idle_blocker"], "gate", "the gate is what actually holds both tickets");
    assert!(
        !result.stdout.contains(&instant(FUTURE_POLL)),
        "the poll's instant must not be published; got:\n{}",
        result.stdout
    );
}

/// (20) The *at-or-before* side of the stopping boundary. A deadline that has
/// already elapsed makes its task ready, so the run works it and takes another
/// pass; the elapsed instant is never reported as outstanding. Case (2) is the
/// *after* side of the same boundary.
// §FS-rhei-run.3 §FS-rhei-run.5.1
#[test]
fn an_already_elapsed_deadline_is_worked_rather_than_reported() {
    let plan = IdlePlan::new(
        "until-idle-elapsed-deadline",
        &format!(
            "---\nmetadata:\n  tasks:\n{}---\n\n## Tasks\n\n### Task 1: Due already\n**State:** poll\n",
            polled("1", "poll", ELAPSED_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--json", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "the retry runs, then the next one is still ahead");
    assert!(
        !records_of(&result, "slot_released").is_empty(),
        "the elapsed deadline made the task ready, so a worker ran; got:\n{}",
        result.stdout
    );
    let stop = stop_payload(&result);
    assert_ne!(
        stop["next_attempt_at"],
        serde_json::json!(instant(ELAPSED_POLL)),
        "a deadline the run already spent is not an outstanding one"
    );
    assert_ne!(
        stop["next_attempt_at"],
        serde_json::Value::Null,
        "the self-loop persisted a fresh deadline, which is what a caller comes back for"
    );
}
