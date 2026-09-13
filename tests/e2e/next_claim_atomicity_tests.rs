//! The command-level boundary between choosing a task and committing its claim.

use std::fs;

use super::*;

const CLAIM_PLAN: &str = r#"# Rhei: Atomic Claim

---
metadata:
  tasks:
    1:
      priority: 7
---

## Tasks

### Task 1: Claim me
**State:** draft
"#;

const CLAIM_MACHINE: &str = r#"name: atomic-claim
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Ready for work
    visits: 2
    instructions: Do the task.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: pending
    condition: visitCount < visits
  - from: pending
    to: completed
    condition: visitCount >= visits
"#;

fn obstructed_claim(prefix: &str) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", CLAIM_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", CLAIM_MACHINE);
    fs::write(dir.join("runtime"), "a regular file blocks the transition ledger\n")
        .expect("write runtime obstruction");
    (dir, plan_path, machine_path)
}

fn record(violations: &mut Vec<String>, condition: bool, message: impl Into<String>) {
    if !condition {
        violations.push(message.into());
    }
}

/// A returned ledger error is before the claim boundary: task and counted-state
/// metadata stay byte-identical, retry reaches the same error, and removing the
/// obstruction permits one complete claim without `reset`.
// §FS-rhei-next.3.1 §REQ-cross-platform.2 §REQ-cross-platform.4
#[test]
fn next_auto_advance_claim_restores_and_retries_after_ledger_failure() {
    let (dir, plan_path, machine_path) = obstructed_claim("next-claim-ledger-failure");
    let original = fs::read_to_string(&plan_path).expect("read original plan");

    let first = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    let after_first = fs::read_to_string(&plan_path).expect("read plan after first claim");
    let transition_path = dir.join("runtime/state-transitions.log");
    let first_has_transition = transition_path.exists();

    let second = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    let after_second = fs::read_to_string(&plan_path).expect("read plan after second claim");

    fs::remove_file(dir.join("runtime")).expect("remove runtime obstruction");
    let recovered = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    let after_recovery = fs::read_to_string(&plan_path).expect("read claimed plan");
    let ledger = fs::read_to_string(&transition_path).unwrap_or_default();

    let mut violations = Vec::new();
    record(&mut violations, !first.status.success(), "first claim unexpectedly succeeded");
    record(
        &mut violations,
        first.stderr.contains("failed to create runtime directory"),
        "first claim did not report the runtime-directory failure",
    );
    record(
        &mut violations,
        after_first == original,
        "first failed claim changed the task or counted-state metadata",
    );
    record(
        &mut violations,
        after_first.contains("**State:** draft") && !after_first.contains("**Assignee:**"),
        "first failed claim did not preserve the original state and ownership",
    );
    record(&mut violations, !first_has_transition, "first failed claim created a transition entry");
    record(&mut violations, !second.status.success(), "second claim unexpectedly succeeded");
    record(
        &mut violations,
        second.stderr.contains("failed to create runtime directory"),
        "second claim did not reach the same underlying runtime failure",
    );
    record(
        &mut violations,
        !second.stderr.contains("mid-workflow"),
        "second claim was stranded in a mid-workflow state",
    );
    record(
        &mut violations,
        after_second == original,
        "second failed claim changed the task or counted-state metadata",
    );
    record(
        &mut violations,
        recovered.status.success(),
        "claim did not succeed after removing the obstruction without reset",
    );
    record(
        &mut violations,
        recovered.stdout.contains("Task plan.1 claimed: 'draft' -> 'pending'"),
        "successful text output did not report the committed state transition",
    );
    record(
        &mut violations,
        after_recovery.contains("**State:** pending\n**Assignee:** manual"),
        "successful claim did not persist its counted state and manual assignee together",
    );
    record(
        &mut violations,
        after_recovery.contains("stateVisits:\n        pending: 1"),
        "successful claim did not persist counted-state metadata",
    );
    record(
        &mut violations,
        ledger == "plan.1 draft@pending\n",
        "successful claim did not leave exactly one transition entry",
    );

    assert!(
        violations.is_empty(),
        "{}\n\nfirst exit: {:?}\nfirst stdout:\n{}\nfirst stderr:\n{}\n\
         plan before:\n{}\nplan after first failure:\n{}\n\
         second exit: {:?}\nsecond stdout:\n{}\nsecond stderr:\n{}\n\
         plan after second failure:\n{}\nrecovery exit: {:?}\nrecovery stdout:\n{}\n\
         recovery stderr:\n{}\nplan after recovery:\n{}\nledger:\n{}",
        violations.join("\n"),
        first.status.code(),
        first.stdout,
        first.stderr,
        original,
        after_first,
        second.status.code(),
        second.stdout,
        second.stderr,
        after_second,
        recovered.status.code(),
        recovered.stdout,
        recovered.stderr,
        after_recovery,
        ledger,
    );
}

/// Effective-target inputs are checked before either half of the claim is
/// persisted, including when the initial-state scan had no inputs to check.
// §FS-rhei-next.3.1 §FS-rhei-transitions.4.5
#[test]
fn next_auto_advance_claim_target_input_refusal_leaves_no_state_owner_or_ledger() {
    let plan = r#"# Rhei: Target Input Claim

## Tasks

### Task 1: Apply findings
**State:** draft
"#;
    let machine = r#"name: target-input-claim
version: 1
states:
  draft:
    initial: true
    description: Setup only
  fix:
    description: Apply findings
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: fix
  - from: fix
    to: completed
"#;
    let dir = unique_temp_dir("next-claim-target-input");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);

    assert!(!result.status.success(), "missing target input must refuse the claim");
    assert_stderr_contains(&result, "Task plan.1 cannot enter state fix.");
    assert_stderr_contains(
        &result,
        "Missing required input artifact: findings (runtime/findings/plan.1.md)",
    );
    assert_eq!(fs::read_to_string(&plan_path).expect("read plan"), plan);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// A leave callback rejects before persistence, so the next-specific claim
/// path adds neither ownership nor a claim ledger record.
// §FS-rhei-next.3.1 §FS-rhei-transitions.4.9
#[test]
fn next_auto_advance_claim_leave_rejection_leaves_no_state_owner_or_ledger() {
    let callback = python_callback_yaml(
        "import json,sys;sys.stdout.write(json.dumps({'success': False, 'error': 'not ready'}))",
    );
    let machine = format!(
        r#"name: leave-rejection-claim
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Ready
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
    on_leave: {callback}
  - from: pending
    to: completed
"#
    );
    let plan = r#"# Rhei: Leave Rejection Claim

## Tasks

### Task 1: Claim me
**State:** draft
"#;
    let dir = unique_temp_dir("next-claim-leave-rejection");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("next", &plan_path, &machine_path, &[]);

    assert!(!result.status.success(), "leave rejection must refuse the claim");
    assert_stderr_contains(&result, "on_leave callback");
    assert_stderr_contains(&result, "not ready");
    assert_eq!(fs::read_to_string(&plan_path).expect("read plan"), plan);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// A non-terminal redirect is the effective claim state: its agent supplies
/// the assignee and JSON identity, and the ledger records that target.
// §FS-rhei-next.3.1
#[test]
fn next_auto_advance_claim_redirect_resolves_effective_state_assignee() {
    let callback = python_callback_yaml(
        "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'review'}))",
    );
    let machine = format!(
        r#"name: redirected-claim
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Default work
    agent: claude-code
  review:
    description: Redirected work
    agent: codex
    instructions: Review the task.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
    on_leave: {callback}
  - from: draft
    to: review
  - from: pending
    to: completed
  - from: review
    to: completed
"#
    );
    let plan = r#"# Rhei: Redirected Claim

## Tasks

### Task 1: Review me
**State:** draft
"#;
    let dir = unique_temp_dir("next-claim-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--json"]);

    assert_success(&result);
    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["from_state"], "draft");
    assert_eq!(json["state"], "review");
    assert_eq!(json["agent"], "codex");
    assert_eq!(json["claimed_as"], "codex");
    let persisted = fs::read_to_string(&plan_path).expect("read claimed plan");
    assert!(persisted.contains("**State:** review\n**Assignee:** codex"));
    let ledger = fs::read_to_string(dir.join("runtime/state-transitions.log"))
        .expect("read transition ledger");
    assert_eq!(ledger, "plan.1 draft@review\n");
}

/// `on_enter` sees the target state, but its ordinary failure restores the
/// source before `next` can persist an assignee or claim ledger entry.
// §FS-rhei-next.3.1 §FS-rhei-transitions.4.9
#[test]
fn next_auto_advance_claim_enter_failure_restores_before_ownership() {
    let callback = python_callback_yaml("import sys;sys.exit(1)");
    let machine = format!(
        r#"name: enter-failure-claim
version: 1
states:
  draft:
    initial: true
    description: Setup only
  pending:
    description: Ready
    agent: codex
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
    on_enter: {callback}
  - from: pending
    to: completed
"#
    );
    let plan = r#"# Rhei: Enter Failure Claim

## Tasks

### Task 1: Claim me
**State:** draft
"#;
    let dir = unique_temp_dir("next-claim-enter-failure");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("next", &plan_path, &machine_path, &[]);

    assert!(!result.status.success(), "enter failure must refuse the claim");
    assert_stderr_contains(&result, "on_enter callback");
    assert_eq!(fs::read_to_string(&plan_path).expect("read plan"), plan);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// JSON reports the same state and ownership that an independently retried
/// claim commits; it does not need to claim the text test's task a second time.
// §FS-rhei-next.3.1 §REQ-cross-platform.3 §REQ-cross-platform.4
#[test]
fn next_auto_advance_claim_json_matches_committed_state_and_assignee_after_retry() {
    let (dir, plan_path, machine_path) = obstructed_claim("next-claim-ledger-json");
    let original = fs::read_to_string(&plan_path).expect("read original plan");

    let failed = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    let after_failure = fs::read_to_string(&plan_path).expect("read plan after failed claim");
    fs::remove_file(dir.join("runtime")).expect("remove runtime obstruction");
    let claimed = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    let persisted = fs::read_to_string(&plan_path).expect("read claimed plan");
    let ledger = fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap_or_default();

    let json = serde_json::from_str::<serde_json::Value>(&claimed.stdout).ok();
    let mut violations = Vec::new();
    record(
        &mut violations,
        !failed.status.success() && failed.stderr.contains("failed to create runtime directory"),
        "setup did not exercise the expected ledger failure",
    );
    record(
        &mut violations,
        after_failure == original,
        "failed JSON-mode claim changed the task or counted-state metadata",
    );
    record(
        &mut violations,
        claimed.status.success(),
        "JSON-mode claim did not succeed after removing the obstruction",
    );
    record(
        &mut violations,
        json.as_ref().is_some_and(|value| {
            value["from_state"] == "draft"
                && value["state"] == "pending"
                && value["claimed_as"] == "manual"
        }),
        "JSON output did not match the committed state and assignee",
    );
    record(
        &mut violations,
        persisted.contains("**State:** pending\n**Assignee:** manual"),
        "JSON-mode claim did not persist the reported state and assignee together",
    );
    record(
        &mut violations,
        ledger == "plan.1 draft@pending\n",
        "JSON-mode claim did not leave exactly one transition entry",
    );

    assert!(
        violations.is_empty(),
        "{}\n\nfailed exit: {:?}\nfailed stdout:\n{}\nfailed stderr:\n{}\n\
         plan before:\n{}\nplan after failure:\n{}\nclaimed exit: {:?}\nclaimed stdout:\n{}\n\
         claimed stderr:\n{}\npersisted plan:\n{}\nledger:\n{}",
        violations.join("\n"),
        failed.status.code(),
        failed.stdout,
        failed.stderr,
        original,
        after_failure,
        claimed.status.code(),
        claimed.stdout,
        claimed.stderr,
        persisted,
        ledger,
    );
}
