//! Two shapes of agent-grounds/rhei#310 that one culprit alone does not reach:
//! two `--parallel` workers that each break their own region, and a poll state
//! whose worker breaks the plan on every attempt. §FS-rhei-run.3.7

use std::fs;
use std::path::{Path, PathBuf};

use super::python_fixture::fixture_command_with_args;
use super::worker_edit_revert_support::{
    end_records, journal_in, meta_value, portable, slot_released, state_in,
};
use super::*;

/// Each task appends a heading to its own task file on its first attempt.
/// Task 2 writes only once Task 1 has, so the loader reports Task 1's file
/// first; Task 1 stays alive until Task 2 has exited, so Task 2's reload meets
/// Task 1's break while Task 1 still runs, and waits on it.
const CULPRIT: &str = r#"n = sys.argv[1]
here = pathlib.Path(__file__).parent
own = here / 'ws' / 'tasks' / ('0' + n + '-t.md')
marker = here / ('noted-' + n)

def wait_for(path):
    deadline = time.time() + 15
    while not path.exists() and time.time() < deadline:
        time.sleep(0.05)

if not marker.exists():
    if n == '2':
        wait_for(here / 'noted-1')
    append(own, '\n#### Visit 1 (cover)\n\nnote ' + n + '\n')
    marker.write_text('x')
    if n == '1':
        wait_for(here / 'exited-2')
        time.sleep(1)
append(here / ('runs-' + n + '.txt'), 'ran\n')
if n == '2':
    write(here / 'exited-2', 'x')
result('Task ' + n + ' wrote its note.\n')
"#;

fn culprit_machine(one: &str, two: &str) -> String {
    format!(
        r#"name: two-culprits
version: 1
states:
  note1:
    description: Writes a heading into its own task body.
    program:
      command: {one}
    program_timeout: 30s
  note2:
    description: Writes a heading into its own task body.
    program:
      command: {two}
    program_timeout: 30s
  completed:
    description: Done.
    final: true
  failed:
    description: Failed.
    final: true
transitions:
  - from: note1
    to: completed
    exit_code: 0
  - from: note2
    to: completed
    exit_code: 0
  - from: "*"
    to: failed
"#
    )
}

/// A directory workspace `ws` beside the scripts in `dir`, where they find
/// it, with one task file per task and its machine.
fn workspace(dir: &Path, tasks: &[(&str, &str)], machine: &str) -> (PathBuf, PathBuf) {
    let ws = dir.join("ws");
    fs::create_dir_all(ws.join("tasks")).expect("create workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Worker edit race\n").expect("write index");
    for (file, text) in tasks {
        fs::write(ws.join("tasks").join(file), text).expect("write task");
    }
    let machine = write_fixture_file(&ws, "states.yaml", machine);
    (ws, machine)
}

fn task_text(ws: &Path, file: &str) -> String {
    fs::read_to_string(ws.join("tasks").join(file)).expect("read task file")
}

fn runs(dir: &Path, name: &str) -> usize {
    fs::read_to_string(dir.join(name)).map(|text| text.lines().count()).unwrap_or(0)
}

/// A held completion keeps its own region: when the worker that exited first
/// waits on a culprit still running, and that culprit's exit restores its own
/// region, the waiting completion's own break is restored and charged too, not
/// read as a break the run must stop on. Both attempts are journalled failed.
// §FS-rhei-run.3.7.5 §FS-rhei-run.3.7.2 §FS-rhei-run.3.7.4
#[test]
fn two_parallel_culprits_each_have_their_own_edit_reverted() {
    let one_text = "### Task 1: One\n**State:** note1\n\nBody one.\n";
    let two_text = "### Task 2: Two\n**State:** note2\n\nBody two.\n";
    let dir = unique_temp_dir("worker-edit-two-culprits");
    let script = write_python_agent(&dir, "culprit.py", CULPRIT);
    let machine = culprit_machine(
        &fixture_command_with_args(&script, &["1"]),
        &fixture_command_with_args(&script, &["2"]),
    );
    let (ws, machine) = workspace(&dir, &[("01-t.md", one_text), ("02-t.md", two_text)], &machine);
    let args = ["--no-tui", "--no-callbacks", "--parallel", "2"];

    // Both tickets stall on their reverted attempts and nothing else moves, so
    // the retries are the next run's. §FS-rhei-run.3.6
    let first = run_cli("run", &ws, &machine, &args);
    let first_released = slot_released(&ws);
    let second = run_cli("run", &ws, &machine, &args);
    let output = portable(&format!(
        "first:\n{}{}\nsecond:\n{}{}",
        first.stdout, first.stderr, second.stdout, second.stderr
    ));

    assert!(!output.contains("text broke the plan"), "no attributed stop\n{output}");
    for (task, file) in [("ws.1", "01-t.md"), ("ws.2", "02-t.md")] {
        assert!(
            output.contains(&format!(
                "Task {task}'s edit to its own task body broke the plan at tasks/{file}:6"
            )),
            "each culprit's own edit is reverted\n{output}"
        );
    }
    assert!(second.status.success(), "the retries complete\n{output}");
    assert_eq!(state_in(&task_text(&ws, "01-t.md")), "completed", "{output}");
    assert_eq!(state_in(&task_text(&ws, "02-t.md")), "completed", "{output}");
    assert!(!task_text(&ws, "01-t.md").contains("#### Visit"), "Task 1's heading is reverted");
    assert!(!task_text(&ws, "02-t.md").contains("#### Visit"), "Task 2's heading is reverted");
    assert_eq!(runs(&dir, "runs-1.txt"), 2, "{output}");
    assert_eq!(runs(&dir, "runs-2.txt"), 2, "{output}");

    let journal = journal_in(&ws);
    for (task, state, file) in [("ws.1", "note1", "01-t.md"), ("ws.2", "note2", "02-t.md")] {
        let ends = end_records(&journal, task, state);
        assert_eq!(ends.len(), 2, "{journal}");
        assert_eq!(meta_value(&ends[0], "outcome").as_deref(), Some("failed"), "{journal}");
        let location = format!("tasks/{file}:6");
        assert_eq!(meta_value(&ends[0], "reverted"), Some(location.clone()), "{journal}");
        assert_eq!(meta_value(&ends[1], "outcome").as_deref(), Some("completed"), "{journal}");
        // The first run's JSON release agrees with the journal. §FS-rhei-run-json.2.1
        assert!(
            first_released.iter().any(|record| record["task"] == task
                && record["outcome"] == "failed"
                && record["reverted"] == location.as_str()),
            "{task}'s reverted release is failed in events.jsonl: {first_released:#?}"
        );
    }
}

/// Appends a heading to its own task body and asks to be polled again.
const POLL_CULPRIT: &str = r#"here = pathlib.Path(__file__).parent
append(here / 'ws' / 'tasks' / '01-t.md', '\n#### Visit (poll)\n\nnote\n')
append(here / 'poll-runs.txt', 'ran\n')
sys.exit(75)
"#;

fn poll_machine(command: &str) -> String {
    format!(
        r#"name: poll-culprit
version: 1
states:
  waiting:
    description: Polls, and breaks the plan every attempt.
    program:
      command: {command}
    program_timeout: 30s
    poll:
      interval: 0s
      max_attempts: 3
  gate:
    gating: true
    description: A person looks.
  done:
    description: Done.
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
    description: Not yet.
  - from: waiting
    to: gate
    condition: pollAttempts >= pollMaxAttempts
    description: Out of attempts.
  - from: gate
    to: done
    description: Looked at.
"#
    )
}

/// On a poll state the reverted attempts spend `poll.max_attempts`: once they
/// have, the ticket stalls in its state across runs, the halt names it, and the
/// exhaustion edge never fires. §FS-rhei-run.3.7.4 §REQ-bounded-neural-work.1
#[test]
fn a_poll_worker_that_breaks_the_plan_every_time_stalls_at_max_attempts() {
    let dir = unique_temp_dir("worker-edit-poll-culprit");
    let script = write_python_agent(&dir, "poll.py", POLL_CULPRIT);
    let task = "### Task 1: One\n**State:** waiting\n\nBody.\n";
    let machine = poll_machine(&fixture_command_with_args(&script, &[]));
    let (ws, machine) = workspace(&dir, &[("01-t.md", task)], &machine);

    let mut output = String::new();
    let mut last = None;
    for _ in 0..4 {
        let ran = run_cli("run", &ws, &machine, &["--no-tui", "--no-callbacks"]);
        output.push_str(&portable(&format!("{}{}\n---\n", ran.stdout, ran.stderr)));
        last = Some(ran);
    }
    let last = last.expect("ran");

    assert_eq!(runs(&dir, "poll-runs.txt"), 3, "spawns never exceed max_attempts\n{output}");
    assert!(!output.contains("attempt 4 of 3"), "{output}");
    assert!(output.contains("attempt 3 of 3 (poll.max_attempts) spent"), "{output}");
    assert_eq!(state_in(&task_text(&ws, "01-t.md")), "waiting", "no edge fires\n{output}");
    assert!(!task_text(&ws, "01-t.md").contains("#### Visit"), "every edit is reverted");
    assert!(!last.status.success(), "the stalled ticket fails the run\n{output}");
    let last_output = format!("{}{}", last.stdout, last.stderr);
    assert!(
        last_output.contains("halting Task ws.1 in state 'waiting': 3 attempts spent"),
        "the halt names the culprit\n{output}"
    );
}
