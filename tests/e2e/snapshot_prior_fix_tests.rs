//! Regression coverage for effective-rule validation and override candidates.
//! §FS-rhei-snapshots.11 §FS-rhei-snapshot-operations.2

use std::fs;

use super::snapshot_prior_inheritance_tests::{read_agent_log, setup_flow};
use super::*;

fn emitter_validation_machine() -> &'static str {
    r#"name: snapshot-effective-validation
version: 1
states:
  source:
    description: Available emitter
    target: fake:acme:model-a
    snapshot:
      emit: { name: available, on: always }
  consume:
    initial: true
    description: Consumer
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: missing
        from: self
        required: true
        select: { state: source }
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#
}

#[test]
fn snapshot_effective_validation_rejects_selected_state_name_mismatch() {
    let plan = r#"# Rhei: Effective validation

## Tasks

### Task consumer: Consumer
**State:** consume
"#;
    let (_dir, plan, machine) =
        setup_flow("snapshot-effective-invalid", plan, emitter_validation_machine());

    let validated = run_cli("validate", &plan, &machine, &[]);
    assert!(!validated.status.success(), "selected-state mismatch must be rejected");
    let output = format!("{}{}", validated.stdout, validated.stderr);
    assert!(
        output.contains("unresolvable effective snapshot inheritance 'missing' from self"),
        "validation should name the effective missing emitter:\n{output}"
    );
}

#[test]
fn snapshot_effective_validation_accepts_name_overlay_and_explicit_opt_out() {
    let plan = r#"# Rhei: Effective validation overlays

## Tasks

### Task overlaid: Uses the available emitter
**State:** consume
**Inherits:** available from self

### Task disabled: Explicitly cold
**State:** consume
**Inherits:** none
"#;
    let (_dir, plan, machine) =
        setup_flow("snapshot-effective-valid", plan, emitter_validation_machine());

    let validated = run_cli("validate", &plan, &machine, &[]);
    assert_success(&validated);
}

#[test]
fn snapshot_override_ignores_ready_emit_only_invocation() {
    let machine_text = r#"name: snapshot-override-candidates
version: 1
states:
  source:
    initial: true
    description: Emit only
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  review:
    description: Inherit
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: implementation
        from: self
        required: true
        select: { state: source }
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: review
    to: completed
"#;
    let initial_plan = r#"# Rhei: Override source

## Tasks

### Task source: Source
**State:** source
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-override-emit-only", initial_plan, machine_text);
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui"]));

    fs::write(
        &plan,
        r#"# Rhei: Override candidates

## Tasks

### Task consumer: Inheriting invocation
**State:** review

### Task plain: Emit-only invocation
**State:** source
"#,
    )
    .expect("write candidate plan");
    let run = run_cli(
        "run",
        &plan,
        &machine,
        &[
            "--no-tui",
            "--from-snapshot",
            "plan.source:implementation:source@1:fake-acme-model-a/g1",
            "--override-inherit",
        ],
    );
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains(
            "task=plan.consumer state=review target=fake-acme-model-a \
             resume=plan.source-source-fake-acme-model-a"
        ),
        "override should select only the inheriting invocation:\n{log}"
    );
}

#[test]
fn snapshot_override_counts_task_opt_in_without_state_rule() {
    let machine_text = r#"name: snapshot-override-task-opt-in
version: 1
states:
  source:
    initial: true
    description: Emit
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  consume:
    description: State has no inheritance rule
    target: fake:acme:model-a
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#;
    let initial_plan = r#"# Rhei: Override opt-in source

## Tasks

### Task source: Source
**State:** source
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-override-task-opt-in", initial_plan, machine_text);
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui"]));

    fs::write(
        &plan,
        r#"# Rhei: Override task opt-in

## Tasks

### Task source: Source
**State:** completed

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
**Inherits:** implementation from prior
"#,
    )
    .expect("write opt-in plan");
    let run = run_cli(
        "run",
        &plan,
        &machine,
        &[
            "--no-tui",
            "--from-snapshot",
            "plan.source:implementation:source@1:fake-acme-model-a/g1",
        ],
    );
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains(
            "task=plan.consumer state=consume target=fake-acme-model-a \
             resume=plan.source-source-fake-acme-model-a"
        ),
        "task opt-in should be an override candidate:\n{log}"
    );
}
