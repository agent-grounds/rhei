//! Programs admitted into an existing project run keep their member runtime
//! while their slot events remain in the original project journal.
//! §FS-rhei-panta.6.2 §FS-rhei-run.3 §FS-rhei-run.5

use std::fs;
use std::path::Path;

use super::python_fixture::fixture_command_with_args;
use super::*;

fn program_machine(name: &str, state: &str, command: &str) -> String {
    format!(
        r#"name: {name}
version: 1
states:
  {state}:
    initial: true
    description: Execute local program
    concurrent: true
    program:
      command: {command}
    program_timeout: 20s
  delivered:
    final: true
    description: Complete
transitions:
  - from: {state}
    to: delivered
    exit_code: 0
"#
    )
}

fn write_program_handoff(project: &Path, parallel: bool) {
    let seed = project.join("seed");
    fs::create_dir_all(seed.join("tasks")).unwrap();
    write_fixture_file(project, "index.panta.md", "# Panta: Program Admission\n");
    let template = project.join(".agent-grounds/rhei/templates/program-follow-on");
    fs::create_dir_all(template.join("tasks")).unwrap();
    let follow_program = write_python_agent(
        project,
        "follow-program.py",
        r#"import json

root = pathlib.Path.cwd()
project = root.parent
original = json.loads((project / 'original-run.json').read_text(encoding='utf-8'))
live = json.loads((project / 'runtime/run.json').read_text(encoding='utf-8'))
assert live['status'] == 'running'
assert live['id'] == original['id']
assert live['pid'] == original['pid']
if (project / 'parallel-probe').exists():
    assert (project / 'seed-blocker-started').is_file()
    assert not (project / 'seed-blocker-finished').exists()
    write(project / 'release-seed-blocker', 'admitted program released the blocker\n')
write(root / 'observation.json', json.dumps({
    'root': str(root),
    'task': os.environ['RHEI_TASK_ID'],
    'plan': os.environ['RHEI_PLAN_PATH'],
    'result': os.environ['RHEI_RESULT_PATH'],
    'run_id': live['id'],
    'run_pid': live['pid'],
}))
print('admitted program stdout', flush=True)
print('admitted program stderr', file=sys.stderr, flush=True)
result('admitted program completed\n')
"#,
    );
    write_fixture_file(
        &template,
        "template.yaml",
        "name: program-follow-on\nversion: 1.0.0\ndescription: Admitted program\n",
    );
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Follow\n**States:** follow-program-machine\n",
    );
    write_fixture_file(
        &template,
        "states.yaml",
        &program_machine("follow-program-machine", "run", &fixture_command(&follow_program)),
    );
    write_fixture_file(
        &template,
        "tasks/01-follow.md",
        "### Task 1: Admitted program\n**State:** run\n",
    );

    let producer = write_python_agent(
        project,
        "publish-program.py",
        r#"import json
import subprocess

project = pathlib.Path(sys.argv[3])
parallel = (project / 'parallel-probe').exists()

def wait_for(name):
    deadline = time.monotonic() + 10.0
    while not (project / name).exists():
        if time.monotonic() >= deadline:
            raise RuntimeError('barrier was not released: ' + name)
        time.sleep(0.02)

if os.environ['RHEI_TASK_ID_LOCAL'] == '2':
    write(project / 'seed-blocker-started', 'started\n')
    wait_for('release-seed-blocker')
    write(project / 'seed-blocker-finished', 'finished\n')
    result('seed blocker completed\n')
else:
    if parallel:
        wait_for('seed-blocker-started')
    assert not (project / 'follow').exists()
    original = json.loads((project / 'runtime/run.json').read_text(encoding='utf-8'))
    assert original['status'] == 'running'
    write(project / 'original-run.json', json.dumps(original))
    completed = subprocess.run(
        [sys.argv[1], 'instantiate', sys.argv[2], '--output', str(project / 'follow')],
        cwd=project, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    print(completed.stdout, end='')
    print(completed.stderr, end='', file=sys.stderr)
    if completed.returncode != 0:
        sys.exit(completed.returncode)
    result('published the program member\n')
"#,
    );
    let command = fixture_command_with_args(
        &producer,
        &[rhei_binary().to_str().unwrap(), template.to_str().unwrap(), project.to_str().unwrap()],
    );
    write_fixture_file(&seed, "index.rhei.md", "# Rhei: Seed\n**States:** seed-machine\n");
    write_fixture_file(&seed, "states.yaml", &program_machine("seed-machine", "publish", &command));
    for task in 1..=if parallel { 2 } else { 1 } {
        write_fixture_file(
            &seed,
            &format!("tasks/{task:02}-seed.md"),
            &format!("### Task {task}: Seed program {task}\n**State:** publish\n"),
        );
    }
    if parallel {
        write_fixture_file(project, "parallel-probe", "hold the second seed slot\n");
    }
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("missing program routing evidence {}: {err}", path.display())
        }),
    )
    .expect("valid JSON evidence")
}

fn assert_program_handoff(parallel: bool) {
    let temp = unique_temp_dir("live-member-program");
    let project = temp.join("project");
    write_program_handoff(&project, parallel);
    let home = temp.join("home");
    let output = rhei_command(&home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .current_dir(&project)
        .args(["run", ".", "--no-tui", "--parallel", if parallel { "2" } else { "1" }])
        .output()
        .expect("project run should start");
    assert_success(&CliRun::from(&output));

    let follow = project.join("follow");
    let task = fs::read_to_string(follow.join("tasks/01-follow.md")).unwrap();
    assert!(task.contains("**State:** delivered"), "admitted program did not complete: {task}");
    let observed = read_json(&follow.join("observation.json"));
    assert_eq!(observed["task"], "follow.1");
    assert_same_path(Path::new(observed["root"].as_str().unwrap()), &follow);
    assert_same_path(Path::new(observed["plan"].as_str().unwrap()), &follow);
    let result = follow.join("runtime/results/follow.1.md");
    assert_same_path(Path::new(observed["result"].as_str().unwrap()), &result);
    assert!(fs::read_to_string(result).unwrap().contains("admitted program completed"));
    let log = follow.join("runtime/logs/task-follow.1-run.log");
    let transcript = fs::read_to_string(&log).expect("member program transcript");
    assert!(transcript.contains("admitted program stdout"));
    assert!(transcript.contains("admitted program stderr"));
    let spawn = read_json(&follow.join("runtime/spawns/task-follow.1-run.json"));
    assert_eq!(spawn["task"], "follow.1");
    assert_eq!(spawn["state"], "run");
    assert_eq!(spawn["kind"], "program");
    assert_eq!(spawn["attempt"], 1);
    assert_eq!(spawn["code"], 0);
    assert_eq!(spawn["ending"], "exited");
    assert_same_path(Path::new(spawn["log"].as_str().unwrap()), &log);
    for directory in ["logs", "spawns"] {
        let path = project.join("runtime").join(directory);
        if path.exists() {
            assert!(
                fs::read_dir(path).unwrap().all(|entry| {
                    !entry.unwrap().file_name().to_string_lossy().starts_with("task-follow.1-")
                }),
                "admitted program artifacts leaked into the project {directory}"
            );
        }
    }

    let original = read_json(&project.join("original-run.json"));
    let descriptor = read_json(&project.join("runtime/run.json"));
    assert_eq!(descriptor["id"], original["id"]);
    assert_eq!(descriptor["pid"], original["pid"]);
    assert_eq!(descriptor["status"], "finished");
    assert_eq!(descriptor["exit_code"], 0);
    assert_eq!(observed["run_id"], original["id"]);
    assert_eq!(observed["run_pid"], original["pid"]);
    for member in [&follow, &project.join("seed")] {
        assert!(!member.join("runtime/run.json").exists(), "no sibling run descriptor");
        assert!(!member.join("runtime/events.jsonl").exists(), "journal stays at project scope");
    }
    let events: Vec<serde_json::Value> = fs::read_to_string(project.join("runtime/events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let starts: Vec<_> = events.iter().filter(|event| event["event"] == "run_started").collect();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["run_id"], original["id"]);
    assert_same_path(Path::new(starts[0]["workspace"].as_str().unwrap()), &project);
    let position = |kind: &str, task: &str| {
        let matches: Vec<_> = events
            .iter()
            .enumerate()
            .filter(|(_, event)| event["event"] == kind && event["task"] == task)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(matches.len(), 1, "one {kind} for {task}");
        matches[0]
    };
    let published = position("slot_released", "seed.1");
    let admitted = position("slot_assigned", "follow.1");
    let completed = position("slot_released", "follow.1");
    assert!(published < admitted && admitted < completed);
    for event in [&events[admitted], &events[completed]] {
        assert_same_path(&project.join(event["log_path"].as_str().unwrap()), &log);
    }
    if parallel {
        assert!(project.join("seed-blocker-finished").is_file());
        let blocker_started = position("slot_assigned", "seed.2");
        let blocker_finished = position("slot_released", "seed.2");
        assert!(blocker_started < admitted && admitted < blocker_finished);
        assert_eq!(events[published]["slot"], events[admitted]["slot"]);
        assert_ne!(events[blocker_started]["slot"], events[admitted]["slot"]);
    }
}

#[test]
fn issue_205_sequential_admitted_program_keeps_member_runtime_in_the_original_run() {
    assert_program_handoff(false);
}

#[test]
fn issue_205_parallel_refill_keeps_admitted_program_runtime_in_the_original_run() {
    assert_program_handoff(true);
}
