//! Validation of the explicit supervisor operation context at the CLI boundary.
//!
//! These are transition-command cases, not metadata-helper cases: the refusal
//! must happen while the locked, re-read plan is still byte-for-byte intact.

// §FS-rhei-transition-cmd.2 §FS-rhei-transition-cmd.3

use std::fs;
use std::path::Path;

use super::supervision_tests::setup_supervision;
use super::*;

const NESTED_PLAN: &str = r#"# Rhei: Supervisor mismatch

---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Outer supervisor
**State:** supervising

#### Task 1.1: Inner supervisor
**State:** supervising

##### Task 1.1.1: Leaf work
**State:** review
"#;

const NO_ANCESTOR_PLAN: &str = r#"# Rhei: No supervisor

## Tasks

### Task 1: Moving task
**State:** review

### Task 2: Real but unrelated task
**State:** review
"#;

const SUPERVISOR_MACHINE: &str = r#"name: supervisor-validation
version: 1
states:
  supervising:
    description: Supervise descendants
    execute_on: descendant-terminal
    agent: mock
    visits: 12
  review:
    description: Review
    agent: mock
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: supervising, description: Release descendants }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: review, to: completed, description: Review done }
  - { from: "*", to: cancelled, description: Drop work }
"#;

fn transition_with_supervisor(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
    supervisor: &str,
) -> CliRun {
    run_cli(
        "transition",
        plan_path,
        machine_path,
        &[
            "--task",
            task,
            "--from",
            from,
            "--to",
            to,
            "--result",
            "finished the leaf",
            "--supervisor",
            supervisor,
            "--no-callbacks",
        ],
    )
}

/// Assert the whole refusal as one observation, so a pre-fix success reports
/// its stdout, diagnostic, changed plan, result, ledger, and checkpoint rather
/// than stopping after the exit-status assertion.
fn assert_refused_without_effects(
    dir: &Path,
    plan_path: &Path,
    before: &str,
    result: &CliRun,
    result_task: &str,
    diagnostic_fragments: &[&str],
) {
    let after = fs::read_to_string(plan_path).expect("read plan after transition");
    let result_path = dir.join(format!("runtime/results/{result_task}.md"));
    let ledger_path = dir.join("runtime/state-transitions.log");
    let diagnostics_match =
        diagnostic_fragments.iter().all(|fragment| result.stderr.contains(fragment));
    let refused_cleanly = !result.status.success()
        && diagnostics_match
        && after == before
        && !result_path.exists()
        && !ledger_path.exists()
        && !after.contains("checkpoints:");

    assert!(
        refused_cleanly,
        "expected a supervisor refusal with no effects\n\
         exit: {}\nstdout:\n{}\nstderr:\n{}\n\
         plan unchanged: {}\nresult exists: {}\nledger exists: {}\ncheckpoint present: {}\n\
         before plan:\n{}\nafter plan:\n{}",
        result.status,
        result.stdout,
        result.stderr,
        after == before,
        result_path.exists(),
        ledger_path.exists(),
        after.contains("checkpoints:"),
        before,
        after,
    );
}

/// A real task id is not valid merely because it resolves: it has to be the
/// moving leaf's nearest in-scope supervising ancestor. The rejected command
/// leaves every durable transition surface untouched.
// §FS-rhei-transition-cmd.2 §FS-rhei-transition-cmd.3
#[test]
fn a_real_but_wrong_supervisor_is_refused_without_transition_effects() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-mismatch", NESTED_PLAN, SUPERVISOR_MACHINE, "");
    let before = fs::read_to_string(&plan_path).expect("read original plan");

    let result =
        transition_with_supervisor(&plan_path, &machine_path, "1.1.1", "review", "completed", "1");

    assert_refused_without_effects(
        &dir,
        &plan_path,
        &before,
        &result,
        "plan.1.1.1",
        &[
            "Task plan.1.1.1 cannot use --supervisor plan.1",
            "nearest in-scope supervising ancestor is Task plan.1.1",
            "pass --supervisor plan.1.1 or omit the flag",
        ],
    );
}

/// With no in-scope supervising ancestor there is no valid explicit value,
/// even when the supplied id names another real task.
// §FS-rhei-transition-cmd.2
#[test]
fn a_real_supervisor_value_is_refused_when_the_task_has_no_supervising_ancestor() {
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervisor-validation-no-ancestor",
        NO_ANCESTOR_PLAN,
        SUPERVISOR_MACHINE,
        "",
    );
    let before = fs::read_to_string(&plan_path).expect("read original plan");

    let result =
        transition_with_supervisor(&plan_path, &machine_path, "1", "review", "completed", "2");

    assert_refused_without_effects(
        &dir,
        &plan_path,
        &before,
        &result,
        "plan.1",
        &[
            "Task plan.1 cannot use --supervisor plan.2",
            "has no in-scope supervising ancestor",
            "omit --supervisor",
        ],
    );
}

/// Compare-and-swap remains the first locked relationship check: a stale
/// caller learns the task's current state before its wrong supervisor matters.
// §FS-rhei-transition-cmd.3
#[test]
fn stale_from_retains_precedence_over_supervisor_validation() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-stale-from", NESTED_PLAN, SUPERVISOR_MACHINE, "");
    let before = fs::read_to_string(&plan_path).expect("read original plan");

    let result =
        transition_with_supervisor(&plan_path, &machine_path, "1.1.1", "completed", "review", "1");

    assert!(!result.status.success(), "a stale --from must fail");
    assert_stderr_contains(
        &result,
        "conflict: Task plan.1.1.1 is in state 'review', expected 'completed'",
    );
    assert!(
        !result.stderr.contains("cannot use --supervisor"),
        "compare-and-swap must speak first:\n{}",
        result.stderr
    );
    assert_eq!(fs::read_to_string(&plan_path).expect("read plan"), before);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// Relationship validation precedes callbacks and source-artifact checks. The
/// callback marker makes that ordering externally visible even though the
/// missing output would eventually refuse the old path before its state write.
// §FS-rhei-transition-cmd.3
#[test]
fn supervisor_validation_precedes_callbacks_and_artifact_checks() {
    let machine = format!(
        r#"name: supervisor-validation-order
version: 1
states:
  supervising:
    description: Supervise descendants
    execute_on: descendant-terminal
    agent: mock
    visits: 12
  review:
    description: Review
    agent: mock
    outputs:
      - name: findings
        path: runtime/findings.md
  completed:
    description: Done
    final: true
transitions:
  - {{ from: supervising, to: supervising, description: Release descendants }}
  - {{ from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }}
  - from: review
    to: completed
    on_leave: {callback}
"#,
        callback = python_callback_yaml(
            "from pathlib import Path; Path('callback-ran').write_text('ran')"
        )
    );
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-order", NESTED_PLAN, &machine, "");
    let before = fs::read_to_string(&plan_path).expect("read original plan");

    let result = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &[
            "--task",
            "1.1.1",
            "--from",
            "review",
            "--to",
            "completed",
            "--result",
            "finished",
            "--supervisor",
            "1",
        ],
    );

    assert!(
        !dir.join("callback-ran").exists(),
        "supervisor validation must happen before on_leave; got:\n{}",
        result.stderr
    );
    assert_refused_without_effects(
        &dir,
        &plan_path,
        &before,
        &result,
        "plan.1.1.1",
        &["Task plan.1.1.1 cannot use --supervisor plan.1"],
    );
}

/// Once compare-and-swap succeeds, the supervisor relationship is the first
/// transition-specific question: an inapplicable edge cannot mask it.
// §FS-rhei-transition-cmd.3
#[test]
fn supervisor_validation_precedes_edge_and_condition_evaluation() {
    let machine = r#"name: supervisor-validation-condition-order
version: 1
states:
  supervising:
    description: Supervise descendants
    execute_on: descendant-terminal
    agent: mock
    visits: 12
  review:
    description: Review
    agent: mock
    visits: 2
  completed:
    description: Done
    final: true
transitions:
  - { from: supervising, to: supervising, description: Release descendants }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: review, to: completed, description: Review done, condition: visitCount >= visits }
"#;
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-condition-order", NESTED_PLAN, machine, "");
    let before = fs::read_to_string(&plan_path).expect("read original plan");
    let result =
        transition_with_supervisor(&plan_path, &machine_path, "1.1.1", "review", "completed", "1");

    assert_refused_without_effects(
        &dir,
        &plan_path,
        &before,
        &result,
        "plan.1.1.1",
        &[
            "Task plan.1.1.1 cannot use --supervisor plan.1",
            "nearest in-scope supervising ancestor is Task plan.1.1",
        ],
    );
}

fn assert_correct_supervisor_suppresses(prefix: &str, supervisor: &str) {
    let (correct_dir, correct_plan, correct_machine) =
        setup_supervision(prefix, NESTED_PLAN, SUPERVISOR_MACHINE, "");
    let correct = transition_with_supervisor(
        &correct_plan,
        &correct_machine,
        "1.1.1",
        "review",
        "completed",
        supervisor,
    );
    assert_success(&correct);
    let correct_after = fs::read_to_string(&correct_plan).expect("read correct plan");
    assert!(
        !correct_after.contains("checkpoints:"),
        "the correct supervisor suppresses its own checkpoint:\n{correct_after}"
    );
    assert!(correct_dir.join("runtime/results/plan.1.1.1.md").exists());
    assert_eq!(
        fs::read_to_string(correct_dir.join("runtime/state-transitions.log"))
            .expect("correct transition ledger"),
        "plan.1.1.1 review@completed\n"
    );
}

/// A correct rhei-local value keeps the flag's defining effect: the command
/// succeeds while suppressing the checkpoint it would deliver to that owner.
// §FS-rhei-transition-cmd.2
#[test]
fn a_correct_local_supervisor_suppresses_its_checkpoint() {
    assert_correct_supervisor_suppresses("supervisor-validation-correct-local", "1.1");
}

/// Project-qualified ticket targeting names the same correct owner and has the
/// same suppression behavior as its unambiguous rhei-local spelling.
// §FS-rhei-transition-cmd.2
#[test]
fn a_correct_qualified_supervisor_suppresses_its_checkpoint() {
    assert_correct_supervisor_suppresses("supervisor-validation-correct-qualified", "plan.1.1");
}

/// Omitting the operation context remains an ordinary transition and delivers
/// the matching checkpoint to the nearest owner.
// §FS-rhei-transition-cmd.2
#[test]
fn omitting_supervisor_delivers_the_nearest_owners_checkpoint() {
    let (omitted_dir, omitted_plan, omitted_machine) =
        setup_supervision("supervisor-validation-omitted", NESTED_PLAN, SUPERVISOR_MACHINE, "");
    let omitted = run_cli(
        "transition",
        &omitted_plan,
        &omitted_machine,
        &[
            "--task",
            "1.1.1",
            "--from",
            "review",
            "--to",
            "completed",
            "--result",
            "finished the leaf",
            "--no-callbacks",
        ],
    );
    assert_success(&omitted);
    let omitted_after = fs::read_to_string(&omitted_plan).expect("read omitted plan");
    assert!(
        omitted_after.contains(
            "checkpoints:\n        - task: 1.1.1\n          from: review\n          to: completed"
        ),
        "omitting the context delivers the inner checkpoint:\n{omitted_after}"
    );
    assert!(omitted_dir.join("runtime/results/plan.1.1.1.md").exists());
}

/// A value that does not resolve remains an ordinary ticket-target error and
/// produces no transition effect.
// §FS-rhei-transition-cmd.2
#[test]
fn an_unknown_supervisor_retains_the_task_not_found_error() {
    let (unknown_dir, unknown_plan, unknown_machine) =
        setup_supervision("supervisor-validation-unknown", NESTED_PLAN, SUPERVISOR_MACHINE, "");
    let unknown_before = fs::read_to_string(&unknown_plan).expect("read unknown plan");
    let unknown = transition_with_supervisor(
        &unknown_plan,
        &unknown_machine,
        "1.1.1",
        "review",
        "completed",
        "404",
    );
    assert!(!unknown.status.success(), "unknown supervisor must fail");
    assert_stderr_contains(&unknown, "task '404' not found");
    assert_eq!(fs::read_to_string(&unknown_plan).expect("read plan"), unknown_before);
    assert!(!unknown_dir.join("runtime/state-transitions.log").exists());
}
