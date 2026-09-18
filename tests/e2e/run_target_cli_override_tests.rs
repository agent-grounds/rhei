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

fn run_target_artifact_case(
    prefix: &str,
    input_target: Option<&str>,
    write_output: bool,
    extra_args: &[&str],
) -> (TestDir, CliRun) {
    let dir = unique_temp_dir(prefix);
    let output = if write_output {
        "write(root / 'runtime' / 'outputs' / (env('RHEI_TARGET_SLUG') + '.txt'), env('RHEI_TARGET') + '\\n')\n"
    } else {
        ""
    };
    let agent_script = write_python_agent(
        &dir,
        "artifact-agent.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
{output}result('## Result\n\nTarget-scoped artifact handled.\n')
"#
        ),
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
    "override-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "state-model": {{ "provider": "registry", "model": "state-concrete" }},
    "override-model": {{ "provider": "registry", "model": "override-concrete" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
    let inputs = input_target.map_or(String::new(), |_| {
        "    inputs:\n      - name: target-input\n        path: runtime/inputs/{target.slug}.txt\n"
            .to_string()
    });
    let machine = format!(
        "name: cli-target-artifacts\nversion: 1\nmodels: [state-model, override-model]\nstates:\n  work:\n    initial: true\n    concurrent: true\n    target: state-agent[yolo]:state-provider:state-model\n    agent_timeout: 5s\n{inputs}    outputs:\n      - name: target-output\n        path: runtime/outputs/{{target.slug}}.txt\n  completed:\n    final: true\ntransitions:\n  - from: work\n    to: completed\n"
    );
    if let Some(target) = input_target {
        let input_dir = dir.join("runtime/inputs");
        fs::create_dir_all(&input_dir).expect("create input directory");
        fs::write(input_dir.join(format!("{target}.txt")), "ready\n")
            .expect("write target-scoped input");
    }
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &[&["--no-tui", "--no-callbacks"], extra_args].concat(),
    );
    (dir, result)
}

fn task_completed(dir: &Path) -> bool {
    fs::read_to_string(dir.join("plan.rhei.md"))
        .expect("read resulting plan")
        .contains("**State:** completed")
}

/// Transition verification uses the identity that produced the output in both
/// serial and parallel execution. §FS-rhei-agents.1.4
#[test]
fn run_target_cli_override_artifact_output_uses_the_effective_target() {
    for parallel in ["1", "2"] {
        let (dir, result) = run_target_artifact_case(
            &format!("run-target-output-parallel-{parallel}"),
            None,
            true,
            &["--agent", "override-agent", "--model", "override-model", "--parallel", parallel],
        );
        assert_success(&result);
        assert!(task_completed(&dir), "task did not reach its terminal state");
        assert!(
            dir.join("runtime/outputs/override-agent-yolo-state-provider-override-model.txt")
                .is_file(),
            "effective target output is absent"
        );
        assert!(
            !dir.join("runtime/outputs/state-agent-yolo-state-provider-state-model.txt").exists(),
            "authored target output must not be required or synthesized"
        );
    }
}

/// Ready input resolution and transition output resolution share the composed
/// run identity. §FS-rhei-agents.1.4 §FS-rhei-agents.1.5
#[test]
fn run_target_cli_override_artifact_input_uses_the_effective_target() {
    let effective = "override-agent-yolo-state-provider-override-model";
    let (dir, result) = run_target_artifact_case(
        "run-target-effective-input",
        Some(effective),
        true,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert_success(&result);
    assert!(task_completed(&dir), "task with its effective input did not complete");
    assert_eq!(spawn_records(&dir).len(), 1, "effective input should admit one worker");
}

/// A file rendered for the authored identity cannot satisfy an overridden
/// run's required input. §FS-rhei-agents.1.4
#[test]
fn run_target_cli_override_missing_effective_input_prevents_execution() {
    let authored = "state-agent-yolo-state-provider-state-model";
    let (dir, result) = run_target_artifact_case(
        "run-target-missing-effective-input",
        Some(authored),
        true,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert!(!result.status.success(), "run ignored its missing effective input");
    assert!(spawn_records(&dir).is_empty(), "worker ran without its effective input");
    assert!(!task_completed(&dir), "task advanced without its effective input");
}

/// Missing output enforcement remains active after the target is recomposed.
/// §FS-rhei-agents.1.4
#[test]
fn run_target_cli_override_missing_effective_output_prevents_transition() {
    let (dir, result) = run_target_artifact_case(
        "run-target-missing-effective-output",
        None,
        false,
        &["--agent", "override-agent", "--model", "override-model"],
    );
    assert!(!result.status.success(), "run advanced without its effective output");
    assert_eq!(spawn_records(&dir).len(), 1, "worker should run before output enforcement");
    assert!(!task_completed(&dir), "task advanced without its effective output");
}

/// Without CLI overrides, the authored target continues to own target-scoped
/// artifacts. §FS-rhei-agents.1.5
#[test]
fn run_target_cli_override_artifact_control_uses_the_authored_target() {
    let (dir, result) =
        run_target_artifact_case("run-target-authored-artifact-control", None, true, &[]);
    assert_success(&result);
    assert!(task_completed(&dir), "no-override control did not complete");
    assert!(
        dir.join("runtime/outputs/state-agent-yolo-state-provider-state-model.txt").is_file(),
        "authored target output is absent"
    );
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
