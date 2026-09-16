//! Provider reset limits are durable scheduler waits, not agent failures.

use std::fs;
use std::path::Path;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

const LIMIT_SIGNAL: &str = "You've hit your session limit · resets 10:20pm (Europe/Zurich)";

struct RunningChild(Option<Child>);

impl RunningChild {
    fn child(&mut self) -> &mut Child {
        self.0.as_mut().expect("run child")
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RunningChild {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {what}");
}

fn count_files(path: &Path) -> usize {
    fs::read_dir(path).map(|entries| entries.filter_map(Result::ok).count()).unwrap_or(0)
}

fn markdown_text(path: &Path) -> String {
    fn visit(path: &Path, text: &mut String) {
        for entry in fs::read_dir(path).expect("read workspace") {
            let path = entry.expect("workspace entry").path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("runtime") {
                    visit(&path, text);
                }
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
                text.push_str(&fs::read_to_string(path).expect("read markdown"));
            }
        }
    }

    let mut text = String::new();
    visit(path, &mut text);
    text
}

fn expire_provider_deadlines(path: &Path) {
    for entry in fs::read_dir(path).expect("read workspace") {
        let path = entry.expect("workspace entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("runtime") {
                expire_provider_deadlines(&path);
            }
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let original = fs::read_to_string(&path).expect("read markdown");
        let mut changed = false;
        let rewritten = original
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("nextAttemptAt:") {
                    if let Some(indent) = line.strip_suffix(line.trim_start()) {
                        changed = true;
                        return format!("{indent}nextAttemptAt: \"2000-01-01T00:00:00Z\"");
                    }
                }
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if changed {
            fs::write(path, format!("{rewritten}\n")).expect("expire provider deadline");
        }
    }
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
      "timeout": "10s",
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
