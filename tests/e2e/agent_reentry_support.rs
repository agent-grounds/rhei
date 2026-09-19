use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(super) const PLAN: &str = r#"# Rhei: Re-entered agent state

## Tasks

### Task 1: Work
**State:** work
"#;

pub(super) fn setup(name: &str, machine: &str, agent_body: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", machine);
    let agent = write_python_agent(&dir, "mock-agent.py", agent_body);
    write_settings(&dir, &agent, false);
    (dir, plan, machine)
}

pub(super) fn write_settings(root: &Path, agent: &Path, with_models: bool) {
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings directory");
    let models = if with_models {
        r#",
  "models": {
    "alpha": { "provider": "mock", "model": "alpha", "default_agent": "mock" },
    "beta": { "provider": "mock", "model": "beta", "default_agent": "mock" }
  }"#
    } else {
        ""
    };
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{
    "mock": {{ "command": {}, "stdin_prompt": true, "timeout": "10s" }}
  }}{}
}}"#,
            fixture_command(agent),
            models
        ),
    )
    .expect("write settings");
}

pub(super) fn spawn_lines(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("runtime/spawn-count.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

pub(super) fn ledger(root: &Path) -> String {
    fs::read_to_string(root.join("runtime/state-transitions.log")).unwrap_or_default()
}

pub(super) fn seed_output(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("output parent")).expect("create output parent");
    fs::write(path, "seeded before this visit\n").expect("seed output");
}

pub(super) fn write_spawn_record(
    root: &Path,
    suffix: Option<&str>,
    moves: u64,
    ending: &str,
    code: Option<i32>,
) -> PathBuf {
    let suffix = suffix.map(|value| format!("-{value}")).unwrap_or_default();
    let path = root.join("runtime/spawns").join(format!("task-plan.1-work{suffix}.json"));
    fs::create_dir_all(path.parent().expect("spawn record parent"))
        .expect("create spawn record directory");
    fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "task": "plan.1",
            "state": "work",
            "moves": moves,
            "attempt": 1,
            "charged": 1,
            "attempt_charged": ending != "interrupted" && ending != "provider_limited",
            "kind": "agent",
            "worker": "mock",
            "log": "runtime/logs/task-plan.1-work.log",
            "started": "2026-09-20T10:00:00Z",
            "ended": "2026-09-20T10:00:01Z",
            "duration": "1s",
            "code": code,
            "ending": ending
        }))
        .expect("serialize spawn record"),
    )
    .expect("write spawn record");
    path
}

pub(super) fn establish_reentry(root: &Path) {
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).expect("create runtime directory");
    fs::write(runtime.join("state-transitions.log"), "plan.1 verifying@work\n")
        .expect("write transition history");
}

pub(super) const COUNTING_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawn-count.log', 'work\n')
write(root / 'runtime' / 'digest.md', 'written by current invocation\n')
"#;

pub(super) const BASIC_MACHINE: &str = r#"name: agent-reentry
version: 1
states:
  work:
    initial: true
    description: Produce the digest
    agent: mock
    agent_timeout: 10s
    outputs:
      - name: digest
        path: runtime/digest.md
  verifying:
    description: Inspect the digest
    gating: true
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: verifying, description: Digest ready }
  - { from: verifying, to: work, description: Revise it }
  - { from: "*", to: cancelled, description: Stop }
"#;
