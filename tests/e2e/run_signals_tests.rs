// §AR-source-file-size.3

// Signal, timeout, and lost-output shutdown cases for `rhei run`, with the
// process-supervision harness they alone use.
//
// Unix-only, and the whole file with it: every case delivers a POSIX signal
// (`SIGTERM`, `SIGINT`, `SIGKILL`) to a process group, or hangs up a pty, and
// asserts on what the group does about it. Windows has neither, and the harness
// that spawns, signals, and reaps those groups is shared by all of them — so
// the imports are gated the same way.
//
// That gate is what lets the fixtures below stay `#!/bin/sh`: a signal trap is
// the thing under test, and it is written in the only shell that has one.

// §REQ-cross-platform.4

#[cfg(unix)]
use std::fs;

#[cfg(unix)]
use super::native_pty_support::open_native_terminal;
#[cfg(unix)]
use super::*;

// ---------------------------------------------------------------------------
// Supervised process groups: interruption, teardown, and the timeout that now
// takes the whole group with it.
// ---------------------------------------------------------------------------

// §FS-rhei-run.3.2: one termination path for every subprocess a run starts.

/// A fake agent that backgrounds a grandchild and then sleeps — the shape
/// issue #53 was reported with, where killing the direct child left the
/// grandchild running.
#[cfg(unix)]
const GRANDCHILD_AGENT: &str = r#"#!/bin/sh
set -eu
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
mkdir -p "$root/runtime/pids"
sleep 300 &
printf '%s\n' "$!" > "$root/runtime/pids/grandchild"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
sleep 300
"#;

/// A fake agent that is one process and dies only to a signal it cannot catch.
#[cfg(unix)]
const LONE_AGENT: &str = r#"#!/bin/sh
set -eu
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
mkdir -p "$root/runtime/pids"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
exec sleep 300
"#;

/// A fake agent that does its ticket's work and exits, so the run reaches its
/// own end and the TUI parks on the finished screen.
#[cfg(unix)]
const QUICK_AGENT: &str = r#"#!/bin/sh
set -eu
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
result_path="$(printf '%s\n' "$prompt" | sed -n '/^## Result$/,/^## /s/^- `\([^`]*\)`$/\1/p')"
mkdir -p "$root/runtime/pids" "$(dirname "${result_path:?}")"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
printf '## Result\n\nMock agent finished.\n' > "$result_path"
exit 0
"#;

/// A fake agent that ignores `SIGTERM`, so only the `SIGKILL` at the end of the
/// grace — or a second interrupt that skips it — can end it.
#[cfg(unix)]
const STUBBORN_AGENT: &str = r#"#!/bin/sh
set -eu
trap '' TERM
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
mkdir -p "$root/runtime/pids"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
exec sleep 300
"#;

/// A one-ticket workspace whose only state runs `agent_body`.
#[cfg(unix)]
fn setup_supervised_workspace(
    prefix: &str,
    agent_body: &str,
    agent_timeout: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Supervised\n").expect("write index");
    fs::write(tasks_dir.join("01-work.md"), "### Task 1: Work\n**State:** work\n")
        .expect("write task file");

    let agent_script = write_fixture_file(&dir, "mock-agent.sh", agent_body);
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "{agent_timeout}" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "stdin_prompt": true, "timeout": "{agent_timeout}" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: supervised
version: 1
states:
  work:
    initial: true
    description: Do it
    agent: mock
    agent_timeout: {agent_timeout}
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
        ),
    );
    (dir, workspace, machine_path)
}

/// A live `rhei run` that dies with the test.
///
/// Every wait in these tests polls to a deadline and panics when it passes,
/// and a panic before the signal under test would leave `rhei`, its agent, and
/// the agent's `sleep 300` running for five minutes. `SIGKILL` to `rhei` is
/// enough on Linux — its subprocesses follow through the parent-death backstop
/// — but not on macOS, where a failed test may still leave an agent behind.
#[cfg(unix)]
struct KillOnDrop(std::process::Child);

#[cfg(unix)]
impl std::ops::Deref for KillOnDrop {
    type Target = std::process::Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(unix)]
impl std::ops::DerefMut for KillOnDrop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(unix)]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// Join a helper thread if it has already finished, and detach it otherwise:
/// a drain thread whose pty never closes must not decide how long a test runs.
#[cfg(unix)]
fn join_or_detach(handle: std::thread::JoinHandle<()>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !handle.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if handle.is_finished() {
        let _ = handle.join();
    }
}

/// Start `rhei run` as a live child so the test can signal it, with its output
/// on disk for the assertions and for the failure message.
#[cfg(unix)]
fn spawn_rhei_run(dir: &Path, workspace: &Path, machine: &Path) -> KillOnDrop {
    spawn_rhei_run_with(dir, workspace, machine, &[])
}

/// [`spawn_rhei_run`] with extra `run` flags.
#[cfg(unix)]
fn spawn_rhei_run_with(
    dir: &Path,
    target: &Path,
    machine: &Path,
    extra_args: &[&str],
) -> KillOnDrop {
    let mut cmd = rhei_command(dir.join(".home"));
    cmd.arg("--state-machine")
        .arg(machine)
        .arg("run")
        .arg(target)
        .arg("--no-tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard");
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(fs::File::create(dir.join("run.out")).expect("create run stdout"));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    KillOnDrop(cmd.spawn().expect("rhei run should start"))
}

#[cfg(unix)]
fn signal_pid(pid: u32, signal: &str) {
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .status()
        .expect("kill should run");
    assert!(status.success(), "kill -{signal} {pid} failed");
}

/// Whether a pid still exists, by the `kill -0` rule.
#[cfg(unix)]
fn pid_is_alive(pid: &str) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Poll until `check` holds, so a slow machine costs patience rather than a
/// failure. Panics with `what` when the deadline passes.
#[cfg(unix)]
fn poll_until(what: &str, timeout: std::time::Duration, mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

/// Wait for `rhei run` to exit, rather than blocking forever if it does not.
#[cfg(unix)]
fn wait_for_exit(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => return status,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("rhei run did not exit within {timeout:?}");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}

/// The pid the fake agent recorded for itself, once it has recorded one.
#[cfg(unix)]
fn wait_for_recorded_pid(workspace: &Path, name: &str) -> String {
    let path = workspace.join("runtime/pids").join(name);
    poll_until(&format!("the fake agent to record its {name} pid"), TEST_PATIENCE, || {
        fs::read_to_string(&path).map(|text| !text.trim().is_empty()).unwrap_or(false)
    });
    fs::read_to_string(&path).expect("read recorded pid").trim().to_string()
}

/// The single agent transcript a one-ticket run produces.
#[cfg(unix)]
fn read_only_agent_log(workspace: &Path) -> String {
    let logs_dir = workspace.join("runtime/logs");
    let mut logs: Vec<PathBuf> = fs::read_dir(&logs_dir)
        .expect("agent log directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 1, "expected exactly one agent log, found {logs:?}");
    fs::read_to_string(&logs[0]).expect("read agent log")
}

/// The stderr `rhei run` was redirected into, through the harness's one seam:
/// a diagnostic lands in this file wrapped exactly as it lands in a pipe.
#[cfg(unix)]
fn read_run_stderr(dir: &Path) -> String {
    super::stderr_from_file(dir.join("run.err"))
}

#[cfg(unix)]
fn read_run_stdout(dir: &Path) -> String {
    fs::read_to_string(dir.join("run.out")).unwrap_or_default()
}

/// Generous on purpose: every wait in these tests polls, so the only cost of a
/// large bound is how long a genuine failure takes to report.
#[cfg(unix)]
const TEST_PATIENCE: std::time::Duration = std::time::Duration::from_secs(30);

/// `SIGTERM` to `rhei run` must take the agent **and its grandchild** with it,
/// leave the ticket exactly where it was, and exit `128 + SIGTERM`.
///
/// This is issue #53: the supervisor died and its agent kept running,
/// reparented to init, still writing into the workspace with nobody left to
/// enforce its timeout or record its transition.
// §FS-rhei-run.3.2 §FS-rhei-agents.8
#[cfg(unix)]
#[test]
fn sigterm_to_the_run_ends_the_agent_and_its_grandchild() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-sigterm-group", GRANDCHILD_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");
    assert!(pid_is_alive(&agent), "the agent should be running");
    assert!(pid_is_alive(&grandchild), "the grandchild should be running");

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);

    // 128 + SIGTERM, the status a shell reports for a process SIGTERM killed.
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("the grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // The interruption is not a verdict on the ticket: it keeps its state and
    // the next run re-executes it.
    assert_task_state(&workspace, &machine_path, "1", "work");

    let log = read_only_agent_log(&workspace);
    assert!(
        log.contains("agent interrupted by run shutdown after"),
        "log should name the interruption, got:\n{log}"
    );
    assert!(log.contains("interrupted: true"), "log footer should flag it, got:\n{log}");
    assert!(!log.contains("timed_out: true"), "an interruption is not a timeout, got:\n{log}");

    let stderr = read_run_stderr(&dir);
    assert!(
        stderr.contains("Interrupted — terminating 1 invocation(s)"),
        "the shutdown notice should reach the operator, got:\n{stderr}"
    );

    // The run stopped; it did not complete, and it did not stop for human
    // attention. Both surfaces have to say the same thing.
    // §FS-rhei-run-report.3.1
    let stdout = read_run_stdout(&dir);
    assert!(
        !stdout.contains("Run complete:"),
        "an interrupted run must not claim completion, got:\n{stdout}"
    );
    let report = fs::read_to_string(workspace.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("Result: interrupted — re-run to continue"),
        "the report should name the interruption as the result, got:\n{report}"
    );
    assert!(
        report.contains("run interrupted while its worker was in state work"),
        "the Attention row should name the interruption as the blocker, got:\n{report}"
    );
    assert!(
        !report.contains("mark the task cancelled"),
        "an interrupted ticket is not something to cancel, got:\n{report}"
    );
}

/// `SIGINT` — what a foreground Ctrl+C and the TUI's re-raise both deliver —
/// takes the same path and exits `130`.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
#[cfg(unix)]
#[test]
fn sigint_to_the_run_interrupts_it_and_exits_130() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-sigint-group", LONE_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let agent = wait_for_recorded_pid(&workspace, "agent");
    signal_pid(run.id(), "INT");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);

    assert_eq!(
        status.code(),
        Some(130),
        "run should exit 128+SIGINT\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    assert_task_state(&workspace, &machine_path, "1", "work");
    assert!(read_only_agent_log(&workspace).contains("interrupted: true"));
}

/// A supervisor `SIGKILL`ed runs no code at all, so nothing it installed can
/// tear its agents down. On Linux the agent's own parent-death signal does it.
///
/// `LONE_AGENT`, not `GRANDCHILD_AGENT`, on purpose: `PR_SET_PDEATHSIG` reaches
/// the direct subprocess and nothing below it. A grandchild of a `SIGKILL`ed
/// supervisor survives unless the agent tears it down as it dies, because
/// group-wide teardown needs the supervisor alive to signal the group.
/// Asserting a dead grandchild here would assert something the backstop does
/// not promise.
// §FS-rhei-run.3.2 §DA-supervised-process-groups
#[cfg(target_os = "linux")]
#[test]
fn sigkill_to_the_run_still_ends_the_agent() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-sigkill-pdeathsig", LONE_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let agent = wait_for_recorded_pid(&workspace, "agent");
    assert!(pid_is_alive(&agent), "the agent should be running");

    signal_pid(run.id(), "KILL");
    wait_for_exit(&mut run, TEST_PATIENCE);
    poll_until("the agent to die with its supervisor", TEST_PATIENCE, || !pid_is_alive(&agent));
}

/// A second interrupt means "now": the group is `SIGKILL`ed without waiting out
/// the grace.
///
/// The assertion is timing, and deliberately coarse. The agent ignores
/// `SIGTERM`, and this is a release-shaped binary, so its grace is the full
/// 10 s — a run that gets all the way out in a couple of seconds can only have
/// skipped it. The two signals are sent a beat apart because a second identical
/// signal delivered while the first is still pending would be coalesced into
/// one, and then there would be nothing to skip the grace.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_second_interrupt_skips_the_termination_grace() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-double-interrupt", STUBBORN_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let agent = wait_for_recorded_pid(&workspace, "agent");
    signal_pid(run.id(), "INT");
    // Long enough for the first signal to be delivered and handled, short
    // enough to be nowhere near the 10 s grace it is about to cut short.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let second = std::time::Instant::now();
    signal_pid(run.id(), "INT");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    let after_second = second.elapsed();
    assert_eq!(status.code(), Some(130), "stderr:\n{}", read_run_stderr(&dir));
    assert!(
        after_second < std::time::Duration::from_secs(6),
        "the second interrupt should skip the 10 s grace; the run took {after_second:?}\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));

    let stderr = read_run_stderr(&dir);
    assert!(
        stderr.contains("press Ctrl+C again to kill immediately"),
        "the notice should say a second signal is available, got:\n{stderr}"
    );
}

/// A timeout signals the agent's **group**, so the MCP servers and shell tools
/// it started go with it. Before this, the timeout killed the direct child pid
/// and left the rest running.
// §FS-rhei-agents.7.3 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_timeout_ends_the_agents_whole_group() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-timeout-group", GRANDCHILD_AGENT, "2s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert!(
        !status.success(),
        "a ticket whose agent timed out with no timeout transition cannot finish"
    );

    poll_until("the timed-out agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("its grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    let log = read_only_agent_log(&workspace);
    assert!(log.contains("agent timed out after"), "got:\n{log}");
    assert!(log.contains("timed_out: true"), "got:\n{log}");
    assert!(!log.contains("interrupted: true"), "a timeout is not an interruption, got:\n{log}");
}

/// An interrupted run must not start the work it had merely queued up.
///
/// Four tickets share one `concurrent: true` program state, so a single pass
/// collects all four and runs them one after another. A `SIGTERM` while the
/// first is in flight has to end the pass, not merely shorten each of the
/// remaining three to the moment its own `wait` reads the token — which is what
/// happened before the loop learned to check.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_interrupted_run_starts_none_of_the_programs_it_had_queued() {
    let plan = r#"# Rhei: Queued Programs

## Tasks

### Task 1: One
**State:** work

### Task 2: Two
**State:** work

### Task 3: Three
**State:** work

### Task 4: Four
**State:** work
"#;
    // `concurrent: true` is what lets one pass pick up all four tickets;
    // without it the state admits one at a time and there is nothing queued.
    let machine = r#"name: queued-programs
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: Sleep until told otherwise
    program: >-
      mkdir -p runtime/started
      && : > "runtime/started/$RHEI_TASK_ID"
      && sleep 300
  completed:
    description: Done
    final: true
transitions:
  - from: work
    to: completed
    exit_code: 0
"#;

    let dir = unique_temp_dir("run-interrupt-queued-programs");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // `--parallel 1` runs programs one at a time from the pass's own loop,
    // which is the path this test is about.
    let mut run = spawn_rhei_run_with(&dir, &plan_path, &machine_path, &["--parallel", "1"]);

    let started_dir = dir.join("runtime/started");
    poll_until("the first program to start", TEST_PATIENCE, || {
        fs::read_dir(&started_dir).map(|entries| entries.count() >= 1).unwrap_or(false)
    });

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    let started: Vec<String> = fs::read_dir(&started_dir)
        .expect("started marker directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(started.len(), 1, "only the in-flight program may have run, got {started:?}");

    let mut logs: Vec<String> = fs::read_dir(dir.join("runtime/logs"))
        .expect("program log directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".log"))
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 1, "the shutdown should open no further program logs, got {logs:?}");

    // The three tickets that never ran are untouched, and so is the one that
    // did: an interruption is not a verdict. §FS-rhei-run.3.2
    for id in ["1", "2", "3", "4"] {
        assert_task_state(&plan_path, &machine_path, id, "work");
    }
}

/// A run interrupted while a *program* is in flight must not answer by
/// spawning an agent.
///
/// The two are scheduled by separate loops in the same pass, and only the
/// program loop checked the token. A pass holding one ticket of each kind
/// therefore spent its whole shutdown inside the program loop and then fell
/// through to the sequential agent block with the run already stopping — and
/// started an agent there, under `bypassPermissions`, after the operator had
/// asked the run to stop.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_interrupted_run_starts_no_agent_after_its_sequential_program() {
    let plan = r#"# Rhei: Program Then Agent

## Tasks

### Task 1: Program
**State:** build

### Task 2: Agent
**State:** work
"#;

    let machine = r#"name: program-then-agent
version: 1
states:
  build:
    initial: true
    description: Sleep until told otherwise
    program: >-
      mkdir -p runtime/started
      && : > runtime/started/program
      && sleep 300
  work:
    description: Agent work
    agent: mock
    agent_timeout: 120s
  completed:
    description: Done
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
  - from: work
    to: completed
"#;

    let agent = r#"#!/bin/sh
set -eu
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
mkdir -p "$root/runtime/started"
: > "$root/runtime/started/agent"
exec sleep 300
"#;

    let dir = unique_temp_dir("run-interrupt-program-then-agent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let agent_script = write_fixture_file(&dir, "mock-agent.sh", agent);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "120s" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "stdin_prompt": true, "timeout": "120s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    // `--parallel 1` is what puts the program on the pass's own loop and the
    // agent on the sequential block below it, which is the path under test.
    let mut run = spawn_rhei_run_with(&dir, &plan_path, &machine_path, &["--parallel", "1"]);

    let started_dir = dir.join("runtime/started");
    poll_until("the program to start", TEST_PATIENCE, || started_dir.join("program").exists());

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    assert!(
        !started_dir.join("agent").exists(),
        "an interrupted run must start no agent\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    // No subprocess, so no log and no journal entry either. §FS-rhei-run.3.2
    let logs: Vec<String> = fs::read_dir(dir.join("runtime/logs"))
        .expect("log directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("-work"))
        .collect();
    assert!(logs.is_empty(), "the shutdown should open no agent log, got {logs:?}");

    // Neither ticket moved: the program was interrupted and the agent never ran.
    assert_task_state(&plan_path, &machine_path, "1", "build");
    assert_task_state(&plan_path, &machine_path, "2", "work");
}

/// A `rhei run` driving a real TUI must end when it is signalled, not park on
/// its finished screen.
///
/// The engine joins the render thread before it writes the report and returns
/// its exit status, so a render thread that waits for `q` holds the whole
/// shutdown open: the run left no report, printed nothing, and ignored every
/// further signal. A pty is the only way to see it — the TUI is not selected
/// without one, and the `--no-tui` tests take a different path entirely.
// §FS-rhei-run-tui.1.5.7 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_external_signal_ends_a_tui_run_instead_of_parking_it() {
    use std::io::Read as _;
    use std::os::fd::OwnedFd;
    use std::sync::{Arc, Mutex};

    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-tui-sigterm", GRANDCHILD_AGENT, "120s");

    // A real size, or ratatui has no room to lay anything out: a new terminal is 0x0.
    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = open_native_terminal(&winsize);

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.env("TERM", "xterm-256color");
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        // `--tui` rather than relying on auto-detection, so a failure to reach
        // the TUI is a failure here and not a silent fallback to stdout.
        .arg("--tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard");
    // Both ends of the pty slave: crossterm reads keys from stdin, ratatui
    // draws to stdout, and the frontend picks the TUI from `stdout.is_terminal()`.
    let slave_in: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdin");
    let slave_out: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdout");
    cmd.stdin(std::process::Stdio::from(slave_in));
    cmd.stdout(std::process::Stdio::from(slave_out));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));
    // The child owns the slave now. Every copy left in this process has to go,
    // the `Command`'s own included — `spawn` keeps its `Stdio` handles until
    // the `Command` drops, and one surviving slave fd keeps the master
    // readable forever, hiding the child's exit from the drain thread below.
    drop(cmd);
    drop(pty.slave);

    // Drain the master continuously: a full pty buffer blocks the render
    // thread's writes, which would wedge the very shutdown under test.
    let screen: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let drained = Arc::clone(&screen);
    let mut master = std::fs::File::from(pty.master);
    let drain = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = master.read(&mut buf) {
            if n == 0 {
                break;
            }
            drained.lock().expect("screen buffer").extend_from_slice(&buf[..n]);
        }
    });
    let saw_alternate_screen = || {
        let seen = screen.lock().expect("screen buffer");
        seen.windows(8).any(|w| w == b"\x1b[?1049h")
    };

    poll_until("the TUI to enter the alternate screen", TEST_PATIENCE, saw_alternate_screen);
    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "a signalled TUI run should exit 128+SIGTERM, not wait for `q`\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    join_or_detach(drain);

    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("the grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // The engine got past the render-thread join and finished its own shutdown.
    let report = fs::read_to_string(workspace.join("runtime/run-report.md"))
        .expect("a signalled TUI run should still write its report");
    assert!(
        report.contains("Result: interrupted — re-run to continue"),
        "the report should name the interruption, got:\n{report}"
    );
    assert_task_state(&workspace, &machine_path, "1", "work");
}

/// Ctrl+C on the TUI's finished screen must leave the run its report.
///
/// The screen invites the key — the footer offers `^C` all run — but answering
/// it with `std::process::exit` from the render thread runs no destructor, and
/// the engine, blocked on joining that very thread, never reaches the report it
/// was about to write. The external-signal path was fixed and this one, which
/// the same screen invites, was not.
// §FS-rhei-run-tui.1.5.7 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn ctrl_c_on_the_finished_tui_screen_still_writes_the_report() {
    use std::io::{Read as _, Write as _};
    use std::os::fd::OwnedFd;
    use std::sync::{Arc, Mutex};

    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-tui-finished-ctrl-c", QUICK_AGENT, "120s");

    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = open_native_terminal(&winsize);

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.env("TERM", "xterm-256color");
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        .arg("--tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard");
    let slave_in: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdin");
    let slave_out: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdout");
    cmd.stdin(std::process::Stdio::from(slave_in));
    cmd.stdout(std::process::Stdio::from(slave_out));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));
    drop(cmd);
    drop(pty.slave);

    let screen: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let drained = Arc::clone(&screen);
    let master = std::fs::File::from(pty.master);
    let mut writer = master.try_clone().expect("clone pty master for writing");
    let mut reader = master;
    let drain = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            drained.lock().expect("screen buffer").extend_from_slice(&buf[..n]);
        }
    });

    // The finished status is one rendered span. The adjacent `q to quit`
    // words may be separated by terminal cursor-positioning sequences.
    poll_until("the TUI to park on its finished screen", TEST_PATIENCE, || {
        let seen = screen.lock().expect("screen buffer");
        seen.windows(9).any(|w| w == b"[finished")
    });

    writer.write_all(b"\x03").expect("send Ctrl+C to the TUI");
    writer.flush().expect("flush Ctrl+C");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(130),
        "Ctrl+C should still exit 128+SIGINT\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    join_or_detach(drain);

    // The point of the fix: the engine got past the join and wrote its report.
    let report = fs::read_to_string(workspace.join("runtime/run-report.md"))
        .expect("Ctrl+C on the finished screen should still leave a report");
    // The run had already finished when the key was pressed, so it reports the
    // result it reached — the interruption did not cut anything short.
    assert!(
        !report.contains("interrupted — re-run to continue"),
        "a run that finished before the key was pressed is not an interrupted run, got:\n{report}"
    );
    assert_task_state(&workspace, &machine_path, "1", "completed");
}

/// A fake agent that plays two parts, one per ticket. Ticket 1 waits for the
/// test's `go` marker and then exits `0`, so `rhei run` has something to print
/// at a moment the test chooses. Every other ticket backgrounds a grandchild
/// and sleeps, so a live process group is in flight when that print fails.
#[cfg(unix)]
const LOST_OUTPUT_AGENT: &str = r#"#!/bin/sh
set -eu
prompt="$(cat)"
root="$(printf '%s\n' "$prompt" | sed -n 's/^- This rhei: `\([^`]*\)`.*/\1/p')"
task="$(printf '%s\n' "$prompt" | sed -n 's/^# Task \([^:]*\):.*/\1/p')"
mkdir -p "$root/runtime/pids"
case "${task:?}" in
*1)
  : > "$root/runtime/pids/talker"
  n=0
  while [ ! -f "$root/runtime/go" ] && [ "$n" -lt 900 ]; do
    sleep 0.1
    n=$((n + 1))
  done
  exit 0
  ;;
*)
  sleep 300 &
  printf '%s\n' "$!" > "$root/runtime/pids/grandchild"
  printf '%s\n' "$$" > "$root/runtime/pids/agent"
  exec sleep 300
  ;;
esac
"#;

/// A two-ticket workspace for the lost-output tests: one talker to make the run
/// print, one sleeper with a grandchild to be left behind by the exit.
///
/// `concurrent: true` plus `--parallel 2` is what puts both in flight at once.
#[cfg(unix)]
fn setup_lost_output_workspace(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let machine = r#"name: lost-output
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: Do it
    agent: mock
    agent_timeout: 120s
  human-review:
    description: Wait for a human decision
    gating: true
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: human-review
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir(prefix);
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Lost Output\n").expect("write index");
    fs::write(tasks_dir.join("01-talker.md"), "### Task 1: Talker\n**State:** work\n")
        .expect("write task file");
    fs::write(tasks_dir.join("02-sleeper.md"), "### Task 2: Sleeper\n**State:** work\n")
        .expect("write task file");

    let agent_script = write_fixture_file(&dir, "mock-agent.sh", LOST_OUTPUT_AGENT);
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "120s" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "stdin_prompt": true, "timeout": "120s" }} }}
}}"#
        ),
    )
    .expect("write settings");
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    (dir, workspace, machine_path)
}

/// Losing the run's console output must not lose the run's subprocesses.
///
/// A `println!` to a pipe whose reader is gone panics, and the hook that turns
/// that into a quiet `141` leaves through `std::process::exit` — which runs no
/// destructor, so the shutdown guard never fires. Before this, the agent still
/// in flight was killed only by the Linux parent-death backstop and **its**
/// grandchild survived outright.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
#[cfg(unix)]
#[test]
fn a_closed_stdout_still_ends_the_groups_in_flight() {
    let (dir, workspace, machine_path) = setup_lost_output_workspace("run-lost-stdout-groups");

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        .arg("--no-tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard")
        .arg("--parallel")
        .arg("2");
    cmd.stdin(std::process::Stdio::null());
    // Piped, and deliberately never read: the run's output is far smaller than
    // a pipe buffer, so nothing blocks before the test closes the read end.
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");
    poll_until("the talking agent to start", TEST_PATIENCE, || {
        workspace.join("runtime/pids/talker").exists()
    });

    // The reader is gone; the run's next `println!` has nowhere to go.
    drop(run.stdout.take().expect("piped stdout"));
    fs::write(workspace.join("runtime/go"), "").expect("release the talking agent");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(141),
        "a lost stdout should end the run the way a closed pipe ends a filter\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    poll_until("the in-flight agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("its grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // Nothing transitioned the sleeper: it was terminated, not judged. The
    // talker did finish its state, which is what produced the failed print.
    assert_task_state(&workspace, &machine_path, "2", "work");
}

/// A terminal that goes away must end the run as quietly as a closed pipe does.
///
/// `EIO` on a *terminal* is how a closed pty reports the session hanging up, and
/// the guard that recognises it asks whether the stream is a terminal. It cannot
/// ask that afterwards: the hangup swaps the slave's file operations out, so the
/// `TCGETS` behind `isatty` fails with `EIO` like every other ioctl on it and the
/// stream reads as *not* a terminal from exactly the moment the answer matters.
/// Asked live, the verdict came back "a real write failure", the panic went
/// unrecognised, the report guard's own `println!` panicked again while
/// unwinding — a double panic, which aborts — and the groups in flight were left
/// to the parent-death backstop.
///
/// The pty here is deliberately not the run's controlling terminal, so no
/// `SIGHUP` is delivered and the failed write is the only thing that can end it.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
#[cfg(unix)]
#[test]
fn a_hung_up_terminal_ends_the_groups_the_way_a_closed_pipe_does() {
    use std::os::fd::OwnedFd;

    let (dir, workspace, machine_path) = setup_lost_output_workspace("run-hung-up-terminal");

    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
    // Neither end may reach another process, whether this test's child or one another
    // test spawns on its own thread at any moment. Whoever held the *master* would keep
    // the terminal alive past the close below, and nothing would hang up. §REQ-cross-platform.7.2
    let pty = open_native_terminal(&winsize);

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.env("TERM", "xterm-256color");
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        // `--no-tui` on purpose: this is about recognising the lost terminal,
        // not about the TUI. The run still writes to a real pty.
        .arg("--no-tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard")
        .arg("--parallel")
        .arg("2");
    let slave_out: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdout");
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::from(slave_out));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));
    // Every slave copy in this process has to go, the `Command`'s own included,
    // or the master close below is not a hangup.
    drop(cmd);
    drop(pty.slave);

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");
    poll_until("the talking agent to start", TEST_PATIENCE, || {
        workspace.join("runtime/pids/talker").exists()
    });

    // The operator's window closes. Nothing has read this pty, so the run's
    // output so far is still sitting in a buffer that is about to be discarded.
    drop(pty.master);
    fs::write(workspace.join("runtime/go"), "").expect("release the talking agent");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(141),
        "a terminal that went away should end the run quietly, not abort it\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    // The abort this replaces announced itself; a quiet exit does not.
    let stderr = read_run_stderr(&dir);
    assert!(
        !stderr.contains("panicked"),
        "a lost terminal is not a crash, but stderr said:\n{stderr}"
    );

    poll_until("the in-flight agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("its grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    assert_task_state(&workspace, &machine_path, "2", "work");
}

/// A one-ticket plan whose program ignores `SIGTERM` and outlives its deadline,
/// so the run spends its whole termination grace waiting on it.
#[cfg(unix)]
const GRACE_INTERRUPT_MACHINE: &str = r#"name: grace-interrupt
version: 1
states:
  work:
    initial: true
    description: Outlive the deadline
    program: |
      mkdir -p runtime
      trap ': > runtime/termed' TERM
      : > runtime/started
      while true; do sleep 300 & wait; done
    program_timeout: 2s
  timed-out:
    description: The deadline fired
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
    exit_code: 0
  - from: work
    to: timed-out
    timeout: 2s
  - from: timed-out
    to: completed
"#;

/// A shutdown that arrives *inside* the termination grace outranks the deadline
/// that opened it.
///
/// An invocation past its timeout has ten seconds to flush and commit, and the
/// operator can hit Ctrl+C at any point in them. Reading the stop token only on
/// the way *into* the grace called that a timeout: the timeout transition fired
/// on a ticket the shutdown had promised to leave alone, and the run's own
/// report called it interrupted while the ledger called the ticket timed out.
// §FS-rhei-run.3.2: a shutdown outranks a deadline, whenever it arrives.
#[cfg(unix)]
#[test]
fn a_shutdown_inside_the_timeout_grace_is_an_interruption_not_a_timeout() {
    let plan = r#"# Rhei: Grace Interrupt

## Tasks

### Task 1: One
**State:** work
"#;

    let dir = unique_temp_dir("run-interrupt-inside-grace");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", GRACE_INTERRUPT_MACHINE);

    let mut run = spawn_rhei_run_with(&dir, &plan_path, &machine_path, &["--parallel", "1"]);

    poll_until("the program to start", TEST_PATIENCE, || dir.join("runtime/started").exists());
    // The deadline fires and the engine asks the group to stop. The program
    // records the `SIGTERM` and keeps running, so the grace stays open and the
    // interrupt below lands inside it rather than after it.
    poll_until("the deadline to fire", TEST_PATIENCE, || dir.join("runtime/termed").exists());

    signal_pid(run.id(), "INT");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(130),
        "run should exit 128+SIGINT\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    // The footer is written from the cause the wait reported, so it is where the
    // two readings first disagree.
    let log_dir = dir.join("runtime/logs");
    let mut logs: Vec<PathBuf> = fs::read_dir(&log_dir)
        .expect("program log directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 1, "expected exactly one program log, found {logs:?}");
    let log = fs::read_to_string(&logs[0]).expect("read program log");
    assert!(
        log.contains("interrupted: true"),
        "the footer should name the interruption, got:\n{log}"
    );
    assert!(
        !log.contains("timed_out: true"),
        "an interrupted invocation is not a timed-out one, got:\n{log}"
    );

    // No timeout transition fired, so the ticket is where the shutdown left it.
    assert_task_state(&plan_path, &machine_path, "1", "work");

    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("Result: interrupted — re-run to continue"),
        "the report should name the interruption, got:\n{report}"
    );
}

/// One ticket fails the run while another is still in flight: the shape of
/// "an early `?` return after workers were spawned".
#[cfg(unix)]
const FAILING_PARALLEL_MACHINE: &str = r#"name: failing-parallel
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: One fails, one sleeps
    program: |
      mkdir -p runtime/started runtime/pids
      : > "runtime/started/$RHEI_TASK_ID"
      case "$RHEI_TASK_ID" in
      *1)
        while [ ! -f runtime/go ]; do sleep 0.05; done
        exit 9
        ;;
      *)
        # Ignoring SIGTERM keeps this group alive through the teardown's ask,
        # so the worker waiting on it is guaranteed to poll once while the run
        # is already stopping. Without that the group is simply dead by the
        # next poll and the wait reports `Exited`, which tests nothing.
        trap '' TERM
        sleep 300 &
        printf '%s\n' "$!" > runtime/pids/grandchild
        printf '%s\n' "$$" > runtime/pids/sleeper
        while true; do sleep 300 & wait $!; done
        ;;
      esac
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
    exit_code: 0
"#;

/// A program-state workspace with **one task file per ticket**.
///
/// That layout is what `--parallel > 1` needs: tickets that share a single plan
/// file fall back to sequential execution, because two agents editing one file
/// would conflict. A test that wants two invocations in flight has to give them
/// a file each.
#[cfg(unix)]
fn setup_program_workspace(
    prefix: &str,
    machine: &str,
    titles: &[&str],
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Programs\n").expect("write index");
    for (index, title) in titles.iter().enumerate() {
        let n = index + 1;
        fs::write(
            tasks_dir.join(format!("{n:02}-task.md")),
            format!("### Task {n}: {title}\n**State:** work\n"),
        )
        .expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    (dir, workspace, machine_path)
}

/// A run that fails on its own must not tell the operator it was interrupted.
///
/// The teardown after a failure raises the same stop token an operator's signal
/// does, and the in-flight worker that noticed it printed the operator-facing
/// shutdown notice — `Interrupted — terminating N invocation(s) …; press Ctrl+C
/// again to kill immediately.` Nobody had pressed it once. The advice was wrong
/// and it pointed the operator away from the failure actually being reported.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_run_that_fails_on_its_own_does_not_report_an_interruption() {
    let (dir, workspace, machine_path) = setup_program_workspace(
        "run-failure-not-interruption",
        FAILING_PARALLEL_MACHINE,
        &["Fails", "Sleeps"],
    );

    let mut run = spawn_rhei_run_with(&dir, &workspace, &machine_path, &["--parallel", "2"]);

    let started_dir = workspace.join("runtime/started");
    poll_until("both programs to start", TEST_PATIENCE, || {
        fs::read_dir(&started_dir).map(|entries| entries.count() >= 2).unwrap_or(false)
    });
    let sleeper = wait_for_recorded_pid(&workspace, "sleeper");
    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");

    fs::write(workspace.join("runtime/go"), "").expect("release the failing program");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    let stderr = read_run_stderr(&dir);
    assert_ne!(status.code(), Some(0), "the run failed\nstderr:\n{stderr}");
    assert_ne!(status.code(), Some(130), "nobody signalled it\nstderr:\n{stderr}");

    assert!(
        stderr.contains("program exited with code 9"),
        "the failure is what should be reported, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("press Ctrl+C again"),
        "a run nobody interrupted must not offer a second Ctrl+C, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("Interrupted — terminating"),
        "a failing run's teardown is not an interruption, got:\n{stderr}"
    );

    // It is still a teardown: the guard takes the group that was in flight.
    poll_until("the in-flight program to be gone", TEST_PATIENCE, || !pid_is_alive(&sleeper));
    poll_until("its grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // And the report is about the failure, not about a shutdown.
    let report = fs::read_to_string(workspace.join("runtime/run-report.md")).expect("run report");
    assert!(
        !report.contains("interrupted — re-run to continue"),
        "a failing run must not tell the operator to re-run, got:\n{report}"
    );
}

/// One ticket whose program fails, and nothing else in flight: the run leaves
/// by an error with no worker left holding a reference to the sink, so the
/// frontend really is the last owner of the TUI and its drop really does join
/// the render thread.
#[cfg(unix)]
const FAILING_LONE_MACHINE: &str = r#"name: failing-lone
version: 1
states:
  work:
    initial: true
    description: Fail once released
    program: |
      mkdir -p runtime
      : > runtime/started
      while [ ! -f runtime/go ]; do sleep 0.05; done
      exit 9
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
    exit_code: 0
"#;

/// A TUI run that fails must leave its screen instead of parking on it.
///
/// The subprocess guard was declared *before* the frontend, so the frontend
/// dropped first and asked "is this run shutting down" before anything had said
/// so. The answer was no, the render thread parked on the finished screen
/// waiting for a `q`, and `TuiSink::finish` blocked on joining it — so a failed
/// run hung there indefinitely, never writing its report and never reporting
/// its error, until an operator who had no reason to still be watching pressed
/// a key.
///
/// The surface is asked through a value the *run* owns for the same reason: by
/// the time it is asked, the guard has handed its thread-local ownership back,
/// and a reading taken through that answers "no run is stopping" for the very
/// run that is.
// §FS-rhei-run-tui.1.5.7 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_failing_tui_run_leaves_its_screen_instead_of_parking_on_it() {
    use std::io::Read as _;
    use std::os::fd::OwnedFd;
    use std::sync::{Arc, Mutex};

    let (dir, workspace, machine_path) =
        setup_program_workspace("run-tui-failure-parks", FAILING_LONE_MACHINE, &["Fails"]);

    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = open_native_terminal(&winsize);

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.env("TERM", "xterm-256color");
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        .arg("--tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard")
        .arg("--parallel")
        .arg("1");
    let slave_in: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdin");
    let slave_out: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdout");
    cmd.stdin(std::process::Stdio::from(slave_in));
    cmd.stdout(std::process::Stdio::from(slave_out));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));
    drop(cmd);
    drop(pty.slave);

    // Drain the master: a full pty buffer blocks the render thread's writes,
    // which would wedge the very shutdown under test.
    let screen: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let drained = Arc::clone(&screen);
    let mut master = std::fs::File::from(pty.master);
    let drain = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = master.read(&mut buf) {
            if n == 0 {
                break;
            }
            drained.lock().expect("screen buffer").extend_from_slice(&buf[..n]);
        }
    });

    poll_until("the TUI to enter the alternate screen", TEST_PATIENCE, || {
        let seen = screen.lock().expect("screen buffer");
        seen.windows(8).any(|w| w == b"\x1b[?1049h")
    });

    poll_until("the program to start", TEST_PATIENCE, || {
        workspace.join("runtime/started").exists()
    });
    fs::write(workspace.join("runtime/go"), "").expect("release the failing program");

    // No key is ever sent to this pty. A run that waits for `q` never returns.
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_ne!(status.code(), Some(0), "the run failed\nstderr:\n{}", read_run_stderr(&dir));
    join_or_detach(drain);

    // It got past the render-thread join, so it still had a turn to say why.
    let stderr = read_run_stderr(&dir);
    assert!(
        stderr.contains("program exited with code 9"),
        "a failing run should still report its failure, got:\n{stderr}"
    );
}

/// A freed slot must not be refilled once the run is interrupted.
///
/// The sequential loop's case is covered above; this is the parallel scheduler,
/// where the decision to start more work is taken in `refill_parallel_worker_pool`
/// and again in each work item — and where the item is handed to a worker thread
/// that only reaches the spawn some way later.
// §FS-rhei-run.3.2: an interrupted run starts nothing further.
#[cfg(unix)]
#[test]
fn an_interrupted_run_refills_no_parallel_slot() {
    let machine = r#"name: refill
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: Sleep until told otherwise
    program: >-
      mkdir -p runtime/started
      && : > "runtime/started/$RHEI_TASK_ID"
      && sleep 300
  completed:
    description: Done
    final: true
transitions:
  - from: work
    to: completed
    exit_code: 0
"#;

    let (dir, workspace, machine_path) = setup_program_workspace(
        "run-interrupt-parallel-refill",
        machine,
        &["One", "Two", "Three", "Four"],
    );

    // Two slots for four tickets, so two are in flight and two are queued.
    let mut run = spawn_rhei_run_with(&dir, &workspace, &machine_path, &["--parallel", "2"]);

    let started_dir = workspace.join("runtime/started");
    poll_until("both slots to fill", TEST_PATIENCE, || {
        fs::read_dir(&started_dir).map(|entries| entries.count() >= 2).unwrap_or(false)
    });

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    let started: Vec<String> = fs::read_dir(&started_dir)
        .expect("started marker directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(started.len(), 2, "only the two in-flight programs may have run, got {started:?}");

    let logs: Vec<String> = fs::read_dir(workspace.join("runtime/logs"))
        .expect("program log directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".log"))
        .collect();
    assert_eq!(logs.len(), 2, "the shutdown should open no further program logs, got {logs:?}");

    for id in ["1", "2", "3", "4"] {
        assert_task_state(&workspace, &machine_path, id, "work");
    }
}
