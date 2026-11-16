//! An expired provider-limit record beside a visit whose attempts are spent
//! (agent-grounds/rhei#728).
//!
//! A provider record is a deadline for every ticket on its execution identity,
//! and an expired one a deadline of "now". A ticket whose spawn site refuses its
//! next spawn for a spent attempt budget is released only by `rhei retry`, so no
//! record is its wait, its own or another ticket's. The continuous run sleeping
//! 1 s and rescanning it printed the same refusal every pass and never halted.
//!
//! Each case counts the spawns its agent made: the budget is two attempts, and
//! no case spawns past it.
// §FS-rhei-run.5.1.1 §FS-rhei-agents.3.2.3

use super::until_idle_support::*;
use super::*;
use std::time::Duration;

/// Far above how long the halt takes; the regression is a run that never ends.
const HALT_PATIENCE: Duration = Duration::from_secs(20);

/// Work whose attempts run out, work that needs a brief, and a supervisor over
/// a gated child.
const MACHINE: &str = r#"name: spent-budget-wait
version: 1
states:
  work:
    initial: true
    description: Agent work whose attempts run out
    target: codex:openai:alpha
    agent_timeout: 30m
  needs:
    description: Agent work that needs a brief
    target: codex:openai:alpha
    agent_timeout: 30m
    inputs:
      - name: brief
        path: runtime/brief.md
  supervising:
    description: Supervises the subtree beneath it
    execute_on: child-terminal
    target: codex:openai:alpha
    agent_timeout: 30m
    visits: 12
  review:
    description: Waiting on a human decision
    gating: true
  done:
    final: true
    description: Done
transitions:
  - from: work
    to: done
  - from: needs
    to: done
  - from: supervising
    to: supervising
  - from: supervising
    to: done
    condition: openDescendants < 1
  - from: review
    to: done
"#;

/// How the agent ends every attempt: with exit `1`, or with exit `0` and no
/// result, which leaves the completion condition unmet.
#[derive(Clone, Copy)]
enum Attempt {
    Fails,
    WritesNothing,
}

/// A plan whose agent ends each attempt as `attempt` says and logs each spawn
/// beside itself. `records` is the `metadata.tasks` mapping, empty for none.
fn spent_plan(prefix: &str, records: &str, tasks: &str, attempt: Attempt) -> IdlePlan {
    let front = if records.is_empty() {
        String::new()
    } else {
        format!("---\nmetadata:\n  tasks:\n{records}---\n\n")
    };
    let plan = IdlePlan::new(prefix, &format!("{front}## Tasks\n\n{tasks}"));
    fs::write(&plan.machine, MACHINE).expect("write the machine");
    let exit = match attempt {
        Attempt::Fails => "sys.exit(1)\n",
        Attempt::WritesNothing => "sys.exit(0)\n",
    };
    let body =
        format!("append(str(pathlib.Path(__file__).with_name('spawns.log')), 'spawn\\n')\n{exit}");
    let agent = write_python_agent(plan.root(), "agent.py", &body);
    let profile = serde_json::json!({
        "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&agent))
            .expect("fixture command"),
        "stdin_prompt": true,
        "timeout": "12s"
    });
    fs::write(
        plan.root().join(".agent-grounds/rhei/settings.json"),
        serde_json::json!({"agents": {"codex": profile}}).to_string(),
    )
    .expect("write agent settings");
    plan
}

/// The issue's expired record on ticket `task` in `state`.
fn expired_record(task: &str, state: &str) -> String {
    format!("    {task}:\n      {}\n", provider_limit(state, ELAPSED_POLL))
}

fn spawns(plan: &IdlePlan) -> usize {
    fs::read_to_string(plan.root().join("spawns.log")).unwrap_or_default().lines().count()
}

fn run_within(plan: &IdlePlan, args: &[&str]) -> CliRun {
    let mut command = rhei_command(&plan.home);
    command.arg("--state-machine").arg(&plan.machine).arg("run").arg(&plan.plan).args(args);
    bounded_within(command, &format!("rhei run {}", args.join(" ")), plan.root(), HALT_PATIENCE)
}

/// The run came back (the guard is the first half), spawned no more than the
/// budget, and printed its refusal of the spent visit at most once rather than
/// once per rescan.
#[track_caller]
fn assert_halted_on_the_spent_budget(plan: &IdlePlan, result: &CliRun, what: &str) {
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(spawns(plan) <= 2, "{what}: no spawn past the two-attempt budget; got:\n{combined}");
    let refusals = combined.matches("attempts spent on this visit").count();
    assert!(refusals <= 1, "{what}: the refusal is not rescanned; got {refusals}:\n{combined}");
    assert!(
        !combined.contains("rhei resumes it automatically"),
        "{what}: no clock releases a spent visit; got:\n{combined}"
    );
}

const TWO_TICKETS: &str =
    "### Task 1: Stalls on a missing output\n**State:** work\n\n### Task 2: Needs a brief\n**State:** needs\n";

/// The expired record sits on another ticket, one missing its input, on the
/// same execution identity. The control, without it, halts after one spawn.
#[test]
fn another_tickets_expired_record_does_not_keep_a_spent_visit_waiting() {
    let control = spent_plan("spent-budget-control", "", TWO_TICKETS, Attempt::WritesNothing);
    let result = run_within(&control, &["--no-tui"]);
    assert_ne!(result.status.code(), Some(0), "the control halts for attention");
    assert_eq!(spawns(&control), 1, "the control halts after its one stalled attempt");

    let records = expired_record("2", "needs");
    let plan = spent_plan("spent-budget-two", &records, TWO_TICKETS, Attempt::WritesNothing);
    let result = run_within(&plan, &["--no-tui"]);
    assert_ne!(result.status.code(), Some(0), "the run halts for attention");
    assert_halted_on_the_spent_budget(&plan, &result, "a record on another ticket");
}

/// The expired record is the spent ticket's own: a failed attempt does not
/// clear it, and it still releases nothing once the attempts are gone.
#[test]
fn a_spent_visits_own_expired_record_does_not_keep_it_waiting() {
    let records = expired_record("1", "work");
    let tasks = "### Task 1: Fails every attempt\n**State:** work\n";
    let plan = spent_plan("spent-budget-own-record", &records, tasks, Attempt::Fails);
    let result = run_within(&plan, &["--no-tui", "--continue-on-error"]);
    assert_halted_on_the_spent_budget(&plan, &result, "the ticket's own record");
}

/// A supervisor over a gated child is ready while that subtree is open; once its
/// visit's attempts are spent, its own expired record is no deadline either.
// §FS-rhei-supervision.3.1
#[test]
fn a_supervisors_expired_record_does_not_keep_its_spent_visit_waiting() {
    let records = expired_record("1", "supervising");
    let tasks = "### Task 1: Supervisor\n**State:** supervising\n\n#### Task 1.1: Child\n**State:** review\n";
    let plan = spent_plan("spent-budget-supervisor", &records, tasks, Attempt::Fails);
    let result = run_within(&plan, &["--no-tui", "--continue-on-error"]);
    assert_halted_on_the_spent_budget(&plan, &result, "a supervisor's record");
}
