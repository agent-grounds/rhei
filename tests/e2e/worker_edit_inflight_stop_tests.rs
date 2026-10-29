//! A `--parallel` run that stops on a break no exited worker made records the
//! worker still in flight before it exits (agent-grounds/rhei#526).
//!
//! The report's journal held Task 1's `start@cover` and no end: the run met
//! Task 1's note in Task 3's body while Task 1 still ran, terminated it, and
//! exited before the thread that reaped it had written its record. Here that
//! window is forced rather than gambled on. A helper takes the lock Task 1's
//! worker needs between the reap and the record, Task 1's spawn-record lock
//! (the one `rhei retry` takes), and holds it until a short while after Task 1
//! is reaped. A run that waits for its worker gets the lock back and writes the
//! record; a run that does not wait has exited by then. §FS-rhei-run.3.2
//!
//! The helper holds the lock with `flock`, which exists only on Unix.
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use super::agent_reentry_support::write_settings;
use super::python_fixture::{fixture_command_with_args, python_command};
use super::worker_edit_revert_support::{
    end_records, journal_in, meta_value, portable, slot_released, COVER, LATER, OTHER,
};
use super::*;

/// Task 1, program or agent: once the helper holds its spawn-record lock it
/// writes the #310 note into Task 3's body, a region no worker runs, and stays
/// in flight until the run's stop terminates it.
const COVER_BODY: &str = r#"here = pathlib.Path(__file__).resolve().parent
write(here / 'cover.pid', str(os.getpid()))
deadline = time.time() + 20
while not (here / 'locked').exists() and time.time() < deadline:
    time.sleep(0.01)
note = '\n#### Visit 1 (cover)\n\nLatest measurement was report-1.json (64.38%, 47/73 lines).\n'
append(here / 'ws' / 'tasks' / '03-later.md', note)
write(here / 'noted', 'x')
# Ended by the run's SIGTERM once it stops; never by this sleep.
time.sleep(30)
result('Task 1 wrote its note.\n')
"#;

/// Task 2, unrelated work that exits once Task 1's note is down, so the run
/// meets the break with Task 1 in flight by either route §FS-rhei-run.3.7.2
/// allows: the reload before Task 2's spawn, or the one after its exit.
const OTHER_BODY: &str = r#"here = pathlib.Path(sys.argv[1])
deadline = time.time() + 20
while not (here / 'noted').exists() and time.time() < deadline:
    time.sleep(0.02)
result('Task 2 did its unrelated work.\n')
"#;

/// How long the helper keeps the lock after Task 1 is reaped. A run that exits
/// without waiting for its worker is gone well inside it; a run that waits
/// pays it once.
const AFTER_REAP_SECONDS: &str = "1.5";

/// Takes Task 1's spawn-record lock as soon as Task 1 has started, before its
/// note exists, and lets go once `rhei run` has exited, or `AFTER_REAP_SECONDS`
/// after Task 1 is reaped, or at a 20 s cap. The reason goes to `released`.
const HOLDER_BODY: &str = r#"import fcntl, os
here = pathlib.Path(sys.argv[1])
after_reap = float(sys.argv[2])
lock_path = here / 'ws' / '.rhei' / 'spawn-locks' / 'task-ws.1-cover.json.lock'
deadline = time.time() + 20
while not (here / 'cover.pid').exists() and time.time() < deadline:
    time.sleep(0.005)
if not (here / 'cover.pid').exists():
    sys.exit('holder: Task 1 never started')
pid = int((here / 'cover.pid').read_text())
lock_path.parent.mkdir(parents=True, exist_ok=True)
fd = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o666)
fcntl.flock(fd, fcntl.LOCK_EX)
write(here / 'locked', 'x')
held_from = time.time()
reaped_at = None
while True:
    if (here / 'rhei-exited').exists():
        reason = 'rhei-exit'
        break
    if time.time() - held_from > 20:
        reason = 'cap'
        break
    if reaped_at is None:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            reaped_at = time.time()
    elif time.time() - reaped_at > after_reap:
        reason = 'after-reap'
        break
    time.sleep(0.01)
write(here / 'released', reason)
fcntl.flock(fd, fcntl.LOCK_UN)
os.close(fd)
"#;

/// Which kind of worker Task 1 is; both run on the `--parallel` worker pool.
#[derive(Clone, Copy, Debug)]
enum Worker {
    Program,
    Agent,
}

fn machine(worker: Worker, cover: &str, other: &str) -> String {
    let cover_state = match worker {
        Worker::Program => {
            format!("    program:\n      command: {cover}\n    program_timeout: 60s")
        }
        Worker::Agent => {
            "    agent: mock\n    agent_timeout: 60s\n    instructions: |\n      Cover.".into()
        }
    };
    let cover_done = match worker {
        Worker::Program => "  - from: cover\n    to: completed\n    exit_code: 0",
        Worker::Agent => "  - { from: cover, to: completed, description: Covered. }",
    };
    format!(
        r#"name: worker-edit-inflight
version: 1
states:
  cover:
    description: Writes a note into another task's body and stays in flight.
{cover_state}
  other:
    description: Unrelated work.
    program:
      command: {other}
    program_timeout: 60s
  completed:
    description: Done.
    final: true
  failed:
    description: Failed.
    final: true
transitions:
{cover_done}
  - from: other
    to: completed
    exit_code: 0
  - from: "*"
    to: failed
"#
    )
}

/// Kills the helper if the test leaves before it let go.
struct Holder(Child);

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Forced {
    _dir: TestDir,
    here: PathBuf,
    ws: PathBuf,
    run: CliRun,
    worker: Worker,
}

impl Forced {
    fn output(&self) -> String {
        format!(
            "worker {:?}, rhei run --parallel 2\nstdout:\n{}\nstderr:\n{}",
            self.worker, self.run.stdout, self.run.stderr
        )
    }

    fn marker(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.here.join(name)).ok()
    }
}

fn run_forced(worker: Worker) -> Forced {
    let dir = unique_temp_dir(&format!("worker-edit-inflight-{worker:?}"));
    let here = dir.to_path_buf();
    let ws = here.join("ws");
    fs::create_dir_all(ws.join("tasks")).expect("create workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Worker edit\n").expect("write index");
    fs::write(ws.join("tasks/01-cover.md"), COVER).expect("write task 1");
    fs::write(ws.join("tasks/02-other.md"), OTHER).expect("write task 2");
    fs::write(ws.join("tasks/03-later.md"), LATER).expect("write task 3");
    let cover = write_python_agent(&here, "cover.py", COVER_BODY);
    let other = write_python_agent(&here, "other.py", OTHER_BODY);
    let holder = write_python_agent(&here, "holder.py", HOLDER_BODY);
    if matches!(worker, Worker::Agent) {
        write_settings(&ws, &cover, false);
    }
    let here_arg = here.display().to_string();
    let text = machine(
        worker,
        &fixture_command_with_args(&cover, &[]),
        &fixture_command_with_args(&other, &[&here_arg]),
    );
    let machine = write_fixture_file(&ws, "states.yaml", &text);

    let mut holder = Holder(
        Command::new(python_command())
            .arg(&holder)
            .arg(&here)
            .arg(AFTER_REAP_SECONDS)
            .stdin(Stdio::null())
            .spawn()
            .expect("the lock holder starts"),
    );
    let run = run_cli("run", &ws, &machine, &["--no-tui", "--no-callbacks", "--parallel", "2"]);
    fs::write(here.join("rhei-exited"), "x").expect("mark rhei's exit");
    let _ = holder.0.wait();
    Forced { _dir: dir, here, ws, run, worker }
}

/// The worker in flight when a `--parallel` run stops on a break in another
/// task's body is journalled `interrupted` before `rhei run` exits, even while
/// its worker thread is still between the reap and the record.
// §FS-rhei-run.3.2 §FS-rhei-run.3.7.6 §FS-rhei-run-tui.1.7 §FS-rhei-run-json.2.1
#[test]
#[ignore = "red until #526: a worker in flight at a stop on a break gets no end@ record; implement removes this"]
fn a_program_in_flight_at_a_stop_on_a_break_is_recorded_before_the_run_exits() {
    assert_recorded_before_exit(Worker::Program);
}

/// The same for an agent worker, which runs on the same worker pool.
// §FS-rhei-run.3.2 §FS-rhei-run.3.7.6 §FS-rhei-run-tui.1.7 §FS-rhei-run-json.2.1
#[test]
#[ignore = "red until #526: a worker in flight at a stop on a break gets no end@ record; implement removes this"]
fn an_agent_in_flight_at_a_stop_on_a_break_is_recorded_before_the_run_exits() {
    assert_recorded_before_exit(Worker::Agent);
}

fn assert_recorded_before_exit(worker: Worker) {
    let ran = run_forced(worker);
    let out = portable(&format!("{}{}", ran.run.stdout, ran.run.stderr));
    let journal = journal_in(&ran.ws);
    let released = ran.marker("released").unwrap_or_default();
    let context = format!("{}\nlock released on: {released}\njournal:\n{journal}", ran.output());

    // The scenario itself happened, or this run proves nothing.
    assert!(!ran.run.status.success(), "the run stops\n{context}");
    assert!(out.contains("tasks/03-later.md:7"), "names file:line\n{context}");
    assert!(out.contains("Task ws.3"), "names the task whose text broke\n{context}");
    assert!(ran.marker("locked").is_some(), "the lock was never taken\n{context}");
    assert!(ran.marker("noted").is_some(), "Task 1 never wrote its note\n{context}");
    assert_ne!(released, "cap", "the window was not forced\n{context}");
    let started = journal.lines().any(|line| {
        let mut columns = line.split_whitespace();
        columns.nth(1) == Some("ws.1") && columns.next() == Some("start@cover")
    });
    assert!(started, "Task 1 never started\n{context}");

    let culprit = end_records(&journal, "ws.1", "cover");
    assert_eq!(culprit.len(), 1, "ws.1 is journalled once\n{context}");
    assert_eq!(meta_value(&culprit[0], "outcome").as_deref(), Some("interrupted"), "{context}");
    let releases = slot_released(&ran.ws);
    assert!(
        releases
            .iter()
            .any(|record| record["task"] == "ws.1" && record["outcome"] == "interrupted"),
        "ws.1's slot is released interrupted: {releases:#?}\n{context}"
    );
}
