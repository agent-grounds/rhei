//! Filtering across the runtime-backed prompt sections and repeat dispatches.
//! §FS-rhei-memory.4 §FS-rhei-agents.5.2

use std::fs;
use std::path::Path;

use super::*;

const SECRET: &str = "EXCLUDED-CONTEXT-207";

fn settings(root: &Path, agent: &Path, models: bool) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).expect("settings directory");
    let model_entries = if models {
        r#",
  "models": {
    "model-a": { "provider": "mock", "model": "model-a", "default_agent": "mock" },
    "model-b": { "provider": "mock", "model": "model-b", "default_agent": "mock" }
  }"#
    } else {
        ""
    };
    fs::write(
        dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{ "mock": {{ "command": {}, "stdin_prompt": true, "timeout": "10s" }} }}{model_entries}
}}"#,
            fixture_command(agent)
        ),
    )
    .expect("settings");
}

/// Results, history, briefs, optional handoffs, and previous-visit material
/// all consult the policy before reading bytes. §FS-rhei-memory.3 §FS-rhei-memory.4
#[test]
fn exclusions_filter_every_runtime_backed_prompt_section() {
    let index = "# Rhei: Context filtering\n\n## Standing context\n\nVisible project map.\n";
    let tasks = [
        ("01-prior.md", "### Task prior: Prior work\n**State:** completed\n"),
        (
            "02-blind.md",
            r#"### Task blind: Blind parent
**State:** review
**Prior:** prior
**Excludes:** artifact=runtime/results/ws.prior.md, artifact=runtime/results/ws.blind.child.md, artifact=runtime/results/ws.blind.md, artifact=runtime/supervise/ws.blind.md, artifact=runtime/handoffs/ws.blind/implementation.md

#### Task blind.child: Finished child
**State:** completed
"#,
        ),
    ];
    let (dir, workspace, machine) = create_workspace("exclude-context", index, &tasks);
    fs::write(
        &machine,
        r#"name: exclusion-context
version: 1
states:
  implement:
    initial: true
    agent: mock
    outputs:
      - { name: implementation, kind: handoff, path: runtime/handoffs/{task_id}/implementation.md }
  review:
    agent: mock
    handoff:
      inherit:
        - { from: transition.previous, required: false }
  completed: { final: true }
transitions:
  - { from: implement, to: review }
  - { from: review, to: completed }
"#,
    )
    .expect("machine");
    let agent = write_python_agent(
        &dir,
        "capture.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'captured.md', agent_prompt())
result('## Result\n\nFinished without excluded context.\n')
"#,
    );
    settings(&workspace, &agent, false);

    for relative in [
        "runtime/results/ws.prior.md",
        "runtime/results/ws.blind.child.md",
        "runtime/results/ws.blind.md",
        "runtime/supervise/ws.blind.md",
        "runtime/handoffs/ws.blind/implementation.md",
    ] {
        let path = workspace.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("runtime parent");
        fs::write(path, format!("## Result\n\n{SECRET} in {relative}\n")).expect("runtime source");
    }
    fs::write(workspace.join("runtime/state-transitions.log"), "ws.blind implement@review\n")
        .expect("transition history");

    let run = run_cli("run", &workspace, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);
    let prompt = fs::read_to_string(workspace.join("runtime/captured.md")).expect("prompt");
    assert!(!prompt.contains(SECRET), "excluded runtime bytes leaked:\n{prompt}");
    for navigation in [
        "Task ws.prior: Prior work",
        "Task ws.blind.child: Finished child",
        "runtime/results/ws.prior.md",
        "runtime/results/ws.blind.md",
        "runtime/supervise/ws.blind.md",
    ] {
        assert!(prompt.contains(navigation), "missing {navigation:?}:\n{prompt}");
    }
}

/// Re-spawning the same state resolves and filters again; a retry cannot regain
/// a source omitted from its first prompt. §FS-rhei-agents.5.2.1
#[test]
fn retry_reapplies_the_same_exclusion_policy() {
    let plan = r#"# Rhei: Retry

## Tasks

### Task prior: Prior
**State:** completed

### Task work: Retried worker
**State:** pending
**Prior:** prior
**Excludes:** artifact=runtime/results/plan.prior.md
"#;
    let (dir, plan_path, machine) = setup_single_file("exclude-retry", plan);
    let agent = write_python_agent(
        &dir,
        "retry.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
attempt = os.environ.get('RHEI_ATTEMPT', '1')
write(root / 'runtime' / ('attempt-' + attempt + '.md'), agent_prompt())
if attempt == '1':
    sys.exit(9)
result('## Result\n\nRetry succeeded.\n')
"#,
    );
    settings(&dir, &agent, false);
    fs::create_dir_all(dir.join("runtime/results")).expect("results");
    fs::write(dir.join("runtime/results/plan.prior.md"), SECRET).expect("prior result");

    let first = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!first.status.success(), "first attempt should fail");
    let second = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&second);
    for attempt in ["1", "2"] {
        let prompt = fs::read_to_string(dir.join(format!("runtime/attempt-{attempt}.md")))
            .expect("attempt prompt");
        assert!(!prompt.contains(SECRET), "attempt {attempt} leaked:\n{prompt}");
    }
}

/// Every fan-out identity receives independently filtered composition.
/// §FS-rhei-agents.5.2.2
#[test]
fn fanout_reapplies_exclusions_for_every_target() {
    let plan = r#"# Rhei: Fanout

## Tasks

### Task prior: Prior
**State:** completed

### Task review: Two reviewers
**State:** review
**Prior:** prior
**Excludes:** artifact=runtime/results/plan.prior.md
"#;
    let (dir, plan_path, machine) = setup_single_file("exclude-fanout", plan);
    fs::write(
        &machine,
        r#"name: exclusion-fanout
version: 1
models: [model-a, model-b]
states:
  review:
    initial: true
    all_targets: ["mock:mock:model-a", "mock:mock:model-b"]
  completed: { final: true }
transitions: [{ from: review, to: completed }]
"#,
    )
    .expect("fanout machine");
    let agent = write_python_agent(
        &dir,
        "fanout.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
model = os.environ.get('RHEI_MODEL_NAME', 'unknown')
write(root / 'runtime' / ('prompt-' + model + '.md'), agent_prompt())
result('## Result\n\nFanout review finished.\n')
"#,
    );
    settings(&dir, &agent, true);
    fs::create_dir_all(dir.join("runtime/results")).expect("results");
    fs::write(dir.join("runtime/results/plan.prior.md"), SECRET).expect("prior result");

    let run = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);
    for model in ["model-a", "model-b"] {
        let prompt = fs::read_to_string(dir.join(format!("runtime/prompt-{model}.md")))
            .expect("fanout prompt");
        assert!(!prompt.contains(SECRET), "target {model} leaked:\n{prompt}");
    }
}
