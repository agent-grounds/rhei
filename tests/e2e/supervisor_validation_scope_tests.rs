//! Scope-first owner selection for explicit supervisor validation.

// §FS-rhei-transition-cmd.2 §FS-rhei-supervision.2.2

use std::fs;

use super::supervision_tests::setup_supervision;
use super::*;

const SCOPE_PLAN: &str = r#"# Rhei: Scope-first supervisor

---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Outer descendant supervisor
**State:** supervising

#### Task 1.1: Near child supervisor
**State:** watching

##### Task 1.1.1: Intermediate task
**State:** review

###### Task 1.1.1.1: Deep leaf
**State:** review
"#;

const SCOPE_MACHINE: &str = r#"name: supervisor-validation-scope
version: 1
states:
  supervising:
    description: Watch all descendants finish
    execute_on: descendant-terminal
    agent: mock
    visits: 12
  watching:
    description: Watch children finish
    execute_on: child-terminal
    agent: mock
    visits: 12
  review:
    description: Review
    agent: mock
  completed:
    description: Done
    final: true
transitions:
  - { from: supervising, to: supervising, description: Release descendants }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: watching, to: watching, description: Release children }
  - { from: watching, to: completed, description: Children done, condition: openDescendants < 1 }
  - { from: review, to: completed, description: Reviewed }
"#;

const EVENT_PLAN: &str = r#"# Rhei: Event-filtered supervisor

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Outer transition supervisor
**State:** supervising

#### Task 1.1: Inner terminal supervisor
**State:** watching

##### Task 1.1.1: Leaf
**State:** review
"#;

const EVENT_MACHINE: &str = r#"name: supervisor-validation-event
version: 1
states:
  supervising:
    description: Watch descendant transitions
    execute_on: descendant-transition
    agent: mock
    visits: 12
  watching:
    description: Watch child terminal entries
    execute_on: child-terminal
    agent: mock
    visits: 12
  review:
    description: Review
    agent: mock
  fix:
    description: Fix
    agent: mock
  completed:
    description: Done
    final: true
transitions:
  - { from: supervising, to: supervising, description: Release descendants }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: watching, to: watching, description: Release children }
  - { from: watching, to: completed, description: Children done, condition: openDescendants < 1 }
  - { from: review, to: fix, description: Needs fixes }
  - { from: fix, to: completed, description: Fixed }
"#;

/// A nearer `child-*` supervisor excludes a grandchild, so validation must
/// climb to the outer `descendant-*` owner rather than accepting the nearer id.
// §FS-rhei-transition-cmd.2 §FS-rhei-supervision.2.2
#[test]
fn an_out_of_scope_nearer_supervisor_is_refused_in_favor_of_the_outer_owner() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-scope-mismatch", SCOPE_PLAN, SCOPE_MACHINE, "");
    let before = fs::read_to_string(&plan_path).expect("read original plan");

    let result = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &[
            "--task",
            "1.1.1.1",
            "--from",
            "review",
            "--to",
            "completed",
            "--result",
            "deep work finished",
            "--supervisor",
            "1.1",
            "--no-callbacks",
        ],
    );
    let after = fs::read_to_string(&plan_path).expect("read resulting plan");

    assert!(
        !result.status.success()
            && result.stderr.contains("Task plan.1.1.1.1 cannot use --supervisor plan.1.1")
            && result.stderr.contains("nearest in-scope supervising ancestor is Task plan.1")
            && after == before
            && !dir.join("runtime/state-transitions.log").exists()
            && !dir.join("runtime/results/plan.1.1.1.1.md").exists(),
        "expected scope mismatch refusal with no effects\nexit: {}\nstdout:\n{}\nstderr:\n{}\n\
         plan unchanged: {}\nledger exists: {}\nresult exists: {}\nafter plan:\n{}",
        result.status,
        result.stdout,
        result.stderr,
        after == before,
        dir.join("runtime/state-transitions.log").exists(),
        dir.join("runtime/results/plan.1.1.1.1.md").exists(),
        after,
    );
}

/// The outer task is the actual owner after the nearer scope exclusion, and a
/// qualified spelling of it still suppresses the checkpoint.
// §FS-rhei-transition-cmd.2 §FS-rhei-supervision.2.2
#[test]
fn scope_exclusion_accepts_the_outer_in_scope_supervisor() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-scope-owner", SCOPE_PLAN, SCOPE_MACHINE, "");
    let result = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &[
            "--task",
            "1.1.1.1",
            "--from",
            "review",
            "--to",
            "completed",
            "--result",
            "deep work finished",
            "--supervisor",
            "plan.1",
            "--no-callbacks",
        ],
    );

    assert_success(&result);
    let after = fs::read_to_string(&plan_path).expect("read resulting plan");
    assert!(!after.contains("checkpoints:"), "the owner's checkpoint is suppressed:\n{after}");
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).expect("ledger"),
        "plan.1.1.1.1 review@completed\n"
    );
}

/// Once scope chooses the inner owner, its nonmatching terminal event filter
/// makes this hop quiet; it does not make ownership climb to the outer task.
// §FS-rhei-transition-cmd.2 §FS-rhei-supervision.2.2
#[test]
fn an_event_filter_miss_does_not_make_validation_accept_an_outer_supervisor() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-event-mismatch", EVENT_PLAN, EVENT_MACHINE, "");
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
            "fix",
            "--supervisor",
            "1",
            "--no-callbacks",
        ],
    );
    let after = fs::read_to_string(&plan_path).expect("read resulting plan");

    assert!(
        !result.status.success()
            && result.stderr.contains("Task plan.1.1.1 cannot use --supervisor plan.1")
            && result.stderr.contains("nearest in-scope supervising ancestor is Task plan.1.1")
            && after == before
            && !dir.join("runtime/state-transitions.log").exists(),
        "expected event-filter ownership refusal with no effects\nexit: {}\nstdout:\n{}\n\
         stderr:\n{}\nplan unchanged: {}\nledger exists: {}\nafter plan:\n{}",
        result.status,
        result.stdout,
        result.stderr,
        after == before,
        dir.join("runtime/state-transitions.log").exists(),
        after,
    );
}

/// The event-filtered inner owner is still a valid explicit supervisor even
/// though the non-terminal hop would have emitted no checkpoint anyway.
// §FS-rhei-transition-cmd.2 §FS-rhei-supervision.2.2
#[test]
fn an_event_filter_miss_still_accepts_the_in_scope_owner() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervisor-validation-event-owner", EVENT_PLAN, EVENT_MACHINE, "");
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
            "fix",
            "--supervisor",
            "plan.1.1",
            "--no-callbacks",
        ],
    );

    assert_success(&result);
    let after = fs::read_to_string(&plan_path).expect("read resulting plan");
    assert!(!after.contains("checkpoints:"), "the event-filter miss stays quiet:\n{after}");
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).expect("ledger"),
        "plan.1.1.1 review@fix\n"
    );
}
