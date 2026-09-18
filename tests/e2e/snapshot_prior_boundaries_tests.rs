//! Prior inheritance selection, fallback, project, and manual-override edges.
//! §FS-rhei-snapshots.4.3 §FS-rhei-snapshots.4.6 §FS-rhei-snapshots.5

use std::fs;

use super::snapshot_prior_inheritance_tests::{read_agent_log, setup_flow, state_rule_machine};
use super::snapshot_tests::{write_fake_snapshot_agent, write_fake_snapshot_settings};
use super::*;

fn write_two_agent_snapshot_settings(root: &std::path::Path, agent: &std::path::Path) {
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("settings directory");
    let command = fixture_command(agent);
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "fake": {{
      "command": {command},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "session": {{
        "resume": {{"flag": "--resume"}},
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }},
    "other": {{
      "command": {command},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "session": {{
        "resume": {{"flag": "--resume"}},
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }}
  }}
}}"#
        ),
    )
    .expect("write two-agent settings");
}

fn incompatible_agent_flow(required: bool) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(if required {
        "snapshot-prior-incompatible-required"
    } else {
        "snapshot-prior-incompatible-optional"
    });
    let agent = write_fake_snapshot_agent(&dir);
    write_two_agent_snapshot_settings(&dir, &agent);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Prior Incompatible Agent

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
"#,
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: snapshot-prior-incompatible
version: 1
states:
  source:
    initial: true
    description: Fake source
    target: fake:acme:model-a
    snapshot:
      emit: {{ name: implementation, on: always }}
  consume:
    description: Other-agent consumer
    target: other:acme:model-a
    snapshot:
      inherit:
        name: implementation
        from: prior
        required: {required}
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#
        ),
    );
    (dir, plan, machine)
}

fn capture_only_flow(required: bool) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(if required {
        "snapshot-prior-unsupported-required"
    } else {
        "snapshot-prior-unsupported-optional"
    });
    let agent = write_fake_snapshot_agent(&dir);
    let settings = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("settings directory");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "capture-only": {{
      "command": {},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "session": {{
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }}
  }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write capture-only settings");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Prior Unsupported Session

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
"#,
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: snapshot-prior-unsupported
version: 1
states:
  source:
    initial: true
    description: Capture-only source
    target: capture-only:acme:model-a
    snapshot:
      emit: {{ name: implementation, on: always }}
  consume:
    description: Capture-only consumer
    target: capture-only:acme:model-a
    snapshot:
      inherit:
        name: implementation
        from: prior
        required: {required}
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#
        ),
    );
    (dir, plan, machine)
}

#[test]
fn snapshot_prior_incompatible_optional_source_runs_cold() {
    let (dir, plan, machine) = incompatible_agent_flow(false);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("preload skipped: incompatible agent") && output.contains("running cold"),
        "optional compatibility warning missing:\n{output}"
    );
    let log = read_agent_log(&dir);
    assert!(
        log.contains("task=plan.consumer state=consume target=other-acme-model-a resume= parent="),
        "optional incompatible source should be visibly cold:\n{log}"
    );
}

#[test]
fn snapshot_prior_incompatible_required_source_fails_before_consumer_spawn() {
    let (dir, plan, machine) = incompatible_agent_flow(true);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert!(!run.status.success(), "required incompatible source should fail");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(output.contains("incompatible-snapshot"), "wrong diagnostic:\n{output}");
    let log = fs::read_to_string(dir.join("runtime/fake-agent.log")).unwrap_or_default();
    assert!(
        !log.contains("task=plan.consumer state=consume"),
        "required incompatibility must fail before consumer spawn:\n{log}"
    );
}

#[test]
fn snapshot_prior_unsupported_optional_preload_strategy_runs_cold() {
    let (dir, plan, machine) = capture_only_flow(false);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("unsupported-snapshot-session") && output.contains("running cold"),
        "optional unsupported-session warning missing:\n{output}"
    );
    let log = read_agent_log(&dir);
    assert!(
        log.contains(
            "task=plan.consumer state=consume target=capture-only-acme-model-a resume= parent="
        ),
        "optional unsupported strategy should be visibly cold:\n{log}"
    );
}

#[test]
fn snapshot_prior_unsupported_required_preload_strategy_fails_before_spawn() {
    let (dir, plan, machine) = capture_only_flow(true);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert!(!run.status.success(), "required unsupported strategy should fail");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(output.contains("unsupported-snapshot-session"), "wrong diagnostic:\n{output}");
    let log = fs::read_to_string(dir.join("runtime/fake-agent.log")).unwrap_or_default();
    assert!(
        !log.contains("task=plan.consumer state=consume"),
        "required unsupported strategy must fail before consumer spawn:\n{log}"
    );
}

#[test]
fn snapshot_prior_cancelled_dependency_never_becomes_ready_via_optional_fallback() {
    let plan = r#"# Rhei: Prior Cancelled Readiness

## Tasks

### Task source: Cancelled source
**State:** cancelled

### Task consumer: Blocked consumer
**State:** consume
**Prior:** Task source
"#;
    let machine = r#"name: snapshot-prior-cancelled
version: 1
states:
  source:
    initial: true
    description: Source
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  consume:
    description: Must stay blocked
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: false }
  cancelled:
    description: Cancelled
    final: true
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#;
    let (dir, plan, machine) = setup_flow("snapshot-prior-cancelled", plan, machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert!(!run.status.success(), "a cancelled Prior must leave the run blocked");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("waiting on Task plan.source (cancelled)")
            && output.contains("rhei run halted with non-terminal tasks remaining"),
        "blocked run should explain the cancelled Prior:\n{output}"
    );
    assert!(
        !dir.join("runtime/fake-agent.log").exists(),
        "optional inheritance must not make a cancelled Prior runnable"
    );
    assert_task_state(&plan, &machine, "consumer", "consume");
}

#[test]
fn snapshot_prior_timed_out_source_is_optional_cold_but_required_pre_spawn_failure() {
    let dir = unique_temp_dir("snapshot-prior-timeout");
    let agent = write_fake_snapshot_agent(&dir);
    write_fake_snapshot_settings(&dir, &agent);
    let machine = write_fixture_file(&dir, "states.yaml", &state_rule_machine(""));
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Prior Timeout

## Tasks

### Task source: Source
**State:** source
"#,
    );
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui"]));

    let manifest_path = dir
        .join(".rhei/cache/snapshots/plan.source/implementation/source/1/fake-acme-model-a/g1/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("source manifest"))
            .expect("manifest JSON");
    manifest["completion"] = serde_json::Value::String("timeout".to_string());
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).expect("manifest JSON"))
        .expect("mark source timed out");

    fs::write(
        &machine,
        r#"name: snapshot-prior-timeout
version: 1
states:
  source:
    initial: true
    description: Source
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  optional:
    description: Optional timeout
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: false }
  required:
    description: Required timeout
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: true }
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: optional
    to: completed
  - from: required
    to: completed
"#,
    )
    .expect("enable timeout consumers");
    fs::write(
        &plan,
        r#"# Rhei: Prior Timeout

## Tasks

### Task source: Source
**State:** completed

### Task optional: Optional consumer
**State:** optional
**Prior:** Task source

### Task required: Required consumer
**State:** required
**Prior:** Task source
"#,
    )
    .expect("write timeout consumers");

    let run = run_cli("run", &plan, &machine, &["--no-tui", "--parallel", "1"]);
    assert!(!run.status.success(), "required timed-out source should fail");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("timed-out snapshot") || output.contains("timed out"),
        "timeout boundary diagnostic missing:\n{output}"
    );
    let log = read_agent_log(&dir);
    assert!(
        log.contains("task=plan.optional state=optional target=fake-acme-model-a resume= parent="),
        "optional timed-out source should run cold:\n{log}"
    );
    assert!(
        !log.contains("task=plan.required state=required"),
        "required timed-out source must fail before spawn:\n{log}"
    );
}

#[test]
fn snapshot_prior_missing_source_is_optional_cold_but_required_pre_spawn_failure() {
    let plan = r#"# Rhei: Prior Missing Boundary

## Tasks

### Task source: Source
**State:** completed

### Task optional: Optional consumer
**State:** optional
**Prior:** Task source

### Task required: Required consumer
**State:** required
**Prior:** Task source
"#;
    let machine = r#"name: snapshot-prior-missing
version: 1
states:
  source:
    initial: true
    description: Can emit but has never run
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  optional:
    description: Runs cold
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: false }
  required:
    description: Fails before spawn
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: true }
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: optional
    to: completed
  - from: required
    to: completed
"#;
    let (dir, plan, machine) = setup_flow("snapshot-prior-missing", plan, machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui", "--parallel", "1"]);
    assert!(!run.status.success(), "required missing source should fail the run");
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(output.contains("missing-snapshot"), "missing required diagnostic:\n{output}");
    let log = read_agent_log(&dir);
    assert!(
        log.contains("task=plan.optional state=optional")
            && log.contains(
                "task=plan.optional state=optional target=fake-acme-model-a resume= parent="
            ),
        "optional missing source should run cold:\n{log}"
    );
    assert!(
        !log.contains("task=plan.required state=required"),
        "required missing source must fail before spawn:\n{log}"
    );
}
