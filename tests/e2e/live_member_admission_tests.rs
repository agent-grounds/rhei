//! A project run whose last known worker publishes its successor. These are
//! black-box tests because the contract is the whole handoff: template
//! publication, graph reload, member initialization, locking, attribution,
//! scheduling, and bounded termination.

use std::fs;
use std::path::{Path, PathBuf};

use super::python_fixture::fixture_command_with_args;
use super::*;

struct HandoffFixture {
    _temp: TestDir,
    project: PathBuf,
    home: PathBuf,
}

fn run_project(fixture: &HandoffFixture, args: &[&str]) -> CliRun {
    let output = rhei_command(&fixture.home)
        .current_dir(&fixture.project)
        .arg("run")
        .arg(".")
        .args(args)
        .output()
        .expect("project run should start");
    CliRun::from(&output)
}

fn write_follow_on_template(project: &Path, tasks: usize, parallel_probe: bool) -> PathBuf {
    let agent = write_python_agent(
        project,
        "follow-on-agent.py",
        if parallel_probe {
            r#"import json

root = pathlib.Path.cwd()
project = root.parent
local = env('RHEI_TASK_ID_LOCAL')
marker = project / ('follow-started-' + local)
write(marker, local + '\n')

if local == '1':
    # Task 2 must not start while the seed blocker still occupies the other
    # --parallel 2 slot. The marker is the evidence; the deadline only bounds
    # how long this deterministic barrier may hold the test.
    deadline = time.monotonic() + 1.0
    while time.monotonic() < deadline and not (project / 'follow-started-2').exists():
        time.sleep(0.02)
    if (project / 'follow-started-2').exists():
        result('follow-on work exceeded the two-slot limit\n')
        sys.exit(23)
    write(project / 'release-seed-blocker', 'release\n')
else:
    if not (project / 'seed-blocker-finished').exists():
        result('task 2 started before the occupied slot was released\n')
        sys.exit(24)

write(root / 'follow-observation-' / (local + '.json'), json.dumps({
    'task': env('RHEI_TASK_ID'),
    'plan': env('RHEI_PLAN_PATH'),
    'cwd': str(root),
    'result': env('RHEI_RESULT_PATH'),
}, sort_keys=True))
result('follow-on task ' + local + ' completed\n')
"#
        } else {
            r#"import json

root = pathlib.Path.cwd()
lock_path = root / '.rhei' / 'run.lock'
lock_path.parent.mkdir(parents=True, exist_ok=True)
with lock_path.open('a+b') as lock:
    acquired = False
    try:
        if os.name == 'nt':
            import msvcrt
            lock.seek(0)
            if lock.read(1) == b'':
                lock.write(b'0')
                lock.flush()
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        acquired = True
    except (BlockingIOError, OSError):
        pass
    finally:
        if acquired:
            if os.name == 'nt':
                lock.seek(0)
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
    if acquired:
        result('the follow-on execution-root lock was not held before execution\n')
        sys.exit(22)

write(root / 'follow-observation.json', json.dumps({
    'task': env('RHEI_TASK_ID'),
    'plan': env('RHEI_PLAN_PATH'),
    'cwd': str(root),
    'result': env('RHEI_RESULT_PATH'),
}, sort_keys=True))
result('follow-on task completed\n')
"#
        },
    );
    let command = fixture_command(&agent);
    let template = project.join(".agent-grounds/rhei/templates/follow-on");
    fs::create_dir_all(template.join("tasks")).expect("create follow-on template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: follow-on\nversion: 1.0.0\ndescription: Follow-on work\n",
    );
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Follow On\n**States:** follow-on-machine\n",
    );
    write_fixture_file(
        &template,
        "states.yaml",
        r#"name: follow-on-machine
version: 1
models: [follow-on-model]
states:
  review:
    initial: true
    description: Execute the admitted follow-on
    concurrent: true
    target: follow-on[yolo]:fixture:follow-on-model
    agent_timeout: 10s
  delivered:
    final: true
    description: Follow-on complete
transitions:
  - from: review
    to: delivered
"#,
    );
    for task in 1..=tasks {
        write_fixture_file(
            &template,
            &format!("tasks/{task:02}-follow.md"),
            &format!("### Task {task}: Follow-on {task}\n**State:** review\n"),
        );
    }
    write_fixture_file(
        &template,
        "settings.json",
        &format!(
            r#"{{
  "agents": {{
    "follow-on": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "10s",
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "follow-on-model": {{
      "provider": "fixture",
      "model": "follow-on-model",
      "default_agent": "follow-on"
    }}
  }}
}}
"#,
        ),
    );
    template
}

fn handoff_fixture(prefix: &str, parallel: bool) -> HandoffFixture {
    let temp = unique_temp_dir(prefix);
    let project = temp.join("project");
    let seed = project.join("seed");
    fs::create_dir_all(seed.join("tasks")).expect("create seed member");
    write_fixture_file(&project, "index.panta.md", "# Panta: Live Admission\n");
    let template = write_follow_on_template(&project, if parallel { 2 } else { 1 }, parallel);

    let producer = write_python_agent(
        &project,
        "publish-follow-on.py",
        if parallel {
            r#"import subprocess

project = pathlib.Path(sys.argv[3])
local = env('RHEI_TASK_ID_LOCAL')
if local == '1':
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline and not (project / 'seed-blocker-started').exists():
        time.sleep(0.02)
    if not (project / 'seed-blocker-started').exists():
        sys.exit(31)
    completed = subprocess.run(
        [sys.argv[1], 'instantiate', sys.argv[2], '--output', str(project / 'follow')],
        cwd=project,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    write(project / 'instantiate.stdout', completed.stdout)
    write(project / 'instantiate.stderr', completed.stderr)
    if completed.returncode != 0:
        sys.exit(completed.returncode)
    result('published the follow-on member\n')
else:
    write(project / 'seed-blocker-started', 'started\n')
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline and not (project / 'release-seed-blocker').exists():
        time.sleep(0.02)
    write(project / 'seed-blocker-finished', 'finished\n')
    result('seed blocker finished\n')
"#
        } else {
            r#"import subprocess

project = pathlib.Path(sys.argv[3])
completed = subprocess.run(
    [sys.argv[1], 'instantiate', sys.argv[2], '--output', str(project / 'follow')],
    cwd=project,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
write(project / 'instantiate.stdout', completed.stdout)
write(project / 'instantiate.stderr', completed.stderr)
if completed.returncode != 0:
    sys.exit(completed.returncode)
result('published the follow-on member\n')
"#
        },
    );
    let producer_command = fixture_command_with_args(
        &producer,
        &[
            rhei_binary().to_str().expect("binary path is UTF-8"),
            template.to_str().expect("template path is UTF-8"),
            project.to_str().expect("project path is UTF-8"),
        ],
    );
    write_fixture_file(&seed, "index.rhei.md", "# Rhei: Seed\n**States:** seed-machine\n");
    write_fixture_file(
        &seed,
        "states.yaml",
        &format!(
            r#"name: seed-machine
version: 1
states:
  publish:
    initial: true
    description: Publish follow-on work
    concurrent: true
    program:
      command: {producer_command}
    program_timeout: 15s
  finished:
    final: true
    description: Seed complete
transitions:
  - from: publish
    to: finished
    exit_code: 0
"#,
        ),
    );
    let seed_tasks = if parallel { 2 } else { 1 };
    for task in 1..=seed_tasks {
        write_fixture_file(
            &seed,
            &format!("tasks/{task:02}-seed.md"),
            &format!("### Task {task}: Seed {task}\n**State:** publish\n"),
        );
    }
    let home = project.join(".home");
    HandoffFixture { _temp: temp, project, home }
}

fn assert_follow_on_finished(project: &Path, tasks: usize) {
    for task in 1..=tasks {
        let text = fs::read_to_string(project.join(format!("follow/tasks/{task:02}-follow.md")))
            .expect("published follow-on task should exist");
        assert!(
            text.contains("**State:** delivered"),
            "follow.{task} was published but not admitted into the live run:\n{text}"
        );
    }
}

/// The producer's final program action publishes a distinct-machine member;
/// the unrestricted run must initialize and execute it before stopping.
// §FS-rhei-panta.6.2 §FS-rhei-run.2.6 §FS-rhei-run.3
#[test]
fn issue_205_unrestricted_run_admits_a_final_program_handoff_into_the_same_run() {
    let fixture = handoff_fixture("live-member-final-handoff", false);
    let run = run_project(&fixture, &["--no-tui", "--parallel", "1"]);
    assert_success(&run);
    assert_follow_on_finished(&fixture.project, 1);

    let follow = fixture.project.join("follow");
    let observed: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(follow.join("follow-observation.json"))
            .expect("follow-on agent should record its context"),
    )
    .expect("observation is JSON");
    assert_eq!(observed["task"], "follow.1");
    assert_eq!(Path::new(observed["cwd"].as_str().unwrap()), follow);
    assert_eq!(Path::new(observed["plan"].as_str().unwrap()), follow);
    assert!(observed["result"].as_str().unwrap().ends_with("follow/runtime/results/follow.1.md"));
    assert!(follow.join("runtime/logs/task-follow.1-review.log").is_file());
    assert!(follow.join("runtime/results/follow.1.md").is_file());

    let descriptor: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture.project.join("runtime/run.json")).expect("run descriptor"),
    )
    .expect("descriptor JSON");
    let run_id = descriptor["id"].as_str().expect("run id");
    let accounting = fs::read_dir(follow.join("runtime/accounting/invocations"))
        .expect("follow-on accounting root")
        .filter_map(Result::ok)
        .map(|entry| fs::read_to_string(entry.path()).expect("accounting record"))
        .collect::<String>();
    assert!(accounting.contains(run_id), "follow-on accounting belongs to the existing run");
}

/// Narrowing fixes candidate membership at startup even though the producer
/// publishes a valid member and the full project remains loadable.
// §FS-rhei-run.2.5 §FS-rhei-run.2.6
#[test]
fn issue_205_explicit_rhei_selection_does_not_admit_a_new_candidate() {
    let fixture = handoff_fixture("live-member-narrowed", false);
    let run = run_project(&fixture, &["--rhei", "seed", "--no-tui", "--parallel", "1"]);
    assert_success(&run);
    let follow = fs::read_to_string(fixture.project.join("follow/tasks/01-follow.md"))
        .expect("producer should publish the member");
    assert!(follow.contains("**State:** review"), "narrowed run must not execute it:\n{follow}");
    assert!(!fixture.project.join("follow/follow-observation.json").exists());
    assert!(!fixture.project.join("follow/runtime/logs/task-follow.1-review.log").exists());
}

/// A refill after publication uses only the slot freed by the producer. The
/// deterministic seed blocker remains in the other slot until follow.1 proves
/// that follow.2 has not started and releases it.
// §FS-rhei-panta.6.2 §FS-rhei-run.3 §FS-rhei-run.5
#[test]
fn issue_205_parallel_admission_refills_without_exceeding_the_slot_limit() {
    let fixture = handoff_fixture("live-member-parallel-refill", true);
    let run = run_project(&fixture, &["--no-tui", "--parallel", "2"]);
    assert_success(&run);
    assert_follow_on_finished(&fixture.project, 2);
    assert!(fixture.project.join("seed-blocker-finished").is_file());
    assert!(fixture.project.join("follow-started-1").is_file());
    assert!(fixture.project.join("follow-started-2").is_file());
}
