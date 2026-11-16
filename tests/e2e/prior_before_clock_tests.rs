//! A clock on a ticket an unsatisfied `**Prior:**` holds (agent-grounds/rhei#728).
//!
//! The ticket waits behind a cancelled prior, which can never satisfy it, and
//! carries a clock of its own that has not run out: a pending program poll, a
//! pending agent poll, or a live provider-limit record. None of them is its
//! wait, because the clock running out would still find the prior cancelled.
//! The prior answers before either clock, as the halt row reads it, so the run
//! reports the cancelled prior, exits non-zero, and is never idle on the clock.
//!
//! The guard keeps the other half: a prior that is itself waiting on a clock is
//! still a deliberate wait for the ticket behind it, and its deadline is the
//! one `--until-idle` names.
// §FS-rhei-run.5.1.1 §FS-rhei-run-report.3.1

use super::until_idle_support::*;
use super::*;
use std::time::Duration;

/// Far above how long the halt takes; the regression is a run that exits `0`
/// on the clock, and the guard turns one sleeping until 2030 into a failure.
const HALT_PATIENCE: Duration = Duration::from_secs(20);

/// An agent poll and agent work a provider limit can park, beside a
/// cancellation state.
const AGENT_MACHINE: &str = r#"name: prior-before-clock
version: 1
states:
  timed:
    initial: true
    description: An agent poll
    target: codex:openai:alpha
    agent_timeout: 30m
    poll:
      interval: 30m
      max_attempts: 5
  work:
    description: Agent work a provider limit can park
    target: codex:openai:alpha
    agent_timeout: 30m
  done:
    final: true
    description: Done
  cancelled:
    final: true
    description: Dropped
transitions:
  - from: timed
    to: timed
    condition: pollAttempts < pollMaxAttempts
  - from: timed
    to: done
    condition: pollAttempts >= pollMaxAttempts
  - from: work
    to: done
"#;

fn agent_machine(_: &Path) -> String {
    AGENT_MACHINE.to_string()
}

/// A program poll and nothing an agent runs, so the run is in callback mode.
fn program_machine(dir: &Path) -> String {
    let command = fixture_command(&write_python_agent(dir, "poll.py", "pass\n"));
    format!(
        r#"name: prior-before-clock-callback
version: 1
states:
  polled:
    initial: true
    description: A program poll
    program:
      command: {command}
    program_timeout: 2m
    poll:
      interval: 30m
      max_attempts: 5
  done:
    final: true
    description: Done
  cancelled:
    final: true
    description: Dropped
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

/// Task 1 in `prior`, and Task 2 in `state` behind it, with `front` as Task 2's
/// metadata, under the machine `machine` renders in the plan's own directory.
fn behind(
    prefix: &str,
    (prior, state): (&str, &str),
    front: &str,
    machine: fn(&Path) -> String,
) -> IdlePlan {
    let plan = IdlePlan::new(
        prefix,
        &format!(
            "---\nmetadata:\n  tasks:\n    2:\n      {front}\n---\n\n## Tasks\n\n\
             ### Task 1: Dropped\n**State:** {prior}\n\n\
             ### Task 2: Waits after it\n**State:** {state}\n**Prior:** Task 1\n"
        ),
    );
    fs::write(&plan.machine, machine(plan.root())).expect("write the machine");
    plan
}

fn pending_poll(state: &str, deadline: u64) -> String {
    format!("pollNextAttemptAt:\n        {state}: {deadline}")
}

fn run_within(plan: &IdlePlan, args: &[&str]) -> CliRun {
    let mut command = rhei_command(&plan.home);
    command.arg("--state-machine").arg(&plan.machine).arg("run").arg(&plan.plan).args(args);
    bounded_within(command, &format!("rhei run {}", args.join(" ")), plan.root(), HALT_PATIENCE)
}

/// The continuous run halts with exit `1`, names the cancelled prior, never
/// sleeps on the clock, and never says the ticket comes back by itself.
#[track_caller]
fn assert_reported_for_the_prior(result: &CliRun, what: &str) {
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_eq!(result.status.code(), Some(1), "{what}: the run halts non-zero; got:\n{combined}");
    for expected in [
        "Nothing to schedule",
        "waiting on Task plan.1 (cancelled), a cancelled prior that can never satisfy it",
    ] {
        assert!(combined.contains(expected), "{what}: expected {expected:?}; got:\n{combined}");
    }
    for refused in ["No ready tasks; sleeping", "rhei resumes it automatically"] {
        assert!(
            !combined.contains(refused),
            "{what}: must not print {refused:?}; got:\n{combined}"
        );
    }
}

/// Under `--until-idle --json`: not idle, exit 1, and no next attempt.
#[track_caller]
fn assert_attention_without_next_attempt(result: &CliRun, what: &str) {
    assert_exit(result, 1, &format!("{what}: a cancelled prior needs the operator"));
    let stop = stop_payload(result);
    assert_eq!(stop["reason"], "attention", "{what}: got stop {stop}");
    assert_eq!(stop["next_attempt_at"], serde_json::Value::Null, "{what}: got stop {stop}");
}

/// Both modes of one case: the continuous run, then `--until-idle --json`.
fn assert_held_by_the_prior(prefix: &str, state: &str, front: &str, machine: fn(&Path) -> String) {
    let plan = behind(prefix, ("cancelled", state), front, machine);
    assert_reported_for_the_prior(&run_within(&plan, &["--no-tui"]), prefix);
    let idle = behind(&format!("{prefix}-idle"), ("cancelled", state), front, machine);
    let result = run_within(&idle, &["--until-idle", "--json", "--no-dashboard"]);
    assert_attention_without_next_attempt(&result, prefix);
    assert!(idle.records().is_empty(), "{prefix}: nothing is spawned behind a cancelled prior");
}

/// A pending program poll does not hold the callback mode, or make the run idle.
// §FS-rhei-run.5.1 §FS-rhei-run-json.2.1
#[test]
fn a_program_poll_behind_a_cancelled_prior_is_reported_for_the_prior() {
    let front = pending_poll("polled", FUTURE_POLL);
    assert_held_by_the_prior("cancelled-prior-program-poll", "polled", &front, program_machine);
}

/// The same for an agent poll.
// §FS-rhei-run.5.1 §FS-rhei-run-json.2.1
#[test]
fn an_agent_poll_behind_a_cancelled_prior_is_reported_for_the_prior() {
    let front = pending_poll("timed", FUTURE_POLL);
    assert_held_by_the_prior("cancelled-prior-agent-poll", "timed", &front, agent_machine);
}

/// The same for a provider-limit record that runs out in 2030.
// §FS-rhei-run.3.3 §FS-rhei-run-json.2.1
#[test]
fn a_live_provider_record_behind_a_cancelled_prior_is_reported_for_the_prior() {
    let front = provider_limit("work", FUTURE_POLL);
    assert_held_by_the_prior("cancelled-prior-live-record", "work", &front, agent_machine);
}

/// The guard against reading every prior as a blocker: a prior waiting on its
/// own poll is a deliberate wait for the ticket behind it, which carries a later
/// poll of its own, so `--until-idle` is idle and names the prior's instant.
// §FS-rhei-run.3 §FS-rhei-run.5.1
#[test]
fn a_prior_waiting_on_a_live_poll_still_makes_the_run_idle() {
    let plan = behind(
        "waiting-prior-live-poll",
        ("timed", "timed"),
        &pending_poll("timed", LATER_POLL),
        agent_machine,
    );
    let text = fs::read_to_string(&plan.plan).expect("read the plan");
    let prior = format!("    1:\n      {}\n    2:", pending_poll("timed", FUTURE_POLL));
    fs::write(&plan.plan, text.replacen("    2:", &prior, 1)).expect("seed the prior's poll");
    let result = run_within(&plan, &["--until-idle", "--no-dashboard"]);
    assert_exit(&result, EXIT_IDLE, "the prior's poll holds both tickets");
    assert_idle_line(&result, "poll", &instant(FUTURE_POLL));
}
