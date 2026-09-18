//! Timeout callbacks retain the run's independently composed agent and model.
//! §FS-rhei-agents.7.5 §FS-rhei-agents.1.4 §FS-rhei-agents.1.5

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::*;

fn read_observation(workspace: &Path, local: &str, phase: &str) -> Value {
    let path = workspace.join(format!("runtime/identities/{local}-{phase}.json"));
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("missing {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("identity observation should be JSON")
}

fn run_timeout_identity_matrix(parallel: &str) {
    let cases: &[(&str, &[&str], &str, &str)] = &[
        ("control", &[], "state-agent", "state-model"),
        ("agent", &["--agent", "override-agent"], "override-agent", "state-model"),
        ("model", &["--model", "override-model"], "state-agent", "override-model"),
        (
            "combined",
            &["--agent", "override-agent", "--model", "override-model"],
            "override-agent",
            "override-model",
        ),
    ];
    for &(case, flags, agent, model) in cases {
        run_timeout_identity_case(
            &format!("timeout-identity-{parallel}-{case}"),
            parallel,
            flags,
            agent,
            model,
        );
    }
}

fn run_timeout_identity_case(
    prefix: &str,
    parallel: &str,
    flags: &[&str],
    agent: &str,
    model: &str,
) {
    // Two ready tasks in separate files force the worker pool with --parallel 2;
    // a one-invocation batch still uses the serial agent path. §FS-rhei-run.5
    let tasks = [
        ("01-first.md", "### Task first: First timeout\n**State:** work\n"),
        ("02-second.md", "### Task second: Second timeout\n**State:** work\n"),
    ];
    let (dir, workspace, machine_path) =
        create_workspace(prefix, "# Rhei: Timeout callback identity\n", &tasks);
    let worker = write_python_agent(
        &dir,
        "timeout-agent.py",
        r#"import json
root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
write(root / 'runtime' / 'identities' / (local + '-worker.json'), json.dumps({
    'identity': {'agent': env('RHEI_AGENT'), 'model': env('RHEI_MODEL')},
    'target': env('RHEI_TARGET'),
    'observed_at': time.monotonic_ns(),
}))
time.sleep(30)
"#,
    );
    let callback = write_fixture_file(
        &dir,
        "timeout-callback.py",
        r#"import json
import os
import pathlib
import sys
import time

context = json.load(sys.stdin)
root = pathlib.Path(os.environ['RHEI_PLAN_PATH'])
if root.is_file():
    root = root.parent
local = os.environ['RHEI_TASK_ID_LOCAL']
phase = sys.argv[1]
observations = root / 'runtime' / 'identities'
observations.mkdir(parents=True, exist_ok=True)
(observations / (local + '-' + phase + '.json')).write_text(json.dumps({
    'identity': {'agent': os.environ.get('RHEI_AGENT'), 'model': os.environ.get('RHEI_MODEL')},
    'context': context,
    'observed_at': time.monotonic_ns(),
}), encoding='utf-8')
print(json.dumps({'success': True, 'data': {'leave_observed': True}}))
"#,
    );
    let command: Value = serde_json::from_str(&fixture_command(&worker)).expect("worker command");
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        json!({
            "agents": {
                "state-agent": {
                    "command": command,
                    "stdin_prompt": true,
                    "modes": {"yolo": []}
                },
                "override-agent": {
                    "command": command,
                    "stdin_prompt": true,
                    "modes": {"yolo": []}
                }
            },
            "models": {
                "state-model": {"provider": "registry", "model": "state-concrete"},
                "override-model": {"provider": "registry", "model": "override-concrete"}
            }
        })
        .to_string(),
    )
    .expect("write settings");
    let callback_command = fixture_command_line(&callback);
    let leave = serde_json::to_string(&format!("cli:{callback_command} leave")).unwrap();
    let enter = serde_json::to_string(&format!("cli:{callback_command} enter")).unwrap();
    fs::write(
        &machine_path,
        format!(
            r#"name: timeout-callback-identity
version: 1
states:
  work:
    initial: true
    concurrent: true
    target: state-agent[yolo]:state-provider:state-model
    agent_timeout: 3s
    attempts: 1
  timed-out:
    final: true
transitions:
  - from: work
    to: timed-out
    timeout: 3s
    on_leave: {leave}
    on_enter: {enter}
"#
        ),
    )
    .expect("write state machine");
    assert_success(&run_cli("validate", &workspace, &machine_path, &[]));
    let run = run_cli(
        "run",
        &workspace,
        &machine_path,
        &[&["--no-tui", "--parallel", parallel], flags].concat(),
    );
    assert_success(&run);
    let output = format!("{}\n{}", run.stdout, run.stderr);
    assert!(output.contains("Final states: timed-out=2"), "{prefix}: {output}");
    assert_eq!(output.matches("Timeout transition: Task ").count(), 2, "{prefix}: {output}");
    let expected = json!({"agent": agent, "model": model});
    let mut worker_starts = Vec::new();
    let mut callback_starts = Vec::new();
    for (local, file) in [("first", "01-first.md"), ("second", "02-second.md")] {
        let task = fs::read_to_string(workspace.join("tasks").join(file)).expect("read task");
        assert!(task.contains("**State:** timed-out"), "{prefix}: {task}");
        let worker = read_observation(&workspace, local, "worker");
        let leave = read_observation(&workspace, local, "leave");
        let enter = read_observation(&workspace, local, "enter");
        assert_eq!(worker["identity"], expected, "{prefix} {local}: worker identity");
        assert_eq!(
            worker["target"],
            format!("{agent}[yolo]:state-provider:{model}"),
            "{prefix} {local}: preserve target provider and mode"
        );
        assert_eq!(
            leave["identity"], worker["identity"],
            "{prefix} {local}: timeout callback identity"
        );
        for observation in [&leave, &enter] {
            let context = &observation["context"];
            assert_eq!(context["transition"]["from"], "work");
            assert_eq!(context["transition"]["to"], "timed-out");
            assert_eq!(context["transition"]["triggeredBy"], "system");
            assert_eq!(context["transitionData"]["timeout"], "3s");
        }
        assert_eq!(enter["context"]["transitionData"]["leave_observed"], true);
        worker_starts.push(worker["observed_at"].as_u64().expect("worker timestamp"));
        callback_starts.push(leave["observed_at"].as_u64().expect("callback timestamp"));
    }
    if parallel == "2" {
        assert!(
            worker_starts.iter().max() < callback_starts.iter().min(),
            "{prefix}: both workers must start before either timeout callback; workers={worker_starts:?}, callbacks={callback_starts:?}"
        );
    }
}

/// The serial completion path retains each independent CLI dimension and its
/// no-override control. §FS-rhei-agents.7.5 §FS-rhei-agents.1.5
#[test]
fn run_target_cli_override_timeout_callbacks_serial() {
    run_timeout_identity_matrix("1");
}

/// Two concurrent tasks exercise parallel timeout completion with the same
/// identity matrix and callback lifecycle assertions. §FS-rhei-agents.7.5 §FS-rhei-agents.1.5
#[test]
fn run_target_cli_override_timeout_callbacks_parallel() {
    run_timeout_identity_matrix("2");
}
