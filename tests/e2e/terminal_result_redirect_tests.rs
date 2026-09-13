//! Where a `nextState` redirect lands, and what the result obligation says
//! about it.
//!
//! A redirect changes the edge after the caller named one, so the check has to
//! be re-run against the state actually entered: a callback must not be able to
//! route a ticket into a terminal state the caller never asked for and skip the
//! result with it, and a redirect to a non-terminal state must still keep the
//! caller's message for the eventual terminal edge.

// §FS-rhei-transition-cmd.3.2 §FS-rhei-complete.4 §FS-rhei-states.3.3

use std::fs;

use super::terminal_result_tests::{ONE_TASK_PLAN, RESULT_MESSAGE};
use super::*;

/// A `nextState` redirect is re-checked against the effective target, so a
/// callback cannot route a ticket into a terminal state the caller never asked
/// for and skip the result with it. §FS-rhei-transition-cmd.3.2
#[test]
fn a_callback_redirect_cannot_smuggle_a_terminal_entry_past_the_check() {
    let machine = format!(
        r#"name: redirect-terminal-result
version: 1
states:
  pending:
    initial: true
    description: Not started
  in-progress:
    description: Working
  rejected:
    final: true
    description: Rejected outright
transitions:
  - from: pending
    to: in-progress
    on_leave: {callback}
  - from: pending
    to: rejected
  - from: in-progress
    to: rejected
"#,
        callback = python_callback_yaml(
            "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'rejected'}))"
        )
    );
    let dir = unique_temp_dir("terminal-result-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    // The requested target is non-terminal; only the redirect is terminal.
    let result = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "in-progress"],
    );
    assert!(
        !result.status.success(),
        "the redirect must be held to the same rule\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "cannot enter terminal state 'rejected' without a result");
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    // With a message, the same redirect goes through and records it.
    assert_success(&run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "in-progress", "--result", RESULT_MESSAGE],
    ));
    assert_task_state(&plan_path, &machine_path, "1", "rejected");
    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains(RESULT_MESSAGE),
        "the redirect records the message; got:\n{recorded}"
    );
}

/// `rhei complete` whose `on_leave` redirects to a **non-terminal** state: the
/// move happened, so the ledger has it and the caller's message rides with it,
/// and `complete` still exits non-zero because the ticket is not finished.
///
/// The recorded message then satisfies the obligation at the eventual terminal
/// edge, exactly as any earlier `transition --result` on the same ticket does.
// §FS-rhei-complete.4 §FS-rhei-states.3.3
#[test]
fn complete_redirected_to_a_non_terminal_state_still_records_the_message() {
    let machine = format!(
        r#"name: redirect-non-terminal
version: 1
states:
  pending:
    initial: true
    description: Not started
  review:
    description: Sent back for review
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
    on_leave: {callback}
  - from: pending
    to: review
  - from: review
    to: completed
"#,
        callback = python_callback_yaml(
            "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'review'}))"
        )
    );
    let dir = unique_temp_dir("terminal-result-complete-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", RESULT_MESSAGE],
    );
    assert!(
        !result.status.success(),
        "the caller asked to finish a ticket the machine sent elsewhere\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // The move is the machine's decision and it stands, message included.
    assert_task_state(&plan_path, &machine_path, "1", "review");
    let history =
        fs::read_to_string(dir.join("runtime/state-transitions.log")).expect("read ledger");
    assert_eq!(history, "plan.1 pending@review\n");
    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert_eq!(recorded, format!("## Result\n\n{RESULT_MESSAGE}\n\n"));

    // And it pre-satisfies the obligation at the real terminal edge.
    assert_success(&run_transition(&plan_path, &machine_path, "1", "review", "completed"));
    assert_task_state(&plan_path, &machine_path, "1", "completed");
}

/// `rhei next`'s auto-advance out of a setup-only initial state never *declares*
/// an edge into a terminal state, but an `on_leave` redirect can still put one
/// there. The shared path refuses it and the plan is left untouched: `next`
/// claims work, it does not finish it.
// §FS-rhei-next.3 §FS-rhei-states.3.3
#[test]
fn next_auto_advance_redirected_into_a_terminal_state_is_refused_cleanly() {
    let machine = format!(
        r#"name: next-redirect-terminal
version: 1
states:
  planning:
    initial: true
    description: Setup only
  pending:
    description: Ready for work
  completed:
    final: true
    description: Done
transitions:
  - from: planning
    to: pending
    on_leave: {callback}
  - from: planning
    to: completed
  - from: pending
    to: completed
"#,
        callback = python_callback_yaml(
            "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'completed'}))"
        )
    );
    let plan = r#"# Rhei: Next Redirect

## Tasks

### Task 1: Do the work
**State:** planning
"#;
    let dir = unique_temp_dir("terminal-result-next-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("next", &plan_path, &machine_path, &[]);
    assert!(
        !result.status.success(),
        "a claim must not finish the ticket by redirect\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "cannot enter terminal state 'completed' without a result");
    assert_task_state(&plan_path, &machine_path, "1", "planning");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "a refused claim must not create the result file"
    );
}

/// A result left by an earlier visit cannot satisfy away `next`'s stronger
/// rule: claiming never redirects into a terminal state. Every existing byte
/// remains untouched on refusal. §FS-rhei-next.3.1
#[test]
fn next_terminal_redirect_is_refused_even_when_a_result_already_exists() {
    let machine = format!(
        r#"name: next-redirect-terminal-with-result
version: 1
states:
  planning:
    initial: true
    description: Setup only
  pending:
    description: Ready for work
  completed:
    final: true
    description: Done
transitions:
  - from: planning
    to: pending
    on_leave: {callback}
  - from: planning
    to: completed
  - from: pending
    to: completed
"#,
        callback = python_callback_yaml(
            "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'completed'}))"
        )
    );
    let plan = r#"# Rhei: Next Redirect With Result

---
metadata:
  tasks:
    1:
      priority: 4
---

## Tasks

### Task 1: Do the work
**State:** planning
"#;
    let dir = unique_temp_dir("terminal-result-next-redirect-existing-result");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    let runtime = dir.join("runtime");
    let results = runtime.join("results");
    fs::create_dir_all(&results).expect("results directory");
    let result_path = results.join("plan.1.md");
    let result_contents = "## Result\n\nEarlier work.\n";
    fs::write(&result_path, result_contents).expect("existing result");
    let ledger_path = runtime.join("state-transitions.log");
    let ledger_contents = "plan.9 pending@completed\n";
    fs::write(&ledger_path, ledger_contents).expect("existing ledger");

    let result = run_cli("next", &plan_path, &machine_path, &[]);

    assert!(!result.status.success(), "a claim must not finish the ticket");
    assert_stderr_contains(
        &result,
        "Task plan.1 cannot enter terminal state 'completed' during a claim",
    );
    let persisted = fs::read_to_string(&plan_path).expect("persisted plan");
    assert_eq!(persisted, plan, "task and counted metadata must be unchanged");
    assert!(!persisted.contains("**Assignee:**"), "original lack of ownership must remain");
    assert_eq!(fs::read_to_string(result_path).unwrap(), result_contents);
    assert_eq!(fs::read_to_string(ledger_path).unwrap(), ledger_contents);
}
