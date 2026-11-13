//! The fixture of the killed-run e2e cases: a one-ticket workspace whose worker
//! can be held inside its visit, and a `rhei run` the test kills outright with
//! `Child::kill` - `SIGKILL`, which no handler of rhei's can catch.
//!
//! Unix-only, like the module that uses it: the worker is a shell script and
//! "gone" is read with `kill -0 -- -<pgid>`. §REQ-cross-platform.4

// §FS-rhei-agents.8.1.1 §FS-rhei-run.3.2

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::*;

/// Generous on purpose: every wait polls, so a large bound only decides how
/// long a genuine failure takes to report.
pub const PATIENCE: Duration = Duration::from_secs(60);

/// Stands in for the agent and for both programs, and spends no tokens. Every
/// start appends `start <state>` to `worker.log` and records its pid - its
/// process group, since a supervised subprocess leads its own. An agent start
/// first copies the spawn record of its state, so a case can read what the run
/// wrote before it spawned this attempt. While `hold-<state>` exists the worker
/// stays inside its visit (`exec sleep`, so the parent-death signal reaches
/// it); with `straggler` present it first leaves a child in its group that
/// ignores `SIGTERM`. Otherwise an agent writes its result and exits 0.
const WORKER: &str = r#"#!/bin/sh
set -u
fixture='@FIXTURE@'
state="${RHEI_STATE:-?}"
prompt="$(cat)"
printf 'start %s\n' "$state" >> "$fixture/worker.log"
n="$(grep -c "^start $state\$" "$fixture/worker.log")"
mkdir -p "$fixture/pids" "$fixture/seen"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
if [ -n "$root" ]; then
  for record in "$root"/runtime/spawns/*"-$state"*.json; do
    [ -e "$record" ] && cp "$record" "$fixture/seen/$state-$n.json"
  done
fi
if [ -e "$fixture/hold-$state" ]; then
  if [ -e "$fixture/straggler" ]; then
    (trap '' TERM; exec sleep 300) &
    printf '%s\n' "$!" > "$fixture/pids/straggler"
  fi
  printf '%s\n' "$$" > "$fixture/pids/$state.tmp"
  mv "$fixture/pids/$state.tmp" "$fixture/pids/$state"
  exec sleep 300
fi
if [ -n "$root" ]; then
  result_path="$(printf '%s\n' "$prompt" | sed -n '/^## Result$/,/^## /s/^- `\([^`]*\)`$/\1/p')"
  mkdir -p "$(dirname "${result_path:?}")"
  printf '## Result\n\nCovered.\n' > "$result_path"
fi
exit 0
"#;

/// The issue's machine: a program, an agent, a second program, then done.
/// `cover` keeps the built-in `attempts: 2`.
const MACHINE: &str = r#"name: killed-launcher
version: 1
states:
  prepare:
    initial: true
    description: Prepare the ticket
    program:
      command: [sh, WORKER]
    program_timeout: 60s
  cover:
    description: Cover the change
    agent: mock
    agent_timeout: 120s
  finalize:
    description: Publish what was covered
    program:
      command: [sh, WORKER]
    program_timeout: 60s
  done:
    final: true
    description: Done
transitions:
  - from: prepare
    to: cover
  - from: cover
    to: finalize
  - from: finalize
    to: done
"#;

/// An agent poll state, which `attempts:` does not bound. §FS-rhei-agents.3.2.3
const POLL_MACHINE: &str = r#"name: killed-poll
version: 1
states:
  waiting:
    initial: true
    description: Wait on the reviewer
    agent: mock
    agent_timeout: 120s
    poll:
      interval: 1s
      max_attempts: 5
  done:
    final: true
    description: Done
transitions:
  - from: waiting
    to: waiting
    condition: pollAttempts < pollMaxAttempts
  - from: waiting
    to: done
    condition: pollAttempts >= pollMaxAttempts
"#;

/// One killed-run workspace: `dir` holds the worker's own files, `workspace`
/// the plan.
pub struct Fixture {
    pub dir: TestDir,
    pub workspace: PathBuf,
    pub machine: PathBuf,
}

/// A live `rhei run` that dies with the test, so a failed wait leaves nothing
/// running behind it.
pub struct LiveRun(pub Child);

impl Drop for LiveRun {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Fixture {
    /// The issue's workspace: one ticket that starts in `prepare`.
    pub fn new(prefix: &str) -> Self {
        Self::with_machine(prefix, MACHINE, "prepare")
    }

    /// One ticket parked in the agent poll state `waiting`.
    pub fn poll(prefix: &str) -> Self {
        Self::with_machine(prefix, POLL_MACHINE, "waiting")
    }

    fn with_machine(prefix: &str, machine: &str, initial: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let workspace = dir.join("workspace");
        fs::create_dir_all(workspace.join("tasks")).expect("create workspace dirs");
        fs::write(workspace.join("index.rhei.md"), "# Rhei: Killed launcher\n")
            .expect("write index");
        fs::write(
            workspace.join("tasks/01-ticket.md"),
            format!("### Task 1: One ticket\n**State:** {initial}\n"),
        )
        .expect("write task file");

        let fixture = dir.to_path_buf();
        let worker = write_fixture_file(
            &dir,
            "worker.sh",
            &WORKER.replace("@FIXTURE@", &fixture.display().to_string()),
        );
        let settings_dir = workspace.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings_dir).expect("create settings dir");
        let worker_json = serde_json::to_string(&worker.display().to_string()).expect("json");
        fs::write(
            settings_dir.join("settings.json"),
            format!(
                r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "120s" }},
  "agents": {{ "mock": {{ "command": ["sh", {worker_json}], "stdin_prompt": true, "intervene_stdin": false, "timeout": "120s" }} }}
}}"#
            ),
        )
        .expect("write settings");
        let machine = write_fixture_file(
            &dir,
            "states.yaml",
            &machine.replace("WORKER", &worker.display().to_string()),
        );
        Fixture { dir, workspace, machine }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Keep the worker of `state` inside its visit until [`Self::release`].
    pub fn hold(&self, state: &str) {
        fs::write(self.path(&format!("hold-{state}")), "").expect("write hold");
    }

    pub fn release(&self, state: &str) {
        let _ = fs::remove_file(self.path(&format!("hold-{state}")));
    }

    /// Start `rhei run` on the workspace, its output in `<name>.out`/`.err`.
    pub fn spawn_run(&self, name: &str) -> LiveRun {
        let mut cmd = rhei_command(self.dir.join(".home"));
        cmd.arg("--state-machine")
            .arg(&self.machine)
            .arg("run")
            .arg(&self.workspace)
            .arg("--no-tui")
            .arg("--no-callbacks")
            .arg("--no-dashboard")
            .stdin(Stdio::null())
            .stdout(fs::File::create(self.path(&format!("{name}.out"))).expect("stdout"))
            .stderr(fs::File::create(self.path(&format!("{name}.err"))).expect("stderr"));
        LiveRun(cmd.spawn().expect("rhei run should start"))
    }

    /// Everything a run printed, stdout then stderr.
    pub fn output(&self, name: &str) -> String {
        let stdout = fs::read_to_string(self.path(&format!("{name}.out"))).unwrap_or_default();
        let stderr = stderr_from_file(self.path(&format!("{name}.err")));
        format!("stdout:\n{stdout}\nstderr:\n{stderr}")
    }

    /// Run to the end and return its exit status and output.
    pub fn run_to_end(&self, name: &str) -> (ExitStatus, String) {
        let mut run = self.spawn_run(name);
        let status = self.wait_for_exit(&mut run, name);
        (status, self.output(name))
    }

    pub fn wait_for_exit(&self, run: &mut LiveRun, name: &str) -> ExitStatus {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = run.0.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "rhei run did not exit within {PATIENCE:?}\n{}",
                self.output(name)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// How many times the worker has started in `state`.
    pub fn starts(&self, state: &str) -> usize {
        let log = fs::read_to_string(self.path("worker.log")).unwrap_or_default();
        log.lines().filter(|line| *line == format!("start {state}")).count()
    }

    /// Wait for start number `n` of `state` to be holding, and return its
    /// process group.
    pub fn wait_for_hold(&self, state: &str, n: usize, live: &mut LiveRun, run: &str) -> i32 {
        let path = self.path(&format!("pids/{state}"));
        let deadline = Instant::now() + PATIENCE;
        loop {
            if self.starts(state) >= n {
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(pgid) = text.trim().parse() {
                        if self.starts(state) == n {
                            // The pid file is written after the start line,
                            // and the previous holder's group is gone.
                            if group_alive(pgid) {
                                return pgid;
                            }
                        }
                    }
                }
            }
            if let Some(status) = live.0.try_wait().expect("try_wait") {
                panic!(
                    "rhei run exited {status:?} before start {n} of '{state}' held\n{}",
                    self.output(run)
                );
            }
            assert!(
                Instant::now() < deadline,
                "start {n} of '{state}' never held\n{}",
                self.output(run)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Kill `rhei run` outright inside the visit held by group `pgid`, then
    /// wait until nothing of that group is left. On Linux the parent-death
    /// signal ends the held worker; where there is none the test stands in for
    /// the operator the still-running refusal addresses and stops the group.
    pub fn kill_run_and_settle(&self, mut run: LiveRun, pgid: i32, name: &str) {
        run.0.kill().expect("SIGKILL rhei run");
        let status = run.0.wait().expect("reap rhei run");
        assert_eq!(
            signal_of(&status),
            Some(9),
            "rhei run should have died of SIGKILL: {status:?}\n{}",
            self.output(name)
        );
        if !wait_group_gone(pgid, Duration::from_secs(10)) {
            kill_group(pgid);
            assert!(wait_group_gone(pgid, PATIENCE), "group {pgid} never emptied");
        }
    }

    /// Log file names under `runtime/logs/` for `state`, sorted.
    pub fn logs_of(&self, state: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.workspace.join("runtime/logs"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter(|name| name.contains(&format!("-{state}")))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    pub fn log_bytes(&self, name: &str) -> Vec<u8> {
        fs::read(self.workspace.join("runtime/logs").join(name)).expect("read log")
    }

    /// The spawn record of `state` as start `n` of the worker saw it, before
    /// it did anything. §FS-rhei-agents.8.4
    pub fn seen_record(&self, state: &str, n: usize) -> serde_json::Value {
        let path = self.path(&format!("seen/{state}-{n}.json"));
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("start {n} of '{state}' saw no spawn record: {err}"));
        serde_json::from_str(&text).expect("parse spawn record")
    }

    /// Every evidence group record a run wrote. §FS-rhei-evidence.3.4
    pub fn group_records(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let runs = self.workspace.join("runtime/evidence/runs");
        for run in fs::read_dir(runs).into_iter().flatten().flatten() {
            for entry in fs::read_dir(run.path()).into_iter().flatten().flatten() {
                if entry.file_name().to_string_lossy().ends_with(".group.json") {
                    found.push(entry.path());
                }
            }
        }
        found
    }

    pub fn assert_state(&self, expected: &str) {
        assert_task_state(&self.workspace, &self.machine, "1", expected);
    }
}

fn signal_of(status: &ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt as _;
    status.signal()
}

/// Whether any process is left in group `pgid`.
pub fn group_alive(pgid: i32) -> bool {
    Command::new("kill")
        .args(["-0", "--", &format!("-{pgid}")])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn wait_group_gone(pgid: i32, patience: Duration) -> bool {
    let deadline = Instant::now() + patience;
    while group_alive(pgid) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

pub fn kill_group(pgid: i32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(Stdio::null())
        .status();
}

/// `rhei summary <workspace> --details`, which must succeed.
pub fn summary_details(fixture: &Fixture) -> String {
    let output = rhei_command(fixture.dir.join(".home"))
        .arg("--state-machine")
        .arg(&fixture.machine)
        .arg("summary")
        .arg(&fixture.workspace)
        .arg("--details")
        .output()
        .expect("rhei summary should run");
    let run = CliRun::from(&output);
    assert!(run.status.success(), "rhei summary failed:\n{}\n{}", run.stdout, run.stderr);
    run.stdout
}
