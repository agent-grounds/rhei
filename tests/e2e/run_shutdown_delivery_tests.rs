//! An agent dead of its own copy of the signal that stopped its run.
//!
//! Unix-only, and the whole file with it: the case delivers one `SIGTERM` to
//! `rhei run` and to its agent's process group together, which is what systemd's
//! default `KillMode=control-group` does to a unit's cgroup when the unit stops.
//! Windows has neither process groups nor the signal. §REQ-cross-platform.4

// §FS-rhei-run.3.2 §FS-rhei-agents.3.2.3

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use super::*;

/// Generous on purpose: every wait polls, so a large bound only decides how
/// long a genuine failure takes to report.
#[cfg(unix)]
const PATIENCE: Duration = Duration::from_secs(60);

/// Attempt 1 records its pid - its process group, since a supervised subprocess
/// leads its own - and becomes a `sleep` with the default `SIGTERM`
/// disposition, so the signal ends it at once, as it ends any agent that does
/// not trap it. Any later attempt writes the result and exits 0.
#[cfg(unix)]
const AGENT: &str = r#"#!/bin/sh
set -eu
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
result_path="$(printf '%s\n' "$prompt" | sed -n '/^## Result$/,/^## /s/^- `\([^`]*\)`$/\1/p')"
mkdir -p "$root/runtime/pids"
if [ -e "$root/runtime/pids/attempted" ]; then
  mkdir -p "$(dirname "${result_path:?}")"
  printf '## Result\n\nThe later attempt implemented it.\n' > "$result_path"
  exit 0
fi
: > "$root/runtime/pids/attempted"
printf '%s\n' "$$" > "$root/runtime/pids/agent.tmp"
mv "$root/runtime/pids/agent.tmp" "$root/runtime/pids/agent"
exec sleep 300
"#;

/// The issue's fixture: one ticket in a state with `attempts: 2`.
#[cfg(unix)]
fn setup(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let workspace = dir.join("workspace");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Shutdown kill\n").expect("write index");
    fs::write(
        workspace.join("tasks/01-implement.md"),
        "### Task 1: Implement the change\n**State:** implement\n",
    )
    .expect("write task file");

    let agent = write_fixture_file(&dir, "mock-agent.sh", AGENT);
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let agent_json = serde_json::to_string(&agent.display().to_string()).expect("agent path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "120s" }},
  "agents": {{ "mock": {{ "command": ["sh", {agent_json}], "stdin_prompt": true, "timeout": "120s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: shutdown-kill
version: 1
states:
  implement:
    initial: true
    description: Implement the change
    agent: mock
    attempts: 2
    agent_timeout: 120s
  done:
    final: true
    description: The change is in
transitions:
  - from: implement
    to: done
"#,
    );
    (dir, workspace, machine)
}

/// A live `rhei run` that dies with the test, so a failed wait leaves nothing
/// running behind it.
#[cfg(unix)]
struct LiveRun(std::process::Child);

#[cfg(unix)]
impl Drop for LiveRun {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(unix)]
fn spawn_run(dir: &Path, workspace: &Path, machine: &Path, name: &str) -> LiveRun {
    let mut cmd = rhei_command(dir.join(".home"));
    cmd.arg("--state-machine")
        .arg(machine)
        .arg("run")
        .arg(workspace)
        .arg("--no-tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard")
        .stdin(std::process::Stdio::null())
        .stdout(fs::File::create(dir.join(format!("{name}.out"))).expect("create stdout"))
        .stderr(fs::File::create(dir.join(format!("{name}.err"))).expect("create stderr"));
    LiveRun(cmd.spawn().expect("rhei run should start"))
}

/// Everything a run printed, stdout then stderr.
#[cfg(unix)]
fn run_output(dir: &Path, name: &str) -> String {
    let stdout = fs::read_to_string(dir.join(format!("{name}.out"))).unwrap_or_default();
    let stderr = stderr_from_file(dir.join(format!("{name}.err")));
    format!("stdout:\n{stdout}\nstderr:\n{stderr}")
}

#[cfg(unix)]
fn wait_for_exit(run: &mut LiveRun, output: impl Fn() -> String) -> std::process::ExitStatus {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = run.0.try_wait().expect("try_wait") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "rhei run did not exit within {PATIENCE:?}\n{}",
            output()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn wait_for_agent_pid(workspace: &Path, output: impl Fn() -> String) -> String {
    let path = workspace.join("runtime/pids/agent");
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Ok(text) = fs::read_to_string(&path) {
            if !text.trim().is_empty() {
                return text.trim().to_string();
            }
        }
        assert!(Instant::now() < deadline, "the agent never recorded its pid\n{}", output());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The one spawn record a one-ticket run leaves.
#[cfg(unix)]
fn only_spawn_record(workspace: &Path) -> serde_json::Value {
    let records: Vec<PathBuf> = fs::read_dir(workspace.join("runtime/spawns"))
        .expect("spawn records")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    assert_eq!(records.len(), 1, "expected one spawn record, found {records:?}");
    serde_json::from_str(&fs::read_to_string(&records[0]).expect("read spawn record"))
        .expect("parse spawn record")
}

/// One `kill -TERM -- <run> -<agent pgid>` - the delivery systemd makes when it
/// stops a cgroup - interrupts the attempt exactly as `rhei stop` does: the run
/// is shutting down when its agent dies, so the attempt is `interrupted`,
/// charged nothing, and the next run is attempt 1 of 2 again.
// §FS-rhei-run.3.2 §FS-rhei-agents.3.2.3
#[cfg(unix)]
#[test]
fn an_agent_killed_by_the_signal_that_stops_its_run_is_interrupted_and_not_charged() {
    let (dir, workspace, machine) = setup("run-cgroup-sigterm");
    let mut first = spawn_run(&dir, &workspace, &machine, "first");
    let agent_pgid = wait_for_agent_pid(&workspace, || run_output(&dir, "first"));

    let status = Command::new("kill")
        .args(["-TERM", "--", &first.0.id().to_string(), &format!("-{agent_pgid}")])
        .status()
        .expect("kill should run");
    assert!(status.success(), "kill -TERM to the run and the agent's group failed");
    let exit = wait_for_exit(&mut first, || run_output(&dir, "first"));
    let output = run_output(&dir, "first");
    assert_eq!(exit.code(), Some(143), "the run should exit 128+SIGTERM\n{output}");

    let record = only_spawn_record(&workspace);
    assert_eq!(
        record["ending"], "interrupted",
        "an agent the shutdown's own signal killed was interrupted, not exited\n\
         record: {record}\n{output}"
    );
    assert_eq!(
        record["attempt_charged"], false,
        "the shutdown spends no attempt\nrecord: {record}"
    );
    assert!(
        output.contains("Interrupted — terminating"),
        "the operator should be told the run is interrupting its work\n{output}"
    );
    assert!(
        !output.contains("--continue-on-error"),
        "an interrupted run reports the interruption, not the halt diagnostic\n{output}"
    );
    assert_task_state(&workspace, &machine, "1", "implement");

    let mut second = spawn_run(&dir, &workspace, &machine, "second");
    let exit = wait_for_exit(&mut second, || run_output(&dir, "second"));
    let output = run_output(&dir, "second");
    assert!(exit.success(), "the second run should finish the ticket\n{output}");
    assert!(
        output.contains("attempt 1 of 2; the previous attempt was interrupted by a run shutdown"),
        "the next run must not count the interrupted attempt against the budget\n{output}"
    );
    assert_task_state(&workspace, &machine, "1", "done");
}
