//! Provider reset limits are durable scheduler waits, not agent failures.

use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use super::*;

use super::provider_limit_support::*;

fn count_files(path: &Path) -> usize {
    fs::read_dir(path).map(|entries| entries.filter_map(Result::ok).count()).unwrap_or(0)
}

fn provider_limit_workspace() -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let index = "# Rhei: Provider limit parking\n";
    let tasks = (1..=8)
        .map(|number| {
            (
                format!("{number:02}-limited.md"),
                format!("### Task {number}: Limited task {number}\n**State:** working\n"),
            )
        })
        .collect::<Vec<_>>();
    let borrowed =
        tasks.iter().map(|(name, contents)| (name.as_str(), contents.as_str())).collect::<Vec<_>>();
    let (dir, workspace, machine) = create_workspace("provider-limit-parking", index, &borrowed);

    let agent = write_python_agent(
        &dir,
        "limited-codex.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
starts = root / 'runtime' / 'provider-limit-starts'
write(starts / (local + '.txt'), env('RHEI_ATTEMPT'))
marker = root / 'runtime' / 'provider-limit-refusals' / (local + '.txt')
if not marker.exists():
    write(marker, 'refused\n')
    deadline = time.time() + 5
    while len(list(starts.glob('*.txt'))) < 8 and time.time() < deadline:
        time.sleep(0.01)
    time.sleep(11)
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
result('## Result\n\nResumed after the provider wait.\n')
write(root / 'runtime' / 'provider-limit-resumed' / (local + '.txt'), 'resumed\n')
"#
        ),
    );
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "codex": {{
      "command": {},
      "stdin_prompt": true,
      "timeout": "20s",
      "modes": {{ "yolo": [] }}
    }}
  }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");
    fs::write(
        &machine,
        r#"name: provider-limit-parking
version: 1
states:
  working:
    initial: true
    concurrent: true
    target: codex[yolo]:openai:gpt-5.6-sol
    attempts: 1
  completed:
    final: true
transitions:
  - from: working
    to: completed
"#,
    )
    .expect("write machine");
    (dir, workspace, machine)
}

fn sequential_provider_limit_workspace(
    prefix: &str,
) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let tasks = [("01-limited.md", "### Task 1: Limited task\n**State:** working\n")];
    let (dir, workspace, machine) =
        create_workspace(prefix, "# Rhei: Sequential provider limit\n", &tasks);
    let agent = write_python_agent(
        &dir,
        "sequential-limited-codex.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
marker = root / 'runtime' / 'provider-limit-refused.txt'
starts = root / 'runtime' / 'provider-limit-starts.txt'
write(starts, (starts.read_text() if starts.exists() else '') + env('RHEI_ATTEMPT') + '\n')
if not marker.exists():
    write(marker, 'refused\n')
    print({LIMIT_SIGNAL:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
result('## Result\n\nResumed after the provider wait.\n')
"#
        ),
    );
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{"agents":{{"codex":{{"command":{},"stdin_prompt":true,"timeout":"10s"}}}}}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");
    fs::write(
        &machine,
        r#"name: sequential-provider-limit
version: 1
states:
  working:
    initial: true
    target: codex:openai:gpt-5.6-sol
    attempts: 1
  completed:
    final: true
transitions:
  - from: working
    to: completed
"#,
    )
    .expect("write machine");
    (dir, workspace, machine)
}

/// Eight simultaneous reset-bearing Codex refusals remain parked, auditable,
/// uncharged, and resumable instead of ending the run as eight failures.
/// §FS-rhei-agents.2 §FS-rhei-agents.3.2.3 §FS-rhei-agents.5.2.2
/// §FS-rhei-agents.8.4 §FS-rhei-run.3.3 §FS-rhei-run.5.1
#[test]
fn eight_parallel_codex_limits_park_and_resume_without_spending_attempts() {
    let (_dir, workspace, machine) = provider_limit_workspace();
    let starts = workspace.join("runtime/provider-limit-starts");
    let mut command = rhei_command(workspace.join(".home"));
    command
        .arg("--state-machine")
        .arg(&machine)
        .arg("run")
        .arg(&workspace)
        .args(["--no-tui", "--no-dashboard", "--no-callbacks", "--parallel", "8"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut run = RunningChild(Some(command.spawn().expect("spawn rhei run")));

    wait_for("all eight controlled agents to start", || count_files(&starts) == 8);
    let persistence_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if markdown_text(&workspace).matches("nextAttemptAt:").count() == 8 {
            break;
        }
        if let Some(status) = run.child().try_wait().expect("inspect run status") {
            panic!(
                "recognized provider limits followed the ordinary failure path: the eight-worker run exited {status} instead of parking"
            );
        }
        assert!(
            Instant::now() < persistence_deadline,
            "the run stayed live but did not persist all eight provider waits"
        );
        thread::sleep(Duration::from_millis(25));
    }
    let parked = markdown_text(&workspace);
    assert_eq!(parked.matches("providerLimits:").count(), 8, "{parked}");
    assert_all_tasks_in_state(&workspace, &machine, "working");

    let spawn_dir = workspace.join("runtime/spawns");
    let records = fs::read_dir(&spawn_dir)
        .expect("read spawn records")
        .filter_map(Result::ok)
        .map(|entry| {
            serde_json::from_str::<serde_json::Value>(
                &fs::read_to_string(entry.path()).expect("read spawn record"),
            )
            .expect("parse spawn record")
        })
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 8, "one retained spawn record per refusal");
    for record in &records {
        assert_eq!(record["ending"], "provider_limited", "{record:#}");
        assert_eq!(record["attempt_charged"], false, "{record:#}");
        assert_eq!(record["code"], 1, "{record:#}");
    }

    let journal =
        fs::read_to_string(workspace.join("runtime/transitions.log")).expect("read run journal");
    assert_eq!(journal.matches("outcome=provider_limited").count(), 8, "{journal}");
    assert!(journal.contains("provider=openai"), "{journal}");

    run.stop();
    expire_provider_deadlines(&workspace);
    let resumed = run_cli(
        "run",
        &workspace,
        &machine,
        &["--no-tui", "--no-dashboard", "--no-callbacks", "--parallel", "8"],
    );
    assert_success(&resumed);
    assert_all_tasks_in_state(&workspace, &machine, "completed");
    assert_eq!(count_files(&workspace.join("runtime/provider-limit-resumed")), 8);
    assert!(
        !markdown_text(&workspace).contains("providerLimits:"),
        "successful resumption must clear provider-limit records"
    );
}

/// A sequential foreground run re-reads durable waits while sleeping, so a
/// controlled deadline advance wakes the same process and resumes uncharged.
/// §FS-rhei-agents.5.2.1 §FS-rhei-run.3.3 §FS-rhei-run.5.1
#[test]
fn sequential_provider_limit_wakes_and_resumes_in_process() {
    let (_dir, workspace, machine) =
        sequential_provider_limit_workspace("provider-limit-sequential-wakeup");
    let mut command = rhei_command(workspace.join(".home"));
    command
        .arg("--state-machine")
        .arg(&machine)
        .arg("run")
        .arg(&workspace)
        .args(["--no-tui", "--no-dashboard", "--no-callbacks"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut run = RunningChild(Some(command.spawn().expect("spawn rhei run")));
    wait_for("the sequential provider wait", || {
        markdown_text(&workspace).contains("nextAttemptAt:")
    });
    expire_provider_deadlines(&workspace);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = run.child().try_wait().expect("inspect run status") {
            break status;
        }
        assert!(Instant::now() < deadline, "the parked run did not resume");
        thread::sleep(Duration::from_millis(25));
    };
    assert!(status.success());
    assert_all_tasks_in_state(&workspace, &machine, "completed");
    let starts = fs::read_to_string(workspace.join("runtime/provider-limit-starts.txt")).unwrap();
    assert_eq!(starts.lines().collect::<Vec<_>>(), ["1", "2"]);
}

/// Restarting before expiry preserves the wait and does not spend or launch a
/// second attempt early. §FS-rhei-run.3.3
#[test]
fn restart_before_provider_deadline_remains_parked() {
    let (_dir, workspace, machine) =
        sequential_provider_limit_workspace("provider-limit-restart-future");
    let spawn = || {
        let mut command = rhei_command(workspace.join(".home"));
        command
            .arg("--state-machine")
            .arg(&machine)
            .arg("run")
            .arg(&workspace)
            .args(["--no-tui", "--no-dashboard", "--no-callbacks"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        RunningChild(Some(command.spawn().expect("spawn rhei run")))
    };
    let mut first = spawn();
    wait_for("the persisted provider wait", || {
        markdown_text(&workspace).contains("nextAttemptAt:")
    });
    first.stop();

    let mut restarted = spawn();
    thread::sleep(Duration::from_millis(750));
    assert!(restarted.child().try_wait().unwrap().is_none(), "restart must keep waiting");
    let starts = fs::read_to_string(workspace.join("runtime/provider-limit-starts.txt")).unwrap();
    assert_eq!(starts.lines().collect::<Vec<_>>(), ["1"]);
    restarted.stop();
}

fn ordinary_failure_case(prefix: &str, continue_on_error: bool) -> (CliRun, usize) {
    let tasks = [
        ("01-first.md", "### Task 1: First ordinary failure\n**State:** working\n"),
        ("02-second.md", "### Task 2: Second ordinary failure\n**State:** working\n"),
    ];
    let (dir, workspace, machine) = create_workspace(prefix, "# Rhei: Ordinary failures\n", &tasks);
    let agent = write_python_agent(
        &dir,
        "ordinary-failure.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'ordinary-starts' / (env('RHEI_TASK_ID_LOCAL') + '.txt'), 'started\n')
print('ordinary agent failure', file=sys.stderr)
raise SystemExit(1)
"#,
    );
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{"agents":{{"codex":{{"command":{},"stdin_prompt":true,"timeout":"10s"}}}}}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");
    fs::write(
        &machine,
        r#"name: ordinary-failure-control
version: 1
states:
  working:
    initial: true
    agent: codex
    attempts: 1
  completed:
    final: true
transitions:
  - from: working
    to: completed
"#,
    )
    .expect("write machine");
    let extra = if continue_on_error {
        vec!["--no-tui", "--no-dashboard", "--no-callbacks", "--continue-on-error"]
    } else {
        vec!["--no-tui", "--no-dashboard", "--no-callbacks"]
    };
    let result = run_cli("run", &workspace, &machine, &extra);
    assert_all_tasks_in_state(&workspace, &machine, "working");
    let starts = count_files(&workspace.join("runtime/ordinary-starts"));
    (result, starts)
}

/// Unrecognized non-zero results retain the stop/skip behavior that provider
/// classification is deliberately narrower than. §FS-rhei-agents.5.2.1
#[test]
fn ordinary_agent_failures_keep_continue_on_error_semantics() {
    let (stopped, stopped_starts) = ordinary_failure_case("ordinary-failure-stop", false);
    assert!(!stopped.status.success(), "ordinary failure must still stop the run");
    assert_eq!(stopped_starts, 1, "without the flag the second task must not start");

    let (continued, continued_starts) = ordinary_failure_case("ordinary-failure-continue", true);
    assert_eq!(continued_starts, 2, "with the flag both ordinary failures must be visited");
    assert!(
        !continued.stderr.contains("provider_limited"),
        "ordinary failures must not be reclassified: {}",
        continued.stderr
    );
}
