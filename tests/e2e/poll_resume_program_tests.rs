//! A resumed program poll must wait for its persisted deadline and then make
//! another subprocess attempt. The stop/restart boundary uses Unix signals,
//! so this exact detached-run scenario is Unix-only; the program fixture and
//! execution-mode rule remain platform-neutral.

// §FS-rhei-run.3 §FS-rhei-run.5.1 §FS-rhei-states.2.4 §FS-rhei-programs.3.2
// §REQ-cross-platform.4

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{fixture_command, rhei_command, stderr, stdout, unique_temp_dir, write_python_agent};

const POLL_INTERVAL_SECS: u64 = 5;
const POLL_MAX_ATTEMPTS: u64 = 3;
const TEST_PATIENCE: Duration = Duration::from_secs(20);

const PLAN: &str = r#"# Rhei: triage-tool-reports

## Tasks

### Task 1: Poll for an external ruling
**State:** triage
"#;

fn machine(program: &Path) -> String {
    format!(
        r#"name: poll-resume-regression
version: 1
states:
  triage:
    initial: true
    description: Poll until an external ruling is available
    program:
      command: {}
    poll:
      interval: {POLL_INTERVAL_SECS}s
      max_attempts: {POLL_MAX_ATTEMPTS}
  needs-human:
    description: Polling was exhausted
    final: true
  resolved:
    description: The ruling was available
    final: true
  cancelled:
    description: Cancelled
    final: true
transitions:
  - from: triage
    to: needs-human
    condition: pollAttempts >= pollMaxAttempts
  - from: triage
    to: resolved
    exit_code: 0
  - from: triage
    to: triage
    exit_code: 75
  - from: triage
    to: needs-human
    exit_code: nonzero
  - from: '*'
    to: cancelled
"#,
        fixture_command(program)
    )
}

struct PollWorkspace {
    _root: super::TestDir,
    project: PathBuf,
    plan: PathBuf,
    machine: PathBuf,
    home: PathBuf,
}

impl PollWorkspace {
    fn new() -> Self {
        let root = unique_temp_dir("resumed-program-poll");
        let project = root.join("agent-grounds");
        fs::create_dir_all(&project).expect("create Panta project");
        fs::write(project.join("index.panta.md"), "# Panta: Poll resume regression\n")
            .expect("write Panta index");
        let plan = project.join("triage-tool-reports.rhei.md");
        fs::write(&plan, PLAN).expect("write member plan");
        let program = write_python_agent(
            &root,
            "poll-program.py",
            r#"append('runtime/program-attempts.txt', 'attempt=' + env('RHEI_ATTEMPT', 'unknown') + '\n')
sys.exit(75)
"#,
        );
        let machine_path = root.join("states.yaml");
        fs::write(&machine_path, machine(&program)).expect("write state machine");
        let home = root.join("home");
        fs::create_dir_all(&home).expect("create isolated home");
        Self { _root: root, project, plan, machine: machine_path, home }
    }

    fn rhei(&self, args: &[&str]) -> Output {
        let mut command = rhei_command(&self.home);
        command.current_dir(&self.project).args(args);
        command.output().expect("rhei command should run")
    }

    fn launch(&self) -> String {
        let project = self.project.to_string_lossy();
        let machine = self.machine.to_string_lossy();
        let out = self.rhei(&[
            "--state-machine",
            &machine,
            "run",
            &project,
            "--rhei",
            "triage-tool-reports",
            "--headless",
            "--no-tui",
            "--no-dashboard",
        ]);
        assert!(
            out.status.success(),
            "headless run should launch\nstdout:\n{}\nstderr:\n{}",
            stdout(&out),
            stderr(&out)
        );
        stdout(&out)
            .lines()
            .find_map(|line| line.strip_prefix("Run ").and_then(|tail| tail.split(' ').next()))
            .unwrap_or_else(|| panic!("launcher printed no run id:\n{}", stdout(&out)))
            .to_string()
    }

    fn stop(&self, id: &str) {
        let out = self.rhei(&["stop", id, "--wait"]);
        assert!(
            out.status.success(),
            "run {id} should stop\nstdout:\n{}\nstderr:\n{}",
            stdout(&out),
            stderr(&out)
        );
    }

    fn stop_quietly(&self) {
        let _ = self.rhei(&["stop", "--wait"]);
    }

    fn snapshot(&self) -> PollSnapshot {
        let plan_text = fs::read_to_string(&self.plan).expect("read member plan");
        let parsed = rhei_core::parse(&plan_text).expect("parse member plan");
        PollSnapshot {
            attempts: fs::read_to_string(self.project.join("runtime/program-attempts.txt"))
                .unwrap_or_default()
                .lines()
                .count(),
            state: parsed.tasks.first().map(|task| task.state.clone()).unwrap_or_default(),
            deadline: triage_metadata_value(&plan_text, "pollNextAttemptAt"),
            visits: triage_metadata_value(&plan_text, "stateVisits"),
            run_log: fs::read_to_string(self.project.join("runtime/run.log")).unwrap_or_default(),
            ledger: fs::read_to_string(self.project.join("runtime/state-transitions.log"))
                .unwrap_or_default(),
        }
    }
}

impl Drop for PollWorkspace {
    fn drop(&mut self) {
        self.stop_quietly();
    }
}

#[derive(Debug)]
struct PollSnapshot {
    attempts: usize,
    state: String,
    deadline: Option<u64>,
    visits: Option<u64>,
    run_log: String,
    ledger: String,
}

fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("current Unix time").as_secs()
}

fn triage_metadata_value(plan: &str, field: &str) -> Option<u64> {
    let marker = format!("{field}:\n");
    plan.split_once(&marker)?.1.lines().next()?.trim().strip_prefix("triage: ")?.parse().ok()
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TEST_PATIENCE;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out after {TEST_PATIENCE:?} waiting for {what}");
}

/// Restarting during a persisted backoff must preserve the deadline, then run
/// the next real program attempt and route from its exit instead of inventing
/// an exit-zero completion.
/// §FS-rhei-run.3
#[test]
fn a_resumed_program_poll_waits_then_spawns_its_next_attempt() {
    let workspace = PollWorkspace::new();
    let first_id = workspace.launch();
    wait_for("attempt 1 and its persisted future deadline", || {
        let observed = workspace.snapshot();
        observed.attempts == 1
            && observed.visits == Some(2)
            && observed.deadline.is_some_and(|deadline| deadline > unix_secs())
    });
    let first = workspace.snapshot();
    let first_deadline = first.deadline.expect("attempt 1 should persist its retry deadline");
    assert_eq!(first.state, "triage", "attempt 1 should take the exit-75 self-loop: {first:#?}");
    assert_eq!(
        first.visits,
        Some(2),
        "attempt 1 should schedule attempt 2 while the three-attempt budget remains"
    );

    workspace.stop(&first_id);
    assert!(
        unix_secs() < first_deadline,
        "the first run must stop before its persisted deadline: {first:#?}"
    );

    let resumed_id = workspace.launch();
    assert!(
        unix_secs() < first_deadline,
        "the resumed run must start before the persisted deadline: {first:#?}"
    );
    while unix_secs() < first_deadline {
        let before_deadline = workspace.snapshot();
        assert_eq!(
            before_deadline.attempts, 1,
            "the persisted deadline must still prevent an early spawn: {before_deadline:#?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }

    wait_for("the resumed run to respawn or take the reported false route", || {
        let observed = workspace.snapshot();
        observed.state == "resolved"
            || (observed.attempts >= 2
                && observed.visits == Some(3)
                && observed.deadline.is_some_and(|deadline| deadline > first_deadline))
    });
    let resumed = workspace.snapshot();
    workspace.stop_quietly();

    assert!(
        resumed.attempts == 2
            && resumed.state == "triage"
            && resumed.visits == Some(3)
            && resumed.deadline.is_some_and(|deadline| deadline > first_deadline)
            && !resumed.ledger.contains("triage@resolved"),
        "after the deadline, attempt 2 must really exit 75 and leave the task polling\n\
         first run id: {first_id}\nresumed run id: {resumed_id}\n\
         poll max attempts: {POLL_MAX_ATTEMPTS}\nfirst snapshot: {first:#?}\n\
         resumed snapshot: {resumed:#?}\nrelevant resumed-run log:\n{}",
        resumed.run_log
    );
}
