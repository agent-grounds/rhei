//! An expired provider-limit record on a ticket held back for a missing input
//! (agent-grounds/rhei#728).
//!
//! The ticket cannot start because its state's required input is not on disk,
//! and it still carries the `providerLimits` record a past session limit left
//! behind. The deadline scan took that record for a deadline of "now", so a
//! continuous run printed `No ready tasks; sleeping 1s …` every second and never
//! halted, and `--until-idle` reported a next attempt nothing could take. A
//! missing input is a blocker no clock releases: the ticket contributes no
//! deadline, and the run halts with the report it gives without the record.
// §FS-rhei-run.5.1.1 §FS-rhei-run-report.3.1

use super::until_idle_support::*;
use super::*;
use std::time::Duration;

/// Far above how long the halt takes, and far below forever: the defect is a
/// run that sleeps one second at a time without end, so the guard is what turns
/// it into a failure instead of a hung suite.
const HALT_PATIENCE: Duration = Duration::from_secs(20);

/// One agent state that requires `runtime/brief.md`, which no case writes.
fn needs_brief_machine() -> &'static str {
    r#"name: expired-wait-missing-input
version: 1
states:
  needs:
    initial: true
    description: Agent work that needs a brief
    target: codex:openai:alpha
    agent_timeout: 30m
    inputs:
      - name: brief
        path: runtime/brief.md
  done:
    final: true
    description: Done
transitions:
  - from: needs
    to: done
"#
}

/// The issue's fixture: one ticket missing its input, carrying a provider
/// record whose wait ended long ago — or, with `record: false`, the control.
fn missing_input_plan(prefix: &str, record: bool) -> IdlePlan {
    let front = if record {
        format!(
            "---\nmetadata:\n  tasks:\n    1:\n      {}\n---\n\n",
            provider_limit("needs", ELAPSED_POLL)
        )
    } else {
        String::new()
    };
    let plan = IdlePlan::new(
        prefix,
        &format!("{front}## Tasks\n\n### Task 1: Needs a brief\n**State:** needs\n"),
    );
    fs::write(&plan.machine, needs_brief_machine()).expect("write the machine");
    plan
}

fn run_within(plan: &IdlePlan, args: &[&str]) -> CliRun {
    let mut command = rhei_command(&plan.home);
    command.arg("--state-machine").arg(&plan.machine).arg("run").arg(&plan.plan).args(args);
    bounded_within(command, &format!("rhei run {}", args.join(" ")), plan.root(), HALT_PATIENCE)
}

/// What both runs must print and not print: the halt report naming the missing
/// input, and never the deadline sleep.
#[track_caller]
fn assert_halted_on_missing_input(result: &CliRun, what: &str) {
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_ne!(result.status.code(), Some(0), "{what}: the run halts non-zero; got:\n{combined}");
    for expected in ["Nothing to schedule", "required input(s) not found: brief"] {
        assert!(combined.contains(expected), "{what}: expected {expected:?}; got:\n{combined}");
    }
    assert!(
        !combined.contains("No ready tasks; sleeping"),
        "{what}: no deadline can release this ticket, so the run must not sleep; got:\n{combined}"
    );
}

/// The control: without the record the continuous run halts at once with the
/// `Nothing to schedule` report. It passes today and pins the fixture.
// §FS-rhei-run-report.3.1
#[test]
fn a_ticket_missing_its_input_halts_with_nothing_to_schedule() {
    let plan = missing_input_plan("missing-input-control", false);
    let result = run_within(&plan, &["--no-tui"]);
    assert_halted_on_missing_input(&result, "no provider record");
    assert!(plan.records().is_empty(), "nothing is spawned for a ticket missing its input");
}

/// The ticket's own case: the expired record changes nothing. The continuous
/// run halts with the same report as the control, instead of sleeping 1 s and
/// rescanning until something kills it. §FS-rhei-run.5.1.1
#[test]
#[ignore = "red until #728: an expired provider record on a ticket missing its input keeps the \
            continuous run sleeping 1s forever; implement removes this"]
fn an_expired_provider_record_does_not_keep_a_ticket_missing_its_input_waiting() {
    let plan = missing_input_plan("missing-input-expired-record", true);
    let result = run_within(&plan, &["--no-tui"]);
    assert_halted_on_missing_input(&result, "an expired provider record");
    assert!(plan.records().is_empty(), "nothing is spawned for a ticket missing its input");
}

/// The same plan under `--until-idle`: the ticket contributes no next attempt,
/// so `next_attempt_at` is present and null rather than the instant of the
/// scan, and the run is not idle — a missing input is the operator's to fix.
/// This mode already halts here, because it judges the remaining work before it
/// consults the deadline; the case keeps it so while the scan is changed.
// §FS-rhei-run.5.1.1 §FS-rhei-run-json.2.1
#[test]
fn an_expired_provider_record_on_a_ticket_missing_its_input_reports_no_next_attempt() {
    let plan = missing_input_plan("missing-input-expired-record-idle", true);
    let result = run_within(&plan, &["--until-idle", "--json", "--no-dashboard"]);
    assert_exit(&result, 1, "a missing input needs the operator, so the run is not idle");
    let stop = stop_payload(&result);
    assert_eq!(
        stop["next_attempt_at"],
        serde_json::Value::Null,
        "no clock releases a missing input; got stop {stop}"
    );
    assert_ne!(stop["reason"], "idle", "got stop {stop}");
}
