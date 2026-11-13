//! What `rhei run --parallel` counts: tasks, each with every arm its state
//! fans out to (#673).
//!
//! The stand-in agent proves the two halves without sleeping on a guess. Each
//! arm marks itself alive, notes how many agents are alive, and then waits for
//! its sibling arm to arrive before it leaves: arms run together or the wait
//! times out and says so. How many were ever alive at once then shows whether
//! the cap held the run to one task at a time.

// §FS-rhei-run.5.3 §FS-rhei-run.5.2

use std::fs;

use super::*;

/// Two tasks, each in a concurrent state that fans out to two reviewers, so a
/// cap on agents and a cap on tasks give different answers under `--parallel 1`.
const MACHINE: &str = r#"name: parallel-task-cap
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    concurrent: true
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;

/// One rhei per task, so the run is not dropped to one ticket at a time by the
/// one-plan-file rule (§FS-rhei-run.2.5) rather than by `--parallel`.
const RHEIS: &[(&str, &str)] = &[
    (
        "topic1.rhei.md",
        "# Rhei: topic1\n\n## Tasks\n\n### Task 1: First topic\n**State:** review\n",
    ),
    (
        "topic2.rhei.md",
        "# Rhei: topic2\n\n## Tasks\n\n### Task 1: Second topic\n**State:** review\n",
    ),
];

/// Each arm: count the agents alive, wait (20 s at most) for the other arm of
/// its own task, record whether it came, and stay alive 2 s more, long enough
/// that a second task let in beside it would be counted, then leave.
const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = re.sub(r'[^A-Za-z0-9]+', '-', env('RHEI_TASK_ID'))
model = env('RHEI_MODEL')
sibling = 'beta' if model == 'alpha' else 'alpha'
live = root / 'live'
arrived = root / 'arrived'
live.mkdir(exist_ok=True)
arrived.mkdir(exist_ok=True)
me = live / (task + '-' + model)
me.touch()
(arrived / (task + '-' + model)).touch()
append(root / 'alive.log', str(len(list(live.iterdir()))) + '\n')
until = time.monotonic() + 20
while not (arrived / (task + '-' + sibling)).exists() and time.monotonic() < until:
    time.sleep(.025)
together = (arrived / (task + '-' + sibling)).exists()
time.sleep(2)
append(root / 'arms.log', task + ' ' + model + ' ' + ('together' if together else 'alone') + '\n')
me.unlink()
result(model + ' reviewed it.\n')
"#;

/// `--parallel 1` admits one task, and that task's two arms run at once: two
/// agents in flight under a cap of one, never four. §FS-rhei-run.5.3
#[test]
fn parallel_one_runs_one_task_with_both_of_its_fan_out_arms_at_once() {
    let dir = unique_temp_dir("parallel-task-cap");
    let root = dir.join("project");
    fs::create_dir_all(&root).expect("create project");
    fs::write(root.join("index.panta.md"), "# Panta: Review round\n").expect("write index");
    for (name, plan) in RHEIS {
        fs::write(root.join(name), plan).expect("write rhei");
    }
    let machine = write_fixture_file(&dir, "parallel-task-cap.yaml", MACHINE);
    let agent = write_python_agent(&dir, "reviewer.py", AGENT);
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings dir");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "60s" }},
  "agents": {{ "mock": {{ "command": {command}, "stdin_prompt": true, "timeout": "60s" }} }},
  "models": {{
    "alpha": {{ "provider": "mock", "model": "alpha", "default_agent": "mock" }},
    "beta": {{ "provider": "mock", "model": "beta", "default_agent": "mock" }}
  }}
}}"#,
            command = fixture_command(&agent)
        ),
    )
    .expect("write settings");

    let run = run_cli("run", &root, &machine, &["--no-tui", "--no-dashboard", "--parallel", "1"]);
    assert_success(&run);
    for (name, _) in RHEIS {
        assert_task_state(&root.join(name), &machine, "1", "completed");
    }

    let arms = fs::read_to_string(root.join("arms.log")).expect("every arm records itself");
    let mut lines: Vec<&str> = arms.lines().collect();
    lines.sort_unstable();
    assert_eq!(lines.len(), 4, "two tasks of two arms each ran; got:\n{arms}");
    assert!(
        lines.iter().all(|line| line.ends_with(" together")),
        "each arm of a scheduled task runs alongside its sibling under --parallel 1; got:\n{arms}"
    );

    let most = fs::read_to_string(root.join("alive.log"))
        .expect("every arm counts the agents alive")
        .lines()
        .filter_map(|line| line.trim().parse::<usize>().ok())
        .max();
    assert_eq!(
        most,
        Some(2),
        "--parallel 1 holds the run to one task, so one task's two arms and no more are alive \
         at once"
    );
}
