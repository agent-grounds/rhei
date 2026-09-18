//! Prior source selection, manual override, and cross-rhei identity.
//! §FS-rhei-snapshots.4.3 §FS-rhei-snapshot-operations.2 §FS-rhei-panta.6.1

use std::fs;

use super::snapshot_prior_inheritance_tests::{
    assert_consumer_resumed_source, read_agent_log, required_prior_rule,
    required_prior_rule_for_any_state, setup_flow, state_rule_machine,
};
use super::snapshot_tests::{write_fake_snapshot_agent, write_fake_snapshot_settings};
use super::*;

#[test]
fn snapshot_prior_target_filter_precedes_cross_source_ambiguity() {
    let plan = r#"# Rhei: Prior Target Selection

## Tasks

### Task source-a: Model A source
**State:** source-a

### Task source-b: Model B source
**State:** source-b

### Task consumer: Model B consumer
**State:** consume
**Prior:** Task source-a, Task source-b
"#;
    let machine = format!(
        r#"name: snapshot-prior-target-filter
version: 1
states:
  source-a:
    initial: true
    description: Model A source
    target: fake:acme:model-a
    snapshot:
      emit: {{ name: implementation, on: always }}
  source-b:
    description: Model B source
    target: fake:acme:model-b
    snapshot:
      emit: {{ name: implementation, on: always }}
  consume:
    description: Select exact target first
    target: fake:acme:model-b
{}  completed:
    description: Done
    final: true
transitions:
  - from: source-a
    to: completed
  - from: source-b
    to: completed
  - from: consume
    to: completed
"#,
        required_prior_rule_for_any_state()
    );
    let (dir, plan, machine) = setup_flow("snapshot-prior-target-filter", plan, &machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains(
            "task=plan.consumer state=consume target=fake-acme-model-b \
             resume=plan.source-b-source-b-fake-acme-model-b"
        ),
        "exact target filtering should choose source-b before ambiguity:\n{log}"
    );
    assert!(!log.contains(
        "task=plan.consumer state=consume target=fake-acme-model-b resume=plan.source-a"
    ));
}

#[test]
fn snapshot_prior_provider_and_model_differences_are_advisory_after_native_selection() {
    let plan = r#"# Rhei: Prior Native Compatibility

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
"#;
    let machine = state_rule_machine(required_prior_rule())
        .replace(
            "  consume:\n    description: Continue the declared predecessor\n    target: fake:acme:model-a",
            "  consume:\n    description: Continue the declared predecessor\n    target: fake:other:model-b",
        )
        .replace("          target: same\n", "");
    let (dir, plan, machine) = setup_flow("snapshot-prior-native-advisory", plan, &machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains(
            "task=plan.consumer state=consume target=fake-other-model-b \
             resume=plan.source-source-fake-acme-model-a"
        ),
        "same agent/layout should preload despite provider/model cache advice:\n{log}"
    );
}

#[test]
fn snapshot_prior_manual_override_cannot_bypass_task_none() {
    let dir = unique_temp_dir("snapshot-prior-manual-none");
    let agent = write_fake_snapshot_agent(&dir);
    write_fake_snapshot_settings(&dir, &agent);
    let machine = write_fixture_file(&dir, "states.yaml", &state_rule_machine(""));
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Prior Manual None

## Tasks

### Task source: Source
**State:** source
"#,
    );
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui"]));
    assert!(
        dir.join(".rhei/cache/snapshots/plan.source/implementation/source/1/fake-acme-model-a/g1")
            .is_dir(),
        "baseline source snapshot should exist"
    );

    fs::write(&machine, state_rule_machine(required_prior_rule()))
        .expect("enable Prior inheritance for the consumer phase");
    fs::write(
        &plan,
        r#"# Rhei: Prior Manual None

## Tasks

### Task source: Source
**State:** completed

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
**Inherits:** none
"#,
    )
    .expect("write consumer plan");
    let source_ref = "plan.source:implementation:source@1:fake-acme-model-a/g1";
    let run = run_cli(
        "run",
        &plan,
        &machine,
        &[
            "--no-tui",
            "--task",
            "plan.consumer",
            "--from-snapshot",
            source_ref,
            "--override-inherit",
        ],
    );
    assert!(!run.status.success(), "task none must reject a manual inheritance source");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("no effective snapshot inheritance contract")
            && output.contains("**Inherits:** none"),
        "manual refusal should name the task opt-out:\n{output}"
    );
}

#[test]
fn snapshot_prior_manual_source_obeys_declared_edges_unless_debug_bypass_is_explicit() {
    let dir = unique_temp_dir("snapshot-prior-manual-edge");
    let agent = write_fake_snapshot_agent(&dir);
    write_fake_snapshot_settings(&dir, &agent);
    let machine = write_fixture_file(&dir, "states.yaml", &state_rule_machine(""));
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Prior Manual Edge

## Tasks

### Task source-a: Declared source
**State:** source

### Task source-b: Undeclared source
**State:** source
"#,
    );
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui"]));

    fs::write(&machine, state_rule_machine(required_prior_rule()))
        .expect("enable Prior inheritance for the consumer phase");
    fs::write(
        &plan,
        r#"# Rhei: Prior Manual Edge

## Tasks

### Task source-a: Declared source
**State:** completed

### Task source-b: Undeclared source
**State:** completed

### Task consumer: Consumer
**State:** consume
**Prior:** Task source-a
"#,
    )
    .expect("write consumer plan");
    let undeclared_ref = "plan.source-b:implementation:source@1:fake-acme-model-a/g1";
    let rejected = run_cli(
        "run",
        &plan,
        &machine,
        &["--no-tui", "--task", "plan.consumer", "--from-snapshot", undeclared_ref],
    );
    assert!(!rejected.status.success(), "undeclared manual source must be rejected");
    let output = format!("{}{}", rejected.stdout, rejected.stderr);
    assert!(
        output.contains("not a declared Prior") && output.contains("plan.source-b"),
        "manual source refusal should name the edge violation:\n{output}"
    );

    let bypassed = run_cli(
        "run",
        &plan,
        &machine,
        &[
            "--no-tui",
            "--task",
            "plan.consumer",
            "--from-snapshot",
            undeclared_ref,
            "--override-inherit",
        ],
    );
    assert_success(&bypassed);
    assert_consumer_resumed_source(&read_agent_log(&dir), "plan.consumer", "plan.source-b");
}

#[test]
fn snapshot_prior_cross_rhei_edge_uses_the_source_identity_and_owning_machine() {
    let dir = unique_temp_dir("snapshot-prior-cross-rhei");
    let agent = write_fake_snapshot_agent(&dir);
    let project = dir.join("project");
    let producer = project.join("producer");
    let consumer = project.join("consumer");
    fs::create_dir_all(producer.join("tasks")).expect("producer workspace");
    fs::create_dir_all(consumer.join("tasks")).expect("consumer workspace");
    write_fake_snapshot_settings(&project, &agent);
    write_fixture_file(&project, "index.panta.md", "# Panta: Cross Rhei\n");
    write_fixture_file(
        &producer,
        "index.rhei.md",
        "# Rhei: Producer\n**States:** producer-machine\n",
    );
    write_fixture_file(
        &producer,
        "states.yaml",
        r#"name: producer-machine
version: 1
states:
  source:
    initial: true
    description: Produce
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  shipped:
    description: Shipped
    final: true
transitions:
  - from: source
    to: shipped
"#,
    );
    write_fixture_file(&producer, "tasks/01-source.md", "### Task 1: Source\n**State:** source\n");
    write_fixture_file(
        &consumer,
        "index.rhei.md",
        "# Rhei: Consumer\n**States:** consumer-machine\n",
    );
    write_fixture_file(
        &consumer,
        "states.yaml",
        r#"name: consumer-machine
version: 1
states:
  consume:
    initial: true
    description: Consume
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: implementation
        from: prior
        required: true
  done:
    description: Done
    final: true
transitions:
  - from: consume
    to: done
"#,
    );
    write_fixture_file(
        &consumer,
        "tasks/01-consumer.md",
        "### Task 1: Consumer\n**State:** consume\n**Prior:** producer.1\n",
    );

    let run = run_cli_without_machine("run", &project, &["--no-tui"]);
    assert_success(&run);
    let log = fs::read_to_string(consumer.join("runtime/fake-agent.log")).expect("consumer log");
    assert_consumer_resumed_source(&log, "consumer.1", "producer.1");
}
