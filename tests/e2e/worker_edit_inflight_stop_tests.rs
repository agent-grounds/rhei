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
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::agent_reentry_support::write_settings;
use super::note_store_support::isolated_command;
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

/// The three scripts as a test runs them, wherever it runs them from.
struct Scripts {
    cover: PathBuf,
    other: PathBuf,
    holder: PathBuf,
}

fn write_scripts(here: &Path) -> Scripts {
    Scripts {
        cover: write_python_agent(here, "cover.py", COVER_BODY),
        other: write_python_agent(here, "other.py", OTHER_BODY),
        holder: write_python_agent(here, "holder.py", HOLDER_BODY),
    }
}

/// The helper as every test starts it; where its stderr goes is the test's to say.
fn holder_command(holder: &Path, here: &Path) -> Command {
    let mut command = Command::new(python_command());
    command.arg(holder).arg(here).arg(AFTER_REAP_SECONDS).stdin(Stdio::null());
    command
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
    let scripts = write_scripts(&here);
    if matches!(worker, Worker::Agent) {
        write_settings(&ws, &scripts.cover, false);
    }
    let here_arg = here.display().to_string();
    let text = machine(
        worker,
        &fixture_command_with_args(&scripts.cover, &[]),
        &fixture_command_with_args(&scripts.other, &[&here_arg]),
    );
    let machine = write_fixture_file(&ws, "states.yaml", &text);

    let mut holder =
        Holder(holder_command(&scripts.holder, &here).spawn().expect("the lock holder starts"));
    // The home `run_cli` gives it, without the identity of a `rhei run` the suite runs under: the
    // mock agent's admission would refuse an ancestor reservation this account never made.
    // §FS-rhei-budgets.7.1
    let mut cmd = isolated_command(&here);
    cmd.arg("--state-machine").arg(&machine).arg("run").arg(&ws);
    cmd.args(["--no-tui", "--no-callbacks", "--parallel", "2"]);
    let run = CliRun::from(&cmd.output().expect("rhei run starts"));
    fs::write(here.join("rhei-exited"), "x").expect("mark rhei's exit");
    let _ = holder.0.wait();
    Forced { _dir: dir, here, ws, run, worker }
}

/// The worker in flight when a `--parallel` run stops on a break in another
/// task's body is journalled `interrupted` before `rhei run` exits, even while
/// its worker thread is still between the reap and the record.
// §FS-rhei-run.3.2 §FS-rhei-run.3.7.6 §FS-rhei-run-tui.1.7 §FS-rhei-run-json.2.1
#[test]
fn a_program_in_flight_at_a_stop_on_a_break_is_recorded_before_the_run_exits() {
    assert_recorded_before_exit(Worker::Program);
}

/// The same for an agent worker, which runs on the same worker pool.
// §FS-rhei-run.3.2 §FS-rhei-run.3.7.6 §FS-rhei-run-tui.1.7 §FS-rhei-run-json.2.1
#[test]
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

/// [`FIXTURE_MARGIN`] as a number of seconds.
fn margin_seconds() -> f64 {
    FIXTURE_MARGIN
        .strip_suffix('s')
        .and_then(|seconds| seconds.parse().ok())
        .expect("the margin is written in seconds")
}

/// How long `cover.pid` stays empty: many of the helper's 5 ms polls, and little for a green run to
/// pay. A helper slow to start meets less of it, which can hide the defect and cannot fail a fix.
const HELD_EMPTY: Duration = Duration::from_millis(500);

/// Waits at most `within` for `done`, and stops early if the helper exits. Returns how it exited.
fn watch_holder(
    holder: &mut Holder,
    within: Duration,
    done: impl Fn() -> bool,
) -> Option<ExitStatus> {
    let deadline = Instant::now() + within;
    loop {
        let exited = holder.0.try_wait().expect("the lock holder can be polled");
        if exited.is_some() || done() || Instant::now() >= deadline {
            return exited;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// §REQ-cross-platform.4.1: `cover.pid` exists before it holds Task 1's pid, so the helper that
/// parses it waits for the text. The file is held empty here rather than gambled on, then filled
/// with a live pid, and the helper still takes the lock and lets go cleanly.
/// agent-grounds/rhei#553
#[test]
#[ignore = "red until #553: the helper parses cover.pid the moment it exists; implement removes this"]
fn the_lock_holder_waits_for_the_pid_and_not_for_its_file() {
    let dir = unique_temp_dir("worker-edit-inflight-empty-pid");
    let here = dir.to_path_buf();
    let scripts = write_scripts(&here);
    fs::write(here.join("cover.pid"), "").expect("create cover.pid before its text");
    let stderr_path = here.join("holder.stderr");
    let stderr = fs::File::create(&stderr_path).expect("create the helper's stderr");
    let mut holder = Holder(
        holder_command(&scripts.holder, &here)
            .stderr(stderr)
            .spawn()
            .expect("the lock holder starts"),
    );
    let said = || stderr_from_file(&stderr_path);
    let margin = Duration::from_secs_f64(margin_seconds());

    let died = watch_holder(&mut holder, HELD_EMPTY, || false);
    assert_eq!(died, None, "the helper died while cover.pid was empty:\n{}", said());

    let pid = std::process::id().to_string();
    fs::write(here.join("cover.pid"), pid).expect("fill cover.pid with a live pid");
    let locked = here.join("locked");
    let exited = watch_holder(&mut holder, margin, || locked.exists());
    assert!(locked.exists(), "the lock was never taken, helper {exited:?}:\n{}", said());

    fs::write(here.join("rhei-exited"), "x").expect("mark rhei's exit");
    let exited = watch_holder(&mut holder, margin, || false);
    assert!(
        exited.is_some_and(|status| status.success()),
        "the helper does not end cleanly once rhei has exited, {exited:?}:\n{}",
        said()
    );
    let released = fs::read_to_string(here.join("released")).unwrap_or_default();
    assert_eq!(released, "rhei-exit", "the helper let go for the wrong reason:\n{}", said());
}

/// Runs a script on a clock only its own sleeps move, so a bound of any length is read in no time,
/// and however it is spelled. A sleep under a second is a poll spending a wait; a longer one is a
/// stay. `finally`, because a script that gives up exits through `sys.exit`.
const CLOCK_BODY: &str = r#"import runpy
report, script = sys.argv[1:3]
sys.argv = sys.argv[2:]
now = time.time()
polled = 0.0
stayed = 0.0
def sleep(seconds):
    global now, polled, stayed
    now += seconds
    if seconds < 1:
        polled += seconds
    else:
        stayed += seconds
def clock():
    return now
time.time = time.monotonic = clock
time.sleep = sleep
try:
    runpy.run_path(script, run_name='__main__')
finally:
    write(report, '{} {}'.format(polled, stayed))
"#;

/// What one script spent on [`CLOCK_BODY`]'s clock, and what it printed to stderr.
struct OnTheClock {
    polled: f64,
    stayed: f64,
    stderr: String,
}

/// Runs one script on the clock in a directory of its own, where `cover.pid` holds `cover_pid` if
/// one is given. Every script gets the helper's two arguments; the others read only what they need.
/// Without a run's identity, or `result` writes to the result path of a `rhei run` the suite runs
/// under. §REQ-test-isolation
fn on_the_clock(script: fn(&Scripts) -> &PathBuf, cover_pid: Option<&str>) -> OnTheClock {
    let dir = unique_temp_dir("worker-edit-inflight-clock");
    if let Some(text) = cover_pid {
        fs::write(dir.join("cover.pid"), text).expect("write cover.pid");
    }
    let scripts = write_scripts(&dir);
    let clock = write_python_agent(&dir, "clock.py", CLOCK_BODY);
    let report = dir.join("clock.report");
    let out = Command::new(python_command())
        .arg(&clock)
        .arg(&report)
        .arg(script(&scripts))
        .arg(&*dir)
        .arg(AFTER_REAP_SECONDS)
        .env_remove("RHEI_ATTEMPT")
        .env_remove("RHEI_RESULT_PATH")
        .stdin(Stdio::null())
        .output()
        .expect("the clock starts");
    let stderr = stderr(&out);
    let read = fs::read_to_string(&report).unwrap_or_else(|_| panic!("no report:\n{stderr}"));
    let (polled, stayed) = read.split_once(' ').expect("the report holds two numbers");
    OnTheClock {
        polled: polled.parse().expect("seconds polled"),
        stayed: stayed.parse().expect("seconds stayed"),
        stderr,
    }
}

/// §REQ-cross-platform.8: a correct run never reaches any bound these scripts set, so each of them
/// is the binary's one margin and no second number beside it. The real scripts run on a clock
/// rather than having their text scanned, so a number moved behind a name is still read.
/// `AFTER_REAP_SECONDS` is the forcing rather than a margin (§REQ-cross-platform.8.1), and is never
/// reached here because the pid stays alive. `machine`'s timeouts are read from this source, as
/// `tests_fixture_margins.rs` reads its own, because a number equal to the margin today is still a
/// second number.
#[test]
#[ignore = "red until #553: the scripts wait 20 s and sleep 30 s beside the 60 s margin; implement removes this"]
fn every_bound_in_these_scripts_is_the_fixture_margin() {
    let cover = on_the_clock(|scripts| &scripts.cover, None);
    let other = on_the_clock(|scripts| &scripts.other, None);
    let no_pid = on_the_clock(|scripts| &scripts.holder, None);
    let empty = on_the_clock(|scripts| &scripts.holder, Some(""));
    let live_pid = std::process::id().to_string();
    let capped = on_the_clock(|scripts| &scripts.holder, Some(&live_pid));
    let bounds = [
        ("cover.py waits for `locked`", cover.polled),
        ("cover.py stays in flight", cover.stayed),
        ("other.py waits for `noted`", other.polled),
        ("holder.py waits for cover.pid", no_pid.polled),
        ("holder.py waits for cover.pid to hold text", empty.polled),
        ("holder.py holds the lock", capped.polled),
    ];

    let margin = margin_seconds();
    let mut wrong: Vec<String> = bounds
        .iter()
        .filter(|(_, seconds)| (seconds - margin).abs() >= 1.0)
        .map(|(bound, seconds)| format!("  {bound}: {seconds:.0} s"))
        .collect();
    let source = include_str!("worker_edit_inflight_stop_tests.rs");
    for (index, line) in source.lines().enumerate() {
        let numbered = ["program_timeout: ", "agent_timeout: "].iter().any(|key| {
            line.split(key)
                .nth(1)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        });
        if numbered {
            wrong.push(format!("  line {}: {}", index + 1, line.trim()));
        }
    }
    assert!(
        wrong.is_empty(),
        "bounds in this file that are not FIXTURE_MARGIN ({FIXTURE_MARGIN}):\n{}\n\
         helper's stderr with no cover.pid:\n{}\nhelper's stderr with cover.pid empty:\n{}",
        wrong.join("\n"),
        no_pid.stderr,
        empty.stderr,
    );
    assert!(
        no_pid.stderr.contains("holder: Task 1 never started"),
        "the helper's give-up names why, in the words the reproducer reads:\n{}",
        no_pid.stderr
    );
}
