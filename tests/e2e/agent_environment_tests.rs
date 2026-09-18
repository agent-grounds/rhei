use std::fs;

use super::*;

const OUTER_RUNTIME_SENTINEL: &[u8] = b"outer runtime sentinel\n";
const OUTER_RESULT_SENTINEL: &[u8] = b"outer result sentinel\n";

/// An autonomous worker receives its task contract in the prompt, but none of
/// the outer run identity in its environment. Its child can therefore run a
/// nested Rhei without redirecting that run into the outer runtime.
/// §FS-rhei-agents.4
#[test]
fn autonomous_worker_children_cannot_inherit_the_outer_rhei_identity() {
    let dir = unique_temp_dir("agent-environment-isolation");
    let inner = dir.join("inner");
    fs::create_dir_all(&inner).expect("create nested workspace");

    let inner_program = write_python_agent(
        &inner,
        "nested-program.py",
        r#"root = pathlib.Path(env('RHEI_ROOT', '.'))
write(root / 'runtime' / 'nested-marker.txt', 'nested workspace\n')
result('## Result\n\nNested fixture finished.\n')
"#,
    );
    write_fixture_file(
        &inner,
        "plan.rhei.md",
        r#"# Rhei: Nested Fixture

## Tasks

### Task 1: Write inside the nested workspace
**State:** run
"#,
    );
    write_fixture_file(
        &inner,
        "states.yaml",
        &format!(
            r#"name: nested-agent-environment
version: 1
states:
  run:
    initial: true
    program:
      command: {}
  completed:
    final: true
transitions:
  - from: run
    to: completed
    exit_code: 0
"#,
            fixture_command(&inner_program)
        ),
    );

    write_python_agent(
        &dir,
        "worker-child.py",
        r#"import json
import subprocess

names = (
    'RHEI_ROOT',
    'RHEI_PLAN_PATH',
    'RHEI_RESULT_PATH',
    'RHEI_TASK_ID',
    'RHEI_TASK_ID_LOCAL',
)
write(pathlib.Path.cwd() / 'observed-env.json', json.dumps(
    {name: os.environ.get(name) for name in names},
    indent=2,
    sort_keys=True,
) + '\n')

# This is the corruption reported in issue #157: an ordinary child that uses
# an inherited result contract can replace the live outer task's result.
outer_result = os.environ.get('RHEI_RESULT_PATH')
if outer_result:
    write(outer_result, 'child replaced the outer result\n')

nested = pathlib.Path.cwd() / 'inner'
completed = subprocess.run(
    [
        sys.argv[1],
        '--state-machine',
        str(nested / 'states.yaml'),
        'run',
        str(nested / 'plan.rhei.md'),
        '--no-tui',
        '--no-callbacks',
    ],
    cwd=nested,
    text=True,
    capture_output=True,
)
write(
    pathlib.Path.cwd() / 'nested-command.txt',
    'exit={}\nstdout:\n{}\nstderr:\n{}'.format(
        completed.returncode,
        completed.stdout,
        completed.stderr,
    ),
)
sys.exit(completed.returncode)
"#,
    );

    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"import subprocess

prompt = sys.stdin.read()
output_line = next(
    line for line in prompt.splitlines() if line.startswith('Prompt evidence output: ')
)
output = pathlib.Path(output_line.removeprefix('Prompt evidence output: '))
if not output.is_absolute():
    output = pathlib.Path.cwd() / output
write(output, prompt)

completed = subprocess.run(
    [sys.executable, str(pathlib.Path.cwd() / 'worker-child.py'), sys.argv[1]],
    cwd=pathlib.Path.cwd(),
    text=True,
    capture_output=True,
)
if completed.stdout:
    print(completed.stdout, end='')
if completed.stderr:
    print(completed.stderr, end='', file=sys.stderr)
sys.exit(completed.returncode)
"#,
    );
    let agent_command = serde_json::to_string(&vec![
        python_command().to_string(),
        agent.display().to_string(),
        rhei_binary().display().to_string(),
    ])
    .expect("serialize mock agent command");
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{
    "mock": {{ "command": {agent_command}, "stdin_prompt": true, "timeout": "30s" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Outer Worker

## Tasks

### Task 1: Inspect isolation
**State:** work
"#,
    );
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: outer-agent-environment
version: 1
states:
  work:
    initial: true
    agent: mock
    instructions: |
      Task identity: {task_id}
      Prompt evidence output: {output.prompt-evidence.path}
    outputs:
      - name: prompt-evidence
        path: runtime/prompt-evidence.md
  human-review:
    gating: true
  completed:
    final: true
transitions:
  - from: work
    to: human-review
  - from: human-review
    to: completed
"#,
    );

    let outer_runtime = dir.join("runtime/nested-marker.txt");
    let outer_result = dir.join("runtime/results/plan.1.md");
    fs::create_dir_all(outer_result.parent().expect("result parent")).expect("outer runtime");
    fs::write(&outer_runtime, OUTER_RUNTIME_SENTINEL).expect("outer runtime sentinel");
    fs::write(&outer_result, OUTER_RESULT_SENTINEL).expect("outer result sentinel");

    let output = rhei_command(dir.join(".home"))
        .current_dir(&dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().expect("fixture has a temporary parent"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&plan_path)
        .args(["--no-tui", "--no-callbacks"])
        .output()
        .expect("outer rhei run");
    let stdout = stdout(&output);
    let stderr = stderr(&output);
    assert!(output.status.success(), "outer run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");

    let prompt = fs::read_to_string(dir.join("runtime/prompt-evidence.md"))
        .expect("worker records its authoritative prompt");
    assert!(prompt.contains("Task plan.1: Inspect isolation"), "task missing:\n{prompt}");
    assert!(prompt.contains("Task identity: plan.1"), "identity missing:\n{prompt}");
    assert!(
        prompt.contains(&format!(
            "You are working in a rhei-managed plan at `{}`.",
            plan_path.display()
        )),
        "plan path missing:\n{prompt}"
    );
    assert!(
        prompt.contains("Prompt evidence output: runtime/prompt-evidence.md"),
        "output path missing:\n{prompt}"
    );

    let observed: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("observed-env.json")).expect("child records environment"),
    )
    .expect("recorded environment is JSON");
    let identity_names =
        ["RHEI_ROOT", "RHEI_PLAN_PATH", "RHEI_RESULT_PATH", "RHEI_TASK_ID", "RHEI_TASK_ID_LOCAL"];
    let leaked = identity_names
        .iter()
        .filter_map(|name| observed[*name].as_str().map(|value| (*name, value)))
        .collect::<Vec<_>>();
    let outer_runtime_after = fs::read(&outer_runtime).expect("read outer runtime sentinel");
    let outer_result_after = fs::read(&outer_result).expect("read outer result sentinel");
    let nested_marker = inner.join("runtime/nested-marker.txt");
    let nested_result = inner.join("runtime/results/plan.1.md");

    assert!(
        leaked.is_empty()
            && outer_runtime_after == OUTER_RUNTIME_SENTINEL
            && outer_result_after == OUTER_RESULT_SENTINEL
            && fs::read(&nested_marker).is_ok_and(|bytes| bytes == b"nested workspace\n")
            && nested_result.exists(),
        "autonomous worker leaked outer Rhei identity into its child:\n\
         leaked variables: {leaked:?}\n\
         outer runtime sentinel: {:?}\n\
         outer result sentinel: {:?}\n\
         nested marker exists: {}\n\
         nested result exists: {}\n\
         nested command:\n{}",
        String::from_utf8_lossy(&outer_runtime_after),
        String::from_utf8_lossy(&outer_result_after),
        nested_marker.exists(),
        nested_result.exists(),
        fs::read_to_string(dir.join("nested-command.txt")).unwrap_or_default(),
    );
}
