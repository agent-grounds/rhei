//! Shapes of agent-grounds/rhei#310 that one culprit alone does not reach: two
//! `--parallel` workers that each break their own region, three whose held
//! exits meet a stop, and a poll state whose worker breaks the plan on every
//! attempt. §FS-rhei-run.3.7

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

/// Each task appends a heading to its own task file, and Task 1 to Task 4's as
/// well, which no worker holds. Task 2 and Task 3 exit while Task 1 runs, so
/// both are held. Task 3 writes once Task 2 was reaped, so Task 2's completion
/// is the one whose reload waits on Task 1. Task 1 exits once Task 3 was reaped.
const HELD_CULPRIT: &str = r#"import json
n = sys.argv[1]
here = pathlib.Path(__file__).parent
spawns = here / 'ws' / 'runtime' / 'spawns'

def wait_for(ready):
    deadline = time.time() + 15
    while not ready() and time.time() < deadline:
        time.sleep(0.02)

def reaped(m):
    record = spawns / ('task-ws.' + m + '-note' + m + '.json')
    try:
        return bool(json.loads(record.read_text())['ending'])
    except (OSError, ValueError, KeyError):
        return False

def note(m):
    append(here / 'ws' / 'tasks' / ('0' + m + '-t.md'), '\n#### Visit 1 (cover)\n\nnote\n')

write(here / ('started-' + n), 'x')
if n == '1':
    wait_for(lambda: (here / 'started-2').exists() and (here / 'started-3').exists())
    note('1')
    note('4')
    write(here / 'noted-1', 'x')
    wait_for(lambda: reaped('3'))
else:
    wait_for(lambda: (here / 'noted-1').exists())
    if n == '3':
        wait_for(lambda: reaped('2'))
    note(n)
result('Task ' + n + ' wrote its note.\n')
"#;

fn held_culprit_machine(script: &Path) -> String {
    let command = |n: &str| fixture_command_with_args(script, &[n]);
    let (one, two, three) = (command("1"), command("2"), command("3"));
    format!(
        r#"name: held-culprits
version: 1
states:
  note1:
    description: Breaks its own task body and Task 4's.
    program:
      command: {one}
    program_timeout: 30s
  note2:
    description: Breaks its own task body.
    program:
      command: {two}
    program_timeout: 30s
  note3:
    description: Breaks its own task body.
    program:
      command: {three}
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
  - from: note3
    to: completed
    exit_code: 0
  - from: "*"
    to: failed
"#
    )
}

/// A held exit whose region one reload restored on its behalf, before the same
/// reload stopped the run on a break no worker holds, is a reverted attempt
/// that ended on its own: failed, charged, and warned about once. The held exit
/// whose own reload stopped the run is interrupted and uncharged. Which of
/// Task 2 and Task 3 the main thread reads first is the channel's order, which
/// no file marks, so the two roles are asserted whichever task took them.
// §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.6
#[test]
fn a_held_exit_restored_for_it_before_a_stop_is_a_failed_reverted_attempt() {
    let dir = unique_temp_dir("worker-edit-held-then-stop");
    let script = write_python_agent(&dir, "held.py", HELD_CULPRIT);
    let text = |n: u8, state: &str| format!("### Task {n}: T{n}\n**State:** {state}\n\nBody.\n");
    let tasks = [text(1, "note1"), text(2, "note2"), text(3, "note3"), text(4, "completed")];
    let files = ["01-t.md", "02-t.md", "03-t.md", "04-t.md"];
    let named: Vec<(&str, &str)> =
        files.iter().copied().zip(tasks.iter().map(String::as_str)).collect();
    let (ws, machine) = workspace(&dir, &named, &held_culprit_machine(&script));

    let ran = run_cli("run", &ws, &machine, &["--no-tui", "--no-callbacks", "--parallel", "3"]);
    let output = portable(&format!("{}{}", ran.stdout, ran.stderr));
    let journal = journal_in(&ws);
    let released = slot_released(&ws);

    assert!(!ran.status.success(), "the run stops\n{output}");
    assert!(output.contains("Task ws.4"), "names the task whose text broke\n{output}");
    assert!(output.contains("tasks/04-t.md:6"), "names file:line\n{output}");
    for (task, file) in [("ws.1", "01-t.md"), ("ws.2", "02-t.md"), ("ws.3", "03-t.md")] {
        let warning =
            format!("Task {task}'s edit to its own task body broke the plan at tasks/{file}:6");
        assert_eq!(output.matches(&warning).count(), 1, "{task} is warned once\n{output}");
        assert!(!task_text(&ws, file).contains("#### Visit"), "{task}'s edit is reverted");
    }
    assert!(task_text(&ws, "04-t.md").contains("#### Visit"), "the stop reverts nothing");

    let outcome = |task: &str, state: &str| {
        let ends = end_records(&journal, task, state);
        assert_eq!(ends.len(), 1, "{task} is journalled once\n{journal}");
        (meta_value(&ends[0], "outcome"), meta_value(&ends[0], "reverted"))
    };
    let failed = |file: &str| (Some("failed".to_string()), Some(format!("tasks/{file}:6")));
    assert_eq!(outcome("ws.1", "note1"), failed("01-t.md"), "{journal}");
    let two = outcome("ws.2", "note2");
    let behalf: u8 = if two.0.as_deref() == Some("interrupted") { 3 } else { 2 };
    let own = 5 - behalf;
    let (behalf_task, behalf_state, behalf_file) =
        (format!("ws.{behalf}"), format!("note{behalf}"), format!("0{behalf}-t.md"));
    let interrupted = (Some("interrupted".to_string()), Some(format!("tasks/0{own}-t.md:6")));
    assert_eq!(outcome(&format!("ws.{own}"), &format!("note{own}")), interrupted, "{journal}");
    assert_eq!(outcome(&behalf_task, &behalf_state), failed(&behalf_file), "{journal}");

    // The JSON release agrees with the journal. §FS-rhei-run-json.2.1
    let location = format!("tasks/{behalf_file}:6");
    assert!(
        released.iter().any(|record| record["task"] == behalf_task.as_str()
            && record["outcome"] == "failed"
            && record["reverted"] == location.as_str()),
        "{behalf_task}'s release is failed and reverted: {released:#?}"
    );
    let record = |n: u8| {
        let path = ws.join(format!("runtime/spawns/task-ws.{n}-note{n}.json"));
        let body = fs::read_to_string(&path).expect("read spawn record");
        serde_json::from_str::<serde_json::Value>(&body).expect("parse spawn record")
    };
    assert_eq!(record(behalf)["charged"], 1, "the restored attempt is charged");
    assert_eq!(record(behalf)["attempt_charged"], true);
    assert_eq!(record(own)["charged"], 0, "the stopped attempt is not");
    assert_eq!(record(own)["ending"], "interrupted");
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
      interval: 3s
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
/// exhaustion edge never fires. No run waits for an attempt that will never
/// spawn. §FS-rhei-run.3.7.4 §REQ-bounded-neural-work.1
#[test]
fn a_poll_worker_that_breaks_the_plan_every_time_stalls_at_max_attempts() {
    let dir = unique_temp_dir("worker-edit-poll-culprit");
    let script = write_python_agent(&dir, "poll.py", POLL_CULPRIT);
    let task = "### Task 1: One\n**State:** waiting\n\nBody.\n";
    let machine = poll_machine(&fixture_command_with_args(&script, &[]));
    let (ws, machine) = workspace(&dir, &[("01-t.md", task)], &machine);

    let mut output = String::new();
    let mut stdouts = Vec::new();
    let mut last = None;
    for _ in 0..4 {
        let ran = run_cli("run", &ws, &machine, &["--no-tui", "--no-callbacks"]);
        output.push_str(&portable(&format!("{}{}\n---\n", ran.stdout, ran.stderr)));
        stdouts.push(ran.stdout.clone());
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
    // The sleep and the spawn are both told on stdout, in the order they happen.
    let mut after_last = stdouts.iter().filter_map(|out| out.split("attempt 3 of 3 (").nth(1));
    assert!(
        after_last.all(|rest| !rest.contains("sleeping")),
        "no run waits once the bound is spent\n{output}"
    );
    let halts = output.lines().filter(|line| line.contains("halting Task ws.1"));
    assert!(
        halts.into_iter().all(|line| line.contains("does not load with its edit")),
        "every halt names the reverted edit as what is owed\n{output}"
    );
}
