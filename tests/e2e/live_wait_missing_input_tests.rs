//! A live clock on a ticket held back for a missing input (agent-grounds/rhei#728).
//!
//! The ticket cannot start because its state's required input is not on disk,
//! and it carries a clock that has not run out: a provider-limit record still
//! ahead, or a pending `pollNextAttemptAt`. Neither is its wait, because the
//! clock running out would still find the file missing. So the run reads it as
//! it reads the same ticket with no clock at all: reported for the input,
//! exit non-zero, never idle, and never a promise that rhei resumes it. A
//! program poll halts the callback mode the same way instead of sleeping on it.
//!
//! The same plans with the input written still wait on their clock: the
//! predicate is about what holds the ticket, not about clocks.
// §FS-rhei-run.5.1.1 §FS-rhei-run-report.3.1

use super::until_idle_support::*;
use super::*;
use std::time::Duration;

/// Far above how long the halt takes; the regression is a run that sleeps until
/// 2030 or one that exits `0`, and the guard turns the first into a failure.
const HALT_PATIENCE: Duration = Duration::from_secs(20);

/// Agent work that needs a brief, and an agent poll that needs one.
const AGENT_MACHINE: &str = r#"name: live-wait-missing-input
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
  timed:
    description: An agent poll that needs a brief
    target: codex:openai:alpha
    agent_timeout: 30m
    inputs:
      - name: brief
        path: runtime/brief.md
    poll:
      interval: 30m
      max_attempts: 5
  done:
    final: true
    description: Done
transitions:
  - from: needs
    to: done
  - from: timed
    to: timed
    condition: pollAttempts < pollMaxAttempts
  - from: timed
    to: done
    condition: pollAttempts >= pollMaxAttempts
"#;

/// A plan of one ticket in `state`, with `front` as the ticket's metadata (empty
/// for none), under the machine `machine` renders in the plan's own directory.
fn one_ticket(prefix: &str, state: &str, front: &str, machine: fn(&Path) -> String) -> IdlePlan {
    let front = if front.is_empty() {
        String::new()
    } else {
        format!("---\nmetadata:\n  tasks:\n    1:\n      {front}\n---\n\n")
    };
    let plan = IdlePlan::new(
        prefix,
        &format!("{front}## Tasks\n\n### Task 1: Needs a brief\n**State:** {state}\n"),
    );
    fs::write(&plan.machine, machine(plan.root())).expect("write the machine");
    plan
}

fn agent_machine(_: &Path) -> String {
    AGENT_MACHINE.to_string()
}

/// A provider record on `needs` that runs out in 2030.
fn live_record_plan(prefix: &str) -> IdlePlan {
    one_ticket(prefix, "needs", &provider_limit("needs", FUTURE_POLL), agent_machine)
}

/// A poll in `state` whose next attempt is in 2030.
fn pending_poll_plan(prefix: &str, state: &str, machine: fn(&Path) -> String) -> IdlePlan {
    let front = format!("pollNextAttemptAt:\n        {state}: {FUTURE_POLL}");
    one_ticket(prefix, state, &front, machine)
}

fn write_brief(plan: &IdlePlan) {
    let runtime = plan.root().join("runtime");
    fs::create_dir_all(&runtime).expect("runtime directory");
    fs::write(runtime.join("brief.md"), "# Brief\n").expect("write the brief");
}

fn run_within(plan: &IdlePlan, args: &[&str]) -> CliRun {
    let mut command = rhei_command(&plan.home);
    command.arg("--state-machine").arg(&plan.machine).arg("run").arg(&plan.plan).args(args);
    bounded_within(command, &format!("rhei run {}", args.join(" ")), plan.root(), HALT_PATIENCE)
}

/// The continuous run halts non-zero, names the input, never sleeps on the
/// clock, and never says the ticket comes back by itself.
#[track_caller]
fn assert_reported_for_the_input(result: &CliRun, what: &str) {
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_ne!(result.status.code(), Some(0), "{what}: the run halts non-zero; got:\n{combined}");
    for expected in ["Nothing to schedule", "required input(s) not found: brief"] {
        assert!(combined.contains(expected), "{what}: expected {expected:?}; got:\n{combined}");
    }
    for refused in
        ["No ready tasks; sleeping", "rhei resumes it automatically", "rerun to pick it up"]
    {
        assert!(
            !combined.contains(refused),
            "{what}: must not print {refused:?}; got:\n{combined}"
        );
    }
}

/// Under `--until-idle --json`: not idle, exit 1, and no next attempt.
#[track_caller]
fn assert_attention_without_next_attempt(result: &CliRun, what: &str) {
    assert_exit(result, 1, &format!("{what}: a missing input needs the operator"));
    let stop = stop_payload(result);
    assert_eq!(stop["reason"], "attention", "{what}: got stop {stop}");
    assert_eq!(stop["next_attempt_at"], serde_json::Value::Null, "{what}: got stop {stop}");
}

/// A live provider record is no more the wait of a ticket missing its input
/// than an expired one: the continuous run reports the input.
#[test]
fn a_live_provider_record_on_a_ticket_missing_its_input_is_reported_for_the_input() {
    let plan = live_record_plan("live-record-missing-input");
    let result = run_within(&plan, &["--no-tui"]);
    assert_reported_for_the_input(&result, "a live provider record");
    assert!(plan.records().is_empty(), "nothing is spawned for a ticket missing its input");
}

/// The same under `--until-idle`: the record does not make the run idle.
// §FS-rhei-run-json.2.1
#[test]
fn a_live_provider_record_on_a_ticket_missing_its_input_is_not_idle() {
    let plan = live_record_plan("live-record-missing-input-idle");
    let result = run_within(&plan, &["--until-idle", "--json", "--no-dashboard"]);
    assert_attention_without_next_attempt(&result, "a live provider record");
}

/// A pending agent poll is no wait for its missing input either.
#[test]
fn a_pending_agent_poll_missing_its_input_is_reported_for_the_input() {
    let plan = pending_poll_plan("pending-poll-missing-input", "timed", agent_machine);
    let result = run_within(&plan, &["--no-tui"]);
    assert_reported_for_the_input(&result, "a pending agent poll");
}

/// The same under `--until-idle`.
// §FS-rhei-run-json.2.1
#[test]
fn a_pending_agent_poll_missing_its_input_is_not_idle() {
    let plan = pending_poll_plan("pending-poll-missing-input-idle", "timed", agent_machine);
    let result = run_within(&plan, &["--until-idle", "--json", "--no-dashboard"]);
    assert_attention_without_next_attempt(&result, "a pending agent poll");
}

/// The guard against reading every clock as nothing: with the brief written,
/// the live record is the ticket's only wait, so `--until-idle` is idle on it
/// and names its instant.
// §FS-rhei-run.5.1.1 §FS-rhei-run.3.3
#[test]
fn a_live_provider_record_alone_is_still_waited_for() {
    let plan = live_record_plan("live-record-with-input");
    write_brief(&plan);
    let result = run_within(&plan, &["--until-idle", "--no-dashboard"]);
    assert_exit(&result, EXIT_IDLE, "the record alone holds the ticket");
    assert_idle_line(&result, "provider_limit", &instant(FUTURE_POLL));
}

/// The same guard for the poll: with the brief written its deadline is the wait.
// §FS-rhei-run.5.1.1 §FS-rhei-run.5.1
#[test]
fn a_pending_agent_poll_alone_is_still_waited_for() {
    let plan = pending_poll_plan("pending-poll-with-input", "timed", agent_machine);
    write_brief(&plan);
    let result = run_within(&plan, &["--until-idle", "--no-dashboard"]);
    assert_exit(&result, EXIT_IDLE, "the poll deadline alone holds the ticket");
    assert_idle_line(&result, "poll", &instant(FUTURE_POLL));
}

/// A program poll that needs a brief: with no agent state in the machine and
/// the input missing, the run is in callback mode.
fn program_poll_machine(dir: &Path) -> String {
    let command = fixture_command(&write_python_agent(dir, "poll.py", "pass\n"));
    format!(
        r#"name: callback-poll-missing-input
version: 1
states:
  polled:
    initial: true
    description: A program poll that needs a brief
    program:
      command: {command}
    program_timeout: 2m
    inputs:
      - name: brief
        path: runtime/brief.md
    poll:
      interval: 30m
      max_attempts: 5
  done:
    final: true
    description: Done
transitions:
  - from: polled
    to: polled
    condition: pollAttempts < pollMaxAttempts
  - from: polled
    to: done
    condition: pollAttempts >= pollMaxAttempts
"#
    )
}

/// The callback mode sleeps on the same deadline scan, so a pending program
/// poll missing its input halts it at once, naming the input, instead of
/// sleeping until the poll's deadline. The control, the same plan with no
/// deadline, gives the same report. §FS-rhei-run.5.1
#[test]
fn a_pending_program_poll_missing_its_input_halts_the_callback_mode() {
    for (prefix, deadline) in [("callback-poll-control", false), ("callback-poll-pending", true)] {
        let plan = if deadline {
            pending_poll_plan(prefix, "polled", program_poll_machine)
        } else {
            one_ticket(prefix, "polled", "", program_poll_machine)
        };
        let result = run_within(&plan, &["--no-tui"]);
        assert_reported_for_the_input(&result, &format!("{prefix}: a program poll"));
    }
}
