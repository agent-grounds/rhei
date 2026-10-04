//! The harness the worker-edit scenarios share: a directory workspace whose
//! first task's program writes a note into a task body, an unrelated sibling,
//! and readers for what the run left behind.
//!
//! A worker that breaks the plan with its own task-body edit costs its own
//! attempt and nothing else: the run restores that task's region, keeps the
//! worker's text beside the attempt's log, and goes on. §FS-rhei-run.3.7

use std::fs;
use std::path::{Path, PathBuf};

use super::python_fixture::fixture_command_with_args;
use super::*;

/// The note of agent-grounds/rhei#310, under the heading that broke the run.
/// Appended to a four-line task file it puts the heading on line 6.
pub const NOTE: &str =
    "\n#### Visit 1 (cover)\n\nLatest measurement was report-1.json (64.38%, 47/73 lines).\n";

/// Task 1, the worker that writes the note. Four lines, as in the ticket.
pub const COVER: &str =
    "### Task 1: Raise line coverage\n**State:** cover\n\nRaise coverage of `src/report.rs` above 80%.\n";

/// Task 2, unrelated work that must finish whatever Task 1 writes.
pub const OTHER: &str = "### Task 2: Unrelated work\n**State:** other\n\nDoes something else.\n";

/// Task 3, not ready while Task 2 runs, so its region is never in flight.
pub const LATER: &str =
    "### Task 3: Later work\n**State:** other\n**Prior:** Task 2\n\nRuns after Task 2.\n";

/// The three ways the run is asked to treat one ticket; none may change the
/// outcome of a reverted edit. §FS-rhei-run.3.7.4
#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Sequential,
    Parallel,
    ContinueOnError,
}

pub const MODES: [Mode; 3] = [Mode::Sequential, Mode::Parallel, Mode::ContinueOnError];

impl Mode {
    pub fn flags(self) -> &'static [&'static str] {
        match self {
            Mode::Sequential => &[],
            Mode::Parallel => &["--parallel", "2"],
            Mode::ContinueOnError => &["--continue-on-error"],
        }
    }

    pub fn concurrent(self) -> bool {
        matches!(self, Mode::Parallel)
    }
}

/// What Task 1's program writes, and where.
#[derive(Clone, Copy, Debug)]
pub enum Edit {
    /// The note into its own body, on the first attempt only.
    Once,
    /// The note into its own body, on every attempt.
    Always,
    /// A valid child task, then the note, on the first attempt only.
    ChildAndNote,
    /// The note into Task 3's body, on the first attempt only.
    OtherTask,
}

impl Edit {
    fn arg(self) -> &'static str {
        match self {
            Edit::Once => "once",
            Edit::Always => "always",
            Edit::ChildAndNote => "child",
            Edit::OtherTask => "other-task",
        }
    }
}

/// Task 1's program. `linger` keeps it alive after the edit until the sibling
/// has exited, so the sibling's reload meets the break while Task 1 still runs.
const COVER_PROGRAM: &str = r#"edit, linger = sys.argv[1], sys.argv[2] == 'linger'
here = pathlib.Path(__file__).parent
ws = here / 'ws'
own = ws / 'tasks' / '01-cover.md'
note = '\n#### Visit 1 (cover)\n\nLatest measurement was report-1.json (64.38%, 47/73 lines).\n'
child = '\n#### Task 1.1: Child the worker added\n**State:** other\n\nA legitimate child.\n'
marker = here / 'noted'
if edit == 'always' or not marker.exists():
    marker.write_text('x')
    if edit == 'other-task':
        append(ws / 'tasks' / '03-later.md', note)
    elif edit == 'child':
        append(own, child + note)
    else:
        append(own, note)
if linger:
    deadline = time.time() + 15
    while not (here / 'sibling-exited').exists() and time.time() < deadline:
        time.sleep(0.05)
    time.sleep(1)
append(here / 'cover-runs.txt', 'ran\n')
result('Task 1 wrote its note.\n')
"#;

/// Task 2's program. Under `--parallel` it waits for Task 1's edit to land and
/// then stays in flight a moment longer, so Task 1's exit finds it running.
const OTHER_PROGRAM: &str = r#"concurrent = sys.argv[1] == 'concurrent'
here = pathlib.Path(__file__).parent
if concurrent:
    deadline = time.time() + 15
    while not (here / 'noted').exists() and time.time() < deadline:
        time.sleep(0.05)
    if sys.argv[2] == 'stay':
        time.sleep(2)
append(here / 'other-runs.txt', 'ran\n')
write(here / 'sibling-exited', 'x')
result('Task 2 did its unrelated work.\n')
"#;

fn machine(cover: &str, other: &str) -> String {
    format!(
        r#"name: worker-edit
version: 1
states:
  cover:
    description: Writes a note into a task body.
    program:
      command: {cover}
    program_timeout: 30s
  other:
    description: Unrelated work.
    program:
      command: {other}
    program_timeout: 30s
  completed:
    description: Done.
    final: true
  failed:
    description: Failed.
    final: true
transitions:
  - from: cover
    to: completed
    exit_code: 0
  - from: other
    to: completed
    exit_code: 0
  - from: "*"
    to: failed
"#
    )
}

/// One scenario's workspace `ws` (so its tasks are `ws.1`, `ws.2`, …), its
/// machine, and the programs, in a directory of their own.
pub struct Scenario {
    pub dir: TestDir,
    pub ws: PathBuf,
    pub machine: PathBuf,
    pub mode: Mode,
}

pub struct Ran {
    pub run: CliRun,
    pub mode: Mode,
}

impl Ran {
    pub fn output(&self) -> String {
        format!("mode {:?}\nstdout:\n{}\nstderr:\n{}", self.mode, self.run.stdout, self.run.stderr)
    }

    /// Both streams, with Windows path separators read as `/`.
    pub fn combined(&self) -> String {
        portable(&format!("{}{}", self.run.stdout, self.run.stderr))
    }
}

impl Scenario {
    /// `with_later` adds Task 3; `linger` keeps Task 1 alive past Task 2.
    pub fn new(name: &str, mode: Mode, edit: Edit, with_later: bool, linger: bool) -> Self {
        let dir = unique_temp_dir(&format!("worker-edit-{name}-{mode:?}"));
        let ws = dir.join("ws");
        fs::create_dir_all(ws.join("tasks")).expect("create workspace");
        fs::write(ws.join("index.rhei.md"), "# Rhei: Worker edit\n").expect("write index");
        fs::write(ws.join("tasks/01-cover.md"), COVER).expect("write task 1");
        fs::write(ws.join("tasks/02-other.md"), OTHER).expect("write task 2");
        if with_later {
            fs::write(ws.join("tasks/03-later.md"), LATER).expect("write task 3");
        }
        let cover = write_python_agent(&dir, "cover.py", COVER_PROGRAM);
        let other = write_python_agent(&dir, "other.py", OTHER_PROGRAM);
        let linger = if linger && mode.concurrent() { "linger" } else { "go" };
        let concurrent = if mode.concurrent() { "concurrent" } else { "alone" };
        // A lingering Task 1 is the one in flight, so Task 2 must not wait on it.
        let stay = if linger == "linger" { "go" } else { "stay" };
        let text = machine(
            &fixture_command_with_args(&cover, &[edit.arg(), linger]),
            &fixture_command_with_args(&other, &[concurrent, stay]),
        );
        let machine = write_fixture_file(&ws, "states.yaml", &text);
        Scenario { dir, ws, machine, mode }
    }

    pub fn run(&self) -> Ran {
        let mut args = vec!["--no-tui", "--no-callbacks"];
        args.extend_from_slice(self.mode.flags());
        Ran { run: run_cli("run", &self.ws, &self.machine, &args), mode: self.mode }
    }

    pub fn task_file(&self, name: &str) -> String {
        fs::read_to_string(self.ws.join("tasks").join(name)).expect("read task file")
    }

    pub fn state_of(&self, name: &str) -> String {
        state_in(&self.task_file(name))
    }

    /// How many times a program ran, from the line it appends per run.
    pub fn runs_of(&self, program: &str) -> usize {
        fs::read_to_string(self.dir.join(format!("{program}-runs.txt")))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    pub fn journal(&self) -> String {
        journal_in(&self.ws)
    }

    pub fn runtime_file(&self, relative: &str) -> PathBuf {
        self.ws.join(relative)
    }
}

/// `text` with Windows path separators read as `/`: the run spells a path the
/// way the platform does, and the scenarios name them one way.
pub fn portable(text: &str) -> String {
    text.replace('\\', "/")
}

/// The run's journal in `ws`, separators read as `/`. §FS-rhei-run-tui.1.7
pub fn journal_in(ws: &Path) -> String {
    portable(&fs::read_to_string(ws.join("runtime/transitions.log")).unwrap_or_default())
}

/// The first `**State:**` in a task file's text.
pub fn state_in(text: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("**State:** "))
        .map(|state| state.trim().to_string())
        .unwrap_or_else(|| panic!("no state line in:\n{text}"))
}

/// The `end@<state>` lines the journal holds for one task, in order.
/// §FS-rhei-run-tui.1.7
pub fn end_records(journal: &str, task: &str, state: &str) -> Vec<String> {
    let event = format!("end@{state}");
    journal
        .lines()
        .filter(|line| {
            let mut columns = line.split_whitespace();
            columns.nth(1) == Some(task) && columns.next() == Some(event.as_str())
        })
        .map(str::to_string)
        .collect()
}

/// The trailing `key=value` metadata of one journal line.
pub fn metadata(line: &str) -> Vec<(String, String)> {
    line.split_whitespace()
        .nth(4)
        .unwrap_or_default()
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

pub fn meta_value(line: &str, key: &str) -> Option<String> {
    metadata(line).into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The durable JSON event log's `slot_released` records. §FS-rhei-run-json.2.1
pub fn slot_released(ws: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(ws.join("runtime/events.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == "slot_released")
        .map(|mut record| {
            if let Some(reverted) = record["reverted"].as_str().map(portable) {
                record["reverted"] = serde_json::Value::String(reverted);
            }
            record
        })
        .collect()
}
