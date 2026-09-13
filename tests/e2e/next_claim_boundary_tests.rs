//! Claim boundaries that need behavior beyond the core failure fixture.

use std::fs;

use super::*;

/// After claim restoration, a configured recovery transition keeps only its
/// own state and ledger line; it does not acquire claim ownership.
// §FS-rhei-next.3.1 §FS-rhei-transitions.4.9
#[test]
fn next_auto_advance_claim_enter_failure_keeps_declared_recovery_only() {
    let callback = python_callback_yaml("import sys;sys.exit(1)");
    let machine = format!(
        r#"name: enter-recovery-claim
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Ready
  retrying:
    description: Recovering
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
    on_enter: {callback}
  - from: draft
    to: retrying
  - from: pending
    to: completed
  - from: retrying
    to: completed
error_handling:
  on_enter_failure:
    - transition_to: retrying
"#
    );
    let plan = r#"# Rhei: Enter Recovery Claim

## Tasks

### Task 1: Claim me
**State:** draft
"#;
    let dir = unique_temp_dir("next-claim-enter-recovery");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("next", &plan_path, &machine_path, &[]);

    assert!(!result.status.success(), "failed enter remains a failed claim");
    assert_stderr_contains(&result, "configured on-enter recovery transitioned");
    let persisted = fs::read_to_string(&plan_path).expect("read recovered plan");
    assert!(persisted.contains("**State:** retrying"));
    assert!(!persisted.contains("**Assignee:**"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.1 draft@retrying\n"
    );
}

/// Prompt assembly follows the durable boundary: a later supervisor-brief read
/// error reports failure without undoing state, owner, or ledger. §FS-rhei-next.3.1
#[test]
fn next_auto_advance_claim_rendering_failure_keeps_committed_claim() {
    let plan = r#"# Rhei: Post-commit Rendering

## Tasks

### Task 1: Claim me
**State:** draft
"#;
    let machine = r#"name: post-commit-rendering
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Ready
    visits: 2
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;
    let dir = unique_temp_dir("next-claim-post-commit-render");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    fs::create_dir_all(dir.join("runtime/supervise/plan.1.md"))
        .expect("create unreadable-as-text brief path");

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);

    assert!(!result.status.success(), "brief rendering should fail");
    assert_stderr_contains(&result, "failed to read supervisor brief");
    let persisted = fs::read_to_string(&plan_path).expect("read claimed plan");
    assert!(persisted.contains("**State:** pending\n**Assignee:** manual"));
    assert!(persisted.contains("stateVisits:\n        pending: 1"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.1 draft@pending\n"
    );
}

/// A task already runnable in its initial state takes ownership in one atomic
/// task rewrite and does not invent a self-transition ledger line.
// §FS-rhei-next.3.1
#[test]
fn next_initial_runnable_claim_writes_only_assignee() {
    let plan = r#"# Rhei: Runnable Initial Claim

## Tasks

### Task 1: Claim me
**State:** pending
"#;
    let machine = r#"name: runnable-initial-claim
version: 1
states:
  pending:
    initial: true
    description: Ready
    instructions: Do the task.
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;
    let dir = unique_temp_dir("next-claim-runnable-initial");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);

    assert_success(&result);
    assert!(result.stdout.contains("Task plan.1 claimed by manual (stays in 'pending')"));
    assert_eq!(
        fs::read_to_string(&plan_path).unwrap(),
        plan.replace("**State:** pending", "**State:** pending\n**Assignee:** manual")
    );
    assert!(!dir.join("runtime/state-transitions.log").exists());
}
