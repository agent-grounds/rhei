//! The fixtures the `--until-idle` regression set runs on, and the one bounded
//! way it spawns a run.
//!
//! Every case built on this asserts an observable exit status plus report or
//! stream content. None asserts elapsed time: that a run *returned* is the
//! observable, and that it returned *quickly* is not one — a test asserting
//! that a continuous run would not have returned is the test this suite must
//! not contain. Every deadline is seeded as plan metadata and asserted against
//! the seeded instant, which is the suite's existing idiom.
// §FS-rhei-run.2.1 §FS-rhei-run.3 §REQ-cross-platform.4

use super::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Idle. Produced only when `--until-idle` is selected. §FS-rhei-run-json.5
pub(super) const EXIT_IDLE: i32 = 3;
/// A usage refusal, including this option beside `--tui` or `--headless`.
/// §FS-rhei-run-json.5
pub(super) const EXIT_USAGE: i32 = 2;

/// Seeded instants, written absolutely rather than computed from `now()`.
///
/// A fixed instant is what lets a case assert the *exact* string the run
/// reports; `now + N` would make the expectation move with the test's own
/// clock reading. The two future values are an hour apart so a case can tell
/// "the earliest" from "the later" without arithmetic, and the elapsed one is
/// far enough back that no scheduler reading can find it outstanding.
// §FS-rhei-run.5.1
pub(super) const FUTURE_POLL: u64 = 1_893_456_000; // 2030-01-01T00:00:00Z
pub(super) const LATER_POLL: u64 = 1_893_459_600; // 2030-01-01T01:00:00Z
pub(super) const ELAPSED_POLL: u64 = 946_684_800; // 2000-01-01T00:00:00Z

pub(super) fn instant(epoch: u64) -> String {
    super::provider_limit_support::utc_at(epoch)
}

/// One machine for the whole set, so a case differs from its neighbours only
/// in the plan it is given: work that finishes at once, a human gate, a timed
/// retry, agent work a provider limit can park, and a worker that exits `0`
/// without the artifact it owes.
pub(super) fn idle_machine(dir: &Path) -> String {
    let work =
        fixture_command(&write_python_agent(dir, "work.py", "result('## Result\\n\\nDone.\\n')\n"));
    let silent = fixture_command(&write_python_agent(
        dir,
        "silent.py",
        "result('## Result\\n\\nDone.\\n')\n",
    ));
    format!(
        r#"name: until-idle
version: 1
states:
  work:
    initial: true
    description: Work that finishes at once
    program:
      command: {work}
    program_timeout: 2m
  gate:
    description: Waiting on a human decision
    gating: true
  poll:
    description: A timed retry
    program:
      command: {work}
    program_timeout: 2m
    poll:
      interval: 30m
      max_attempts: 5
  parked:
    description: Agent work a provider limit can park
    target: codex:openai:alpha
    attempts: 3
  timed:
    description: Agent work carrying both a poll deadline and a provider limit
    target: codex:openai:alpha
    attempts: 3
    poll:
      interval: 30m
      max_attempts: 5
  silent:
    description: A worker that exits 0 without the artifact it owes
    program:
      command: {silent}
    program_timeout: 2m
    outputs:
      - name: note
        path: runtime/note.md
  done:
    final: true
    description: Done
transitions:
  - from: work
    to: done
    exit_code: 0
  - from: gate
    to: done
  - from: poll
    to: poll
    condition: pollAttempts < pollMaxAttempts
  - from: poll
    to: done
    condition: pollAttempts >= pollMaxAttempts
  - from: parked
    to: done
  - from: timed
    to: timed
    condition: pollAttempts < pollMaxAttempts
  - from: timed
    to: done
    condition: pollAttempts >= pollMaxAttempts
  - from: silent
    to: done
    exit_code: 0
"#
    )
}

/// A plan, its machine, and the isolated `HOME` every run of it uses.
pub(super) struct IdlePlan {
    /// The workspace root, and the guard that removes it again.
    pub dir: TestDir,
    pub plan: PathBuf,
    pub machine: PathBuf,
    pub home: PathBuf,
}

impl IdlePlan {
    /// A single-file plan. `body` is everything after the `# Rhei:` heading,
    /// so a case seeds its own `metadata:` front matter inline rather than
    /// rewriting the file afterwards.
    pub fn new(prefix: &str, body: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let machine = write_fixture_file(&dir, "states.yaml", &idle_machine(&dir));
        let plan =
            write_fixture_file(&dir, "plan.rhei.md", &format!("# Rhei: Until Idle\n\n{body}"));
        let home = dir.join(".home");
        // An agent profile has to resolve before a state that declares
        // `target:` can even be scheduled, so every plan carries one whether
        // or not its case parks on a provider limit. §FS-rhei-run.3
        write_agent_settings(&dir);
        Self { dir, plan, machine, home }
    }

    /// The workspace root the run writes its `runtime/` tree under.
    pub fn root(&self) -> &Path {
        &self.dir
    }

    pub fn run(&self, args: &[&str]) -> CliRun {
        let mut command = rhei_command(&self.home);
        command.arg("--state-machine").arg(&self.machine).arg("run").arg(&self.plan);
        command.args(args);
        bounded(command, &format!("rhei run {}", args.join(" ")), self.root())
    }

    /// Any other `rhei` subcommand against this plan's isolated `HOME`.
    ///
    /// Gated exactly as its one caller is: the detached control case
    /// (`until_idle_frontend_tests`, case 14) is `#[cfg(unix)]`, so on Windows
    /// this method is dead and `-D warnings` would fail the whole `e2e`
    /// target — all of it, not the twenty cases this set adds.
    #[cfg(unix)]
    pub fn rhei(&self, args: &[&str]) -> CliRun {
        let mut command = rhei_command(&self.home);
        command.args(args);
        bounded(command, &format!("rhei {}", args.join(" ")), self.root())
    }

    /// Nothing a refused startup must not have written: the run lock, the
    /// descriptor, the journal, and the durable event log.
    /// §FS-rhei-run-tui.1.4
    #[track_caller]
    pub fn assert_nothing_written(&self) {
        for relative in [
            ".rhei/run.lock",
            "runtime/run.json",
            "runtime/transitions.log",
            "runtime/events.jsonl",
        ] {
            let path = self.root().join(relative);
            assert!(!path.exists(), "a refusal at startup must not write {relative}");
        }
    }

    pub fn report(&self) -> String {
        fs::read_to_string(self.root().join("runtime/run-report.md")).unwrap_or_default()
    }

    pub fn records(&self) -> Vec<PathBuf> {
        fs::read_dir(self.root().join("runtime/spawns"))
            .map(|entries| entries.map(|entry| entry.expect("spawn record").path()).collect())
            .unwrap_or_default()
    }

    pub fn plan_text(&self) -> String {
        fs::read_to_string(&self.plan).expect("read the plan back")
    }
}

/// The agent profile a `target:` state resolves through. The command is a
/// fixture that never runs in these cases: what the profile is for is that the
/// state resolves at all.
fn write_agent_settings(dir: &Path) {
    let agent = write_python_agent(dir, "agent.py", "result('## Result\\n\\nDone.\\n')\n");
    let settings = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("settings directory");
    let profile = serde_json::json!({
        "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&agent))
            .expect("fixture command"),
        "stdin_prompt": true,
        "timeout": "12s"
    });
    fs::write(
        settings.join("settings.json"),
        serde_json::json!({"agents": {"codex": profile}}).to_string(),
    )
    .expect("write agent settings");
}

/// A provider-limit record on `state`, parked at `deadline`, in the shape the
/// run persists. Indented to sit under a task's `metadata.tasks.<id>` mapping.
/// §FS-rhei-run.3.3
pub(super) fn provider_limit(state: &str, deadline: u64) -> String {
    format!(
        r#"providerLimits:
        {state}:
          identity:
            agent: codex
            provider: openai
          signal: "You've hit your session limit"
          observedAt: "{}"
          nextAttemptAt: "{}""#,
        instant(ELAPSED_POLL),
        instant(deadline)
    )
}

/// How long a run in this set may take before the test calls it a hang.
///
/// The suite's ordinary `run_cli` blocks for as long as the child does, which
/// is right for a command that always returns. It is not right here: the whole
/// contract under test is *returning instead of sleeping*, so a regression
/// that reinstates the sleep would wedge CI rather than fail a case. This is a
/// hang guard and nothing else — no case asserts anything about the time a run
/// took, and the bound is far above any of their real durations.
const PATIENCE: Duration = Duration::from_secs(90);

/// Spawn `command`, wait for it under [`PATIENCE`], and read what it wrote.
///
/// Output goes to files rather than pipes because the wait has to poll: a
/// child that filled a pipe nobody was draining would block, and the guard
/// would report a hang that was the harness's own.
pub(super) fn bounded(mut command: Command, what: &str, scratch: &Path) -> CliRun {
    let out_path = scratch.join(format!("run-out-{}", unique_suffix()));
    let err_path = out_path.with_extension("err");
    let out = fs::File::create(&out_path).expect("run stdout file");
    let err = fs::File::create(&err_path).expect("run stderr file");
    let mut child = command
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {what}: {error}"));
    let deadline = Instant::now() + PATIENCE;
    let status = loop {
        match child.try_wait().expect("inspect run status") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{what} had not returned after {PATIENCE:?}\nstdout:\n{}\nstderr:\n{}",
                    fs::read_to_string(&out_path).unwrap_or_default(),
                    fs::read_to_string(&err_path).unwrap_or_default()
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    CliRun {
        status,
        stdout: fs::read_to_string(&out_path).unwrap_or_default(),
        stderr: rendered_stderr::undo_soft_wrap(&fs::read_to_string(&err_path).unwrap_or_default()),
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos()
}

/// The exit status, named, so a failure says which code arrived.
#[track_caller]
pub(super) fn assert_exit(result: &CliRun, expected: i32, what: &str) {
    assert_eq!(
        result.status.code(),
        Some(expected),
        "{what}\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// The console result line of an idle return: the `Run idle:` discriminator,
/// the blocker word from the same closed vocabulary the stream uses, the
/// reported instant (or the literal `none`), and never `Run complete:`.
/// §FS-rhei-run-report.3.1
#[track_caller]
pub(super) fn assert_idle_line(result: &CliRun, blocker: &str, next_attempt: &str) {
    let combined = format!("{}{}", result.stdout, result.stderr);
    for expected in
        ["Run idle:", &format!("waiting on {blocker}"), &format!("next attempt {next_attempt}")]
    {
        assert!(
            combined.contains(expected),
            "the idle result line should carry {expected:?}; got:\n{combined}"
        );
    }
    assert!(
        !combined.contains("Run complete:"),
        "an idle return must never print `Run complete:`; got:\n{combined}"
    );
}

/// A refusal that is about the *combination*, not about one flag nobody knows.
///
/// The distinction is the whole point of asserting it: an unrecognized flag is
/// itself a usage refusal with the same status, writing nothing, so a case
/// that only checked the status and the flag names would pass today for a
/// reason the change has nothing to do with. Only a declared conflict can say
/// that both flags are known and that *together* they are refused.
// §FS-rhei-run-tui.1.4
#[track_caller]
pub(super) fn assert_conflict(result: &CliRun, other: &str) {
    assert!(
        !result.stderr.contains("unexpected argument"),
        "both flags must be known, and the refusal about the pair; got:\n{}",
        result.stderr
    );
    for flag in ["--until-idle", other] {
        assert!(result.stderr.contains(flag), "the refusal names {flag}; got:\n{}", result.stderr);
    }
}

/// `run_finished.summary.stop`, which is present exactly when the option is
/// selected. §FS-rhei-run-json.2.1
#[track_caller]
pub(super) fn stop_payload(result: &CliRun) -> serde_json::Value {
    let finished = records_of(result, "run_finished");
    let record = finished
        .first()
        .unwrap_or_else(|| panic!("the stream should carry run_finished; got:\n{}", result.stdout));
    let stop = record["summary"]["stop"].clone();
    assert!(!stop.is_null(), "run_finished.summary should carry the stop object; got:\n{record}");
    stop
}

pub(super) fn records_of(result: &CliRun, event: &str) -> Vec<serde_json::Value> {
    result
        .stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == event)
        .collect()
}
