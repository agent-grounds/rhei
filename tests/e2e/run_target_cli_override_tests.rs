//! Run-level identity composition at the autonomous spawn boundary.
//! §FS-rhei-run.2.2 §FS-rhei-agents.1.4 §FS-rhei-agents.1.5

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const PLAN: &str = r#"# Rhei: CLI target override

## Tasks

### Task 1: Observe identity
**State:** work
"#;

fn run_identity_case(
    prefix: &str,
    state_fields: &str,
    plan: &str,
    extra_args: &[&str],
) -> (TestDir, CliRun) {
    let dir = unique_temp_dir(prefix);
    let agent_script = write_python_agent(
        &dir,
        "identity-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
append(
    root / 'runtime' / 'observed-identities.log',
    'task={} agent={} mode={} provider={} model={} target={}\n'.format(
        env('RHEI_TASK_ID'),
        env('RHEI_AGENT'),
        env('RHEI_AGENT_MODE'),
        env('RHEI_MODEL_PROVIDER'),
        env('RHEI_MODEL'),
        env('RHEI_TARGET'),
    ),
)
result('## Result\n\nIdentity observed.\n')
"#,
    );
    let command = fixture_command(&agent_script);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "state-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }},
    "task-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "safe": [] }}
    }},
    "override-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [], "safe": [] }}
    }},
    "no-yolo-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "safe": [] }}
    }}
  }},
  "models": {{
    "state-model": {{ "provider": "registry", "model": "state-concrete" }},
    "task-model": {{ "provider": "registry", "model": "task-concrete" }},
    "override-model": {{ "provider": "registry", "model": "override-concrete" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
    let machine = format!(
        "name: cli-target-override\nversion: 1\nmodels: [state-model, task-model, override-model]\nstates:\n  work:\n    initial: true\n    agent_timeout: 5s\n{state_fields}  completed:\n    final: true\ntransitions:\n  - from: work\n    to: completed\n"
    );
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &[&["--no-tui", "--no-callbacks"], extra_args].concat(),
    );
    (dir, result)
}

fn observed_identities(dir: &Path) -> String {
    fs::read_to_string(dir.join("runtime/observed-identities.log")).unwrap_or_default()
}

fn spawn_records(dir: &Path) -> Vec<(String, PathBuf)> {
    let spawn_dir = dir.join("runtime/spawns");
    if !spawn_dir.exists() {
        return Vec::new();
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(spawn_dir).expect("read spawn records") {
        let path = entry.expect("spawn record entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let record: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).expect("read spawn record"))
                .expect("spawn record JSON");
        records.push((
            record["worker"].as_str().expect("spawn worker").to_string(),
            PathBuf::from(record["log"].as_str().expect("spawn log")),
        ));
    }
    records
}

fn assert_single_identity(
    dir: &Path,
    result: &CliRun,
    agent: &str,
    mode: &str,
    provider: &str,
    model: &str,
    target: &str,
) {
    assert_success(result);
    let observed = observed_identities(dir);
    let expected = format!(
        "task=plan.1 agent={agent} mode={mode} provider={provider} model={model} target={target}"
    );
    let records = spawn_records(dir);
    let mut mismatches = Vec::new();
    if !observed.lines().any(|line| line == expected) {
        mismatches.push(format!("process identity: expected {expected:?}, got {observed:?}"));
    }
    if records.len() != 1 {
        mismatches.push(format!("spawn records: expected one, got {records:?}"));
    } else {
        let (worker, log_path) = &records[0];
        if worker != agent {
            mismatches.push(format!("spawn worker: expected {agent:?}, got {worker:?}"));
        }
        let log = fs::read_to_string(log_path).expect("read invocation log");
        for header in [
            format!("agent: {agent}"),
            format!("mode: {mode}"),
            format!("provider: {provider}"),
            format!("model: {model}"),
            format!("target: {target}"),
        ] {
            if !log.lines().any(|line| line == header) {
                mismatches.push(format!("invocation log missing {header:?}:\n{log}"));
            }
        }
    }
    assert!(mismatches.is_empty(), "wrong launched identity:\n{}", mismatches.join("\n"));
}

#[test]
fn run_target_cli_override_no_flags_preserves_the_state_target() {
    let (dir, result) = run_identity_case(
        "run-state-target-control",
        "    target: state-agent[yolo]:state-provider:state-model\n",
        PLAN,
        &[],
    );
    assert_single_identity(
        &dir,
        &result,
        "state-agent",
        "yolo",
        "state-provider",
        "state-model",
        "state-agent[yolo]:state-provider:state-model",
    );
}

#[test]
fn run_target_cli_override_agent_replaces_only_the_state_target_agent() {
    let (dir, result) = run_identity_case(
        "run-state-target-agent-override",
        "    target: state-agent[yolo]:state-provider:state-model\n",
        PLAN,
        &["--agent", "override-agent"],
    );
    assert_single_identity(
        &dir,
        &result,
        "override-agent",
        "yolo",
        "state-provider",
        "state-model",
        "override-agent[yolo]:state-provider:state-model",
    );
}

#[test]
fn run_target_cli_override_model_replaces_only_the_state_target_model() {
    let (dir, result) = run_identity_case(
        "run-state-target-model-override",
        "    target: state-agent[yolo]:state-provider:state-model\n",
        PLAN,
        &["--model", "override-model"],
    );
    assert_single_identity(
        &dir,
        &result,
        "state-agent",
        "yolo",
        "state-provider",
        "override-model",
        "state-agent[yolo]:state-provider:override-model",
    );
}

#[test]
fn run_target_cli_override_combined_flags_compose_the_state_target_identity() {
    let (dir, result) = run_identity_case(
        "run-state-target-combined-overrides",
        "    target: state-agent[yolo]:state-provider:state-model\n",
        PLAN,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert_single_identity(
        &dir,
        &result,
        "override-agent",
        "yolo",
        "state-provider",
        "override-model",
        "override-agent[yolo]:state-provider:override-model",
    );
}

/// §FS-rhei-plan-language.3.11
#[test]
fn run_target_cli_override_dimensions_take_precedence_over_task_identity() {
    let plan = r#"# Rhei: CLI over task overrides

## Tasks

### Task target: Target identity
**State:** work
**Target:** task-agent[safe]:task-provider:task-model

### Task model: Model identity
**State:** work
**Model:** task-model
"#;
    let (dir, result) = run_identity_case(
        "run-cli-over-task-identities",
        "    target: state-agent[yolo]:state-provider:state-model\n",
        plan,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert_success(&result);
    let observed = observed_identities(&dir);
    let mut missing = Vec::new();
    for expected in [
        "task=plan.target agent=override-agent mode=safe provider=task-provider model=override-model target=override-agent[safe]:task-provider:override-model",
        "task=plan.model agent=override-agent mode=yolo provider=state-provider model=override-model target=override-agent[yolo]:state-provider:override-model",
    ] {
        if !observed.lines().any(|line| line == expected) {
            missing.push(expected);
        }
    }
    assert!(missing.is_empty(), "missing expected identities {missing:?}:\n{observed}");
}

/// §FS-rhei-plan-language.3.11
#[test]
fn run_target_cli_override_remains_effective_on_a_target_locked_state() {
    let (dir, result) = run_identity_case(
        "run-cli-over-target-locked",
        "    target: state-agent[yolo]:state-provider:state-model\n    target_locked: true\n",
        PLAN,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert_single_identity(
        &dir,
        &result,
        "override-agent",
        "yolo",
        "state-provider",
        "override-model",
        "override-agent[yolo]:state-provider:override-model",
    );
}

#[test]
fn run_target_cli_override_keeps_all_targets_selector_owned() {
    let (dir, result) = run_identity_case(
        "run-cli-all-targets-boundary",
        "    all_targets:\n      - state-agent[yolo]:state-provider:state-model\n      - task-agent[safe]:task-provider:task-model\n",
        PLAN,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert_success(&result);
    let observed = observed_identities(&dir);
    for expected in [
        "agent=state-agent mode=yolo provider=state-provider model=state-model target=state-agent[yolo]:state-provider:state-model",
        "agent=task-agent mode=safe provider=task-provider model=task-model target=task-agent[safe]:task-provider:task-model",
    ] {
        assert!(observed.lines().any(|line| line.contains(expected)), "missing {expected:?}:\n{observed}");
    }
    assert_eq!(spawn_records(&dir).len(), 2, "one spawn per authored target");
}

#[test]
fn run_target_cli_override_incompatible_mode_is_refused_before_spawn() {
    let (dir, result) = run_identity_case(
        "run-cli-incompatible-preserved-mode",
        "    target: state-agent[yolo]:state-provider:state-model\n",
        PLAN,
        &["--agent", "no-yolo-agent"],
    );
    assert!(!result.status.success(), "incompatible effective agent unexpectedly ran");
    assert!(
        result.stderr.contains("no-yolo-agent") && result.stderr.contains("yolo"),
        "diagnostic must name the agent and preserved mode:\n{}",
        result.stderr
    );
    assert!(spawn_records(&dir).is_empty(), "incompatible identity spawned an agent");
    assert!(observed_identities(&dir).is_empty(), "incompatible identity launched its process");
}
