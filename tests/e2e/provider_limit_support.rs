//! Controlled provider fixtures and durable wait edits. §FS-rhei-run.3.3

use super::*;
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

pub(super) const LIMIT_SIGNAL: &str =
    "You've hit your session limit · resets 10:20pm (Europe/Zurich)";

/// How long a watched wait gives a live run to show the next event it waits on.
pub(super) const PROVIDER_WAIT_PATIENCE: Duration = Duration::from_secs(30);

/// How long a fixture agent waits for a marker the test writes. One agent can be
/// alive across several of the harness's watched waits - the scheduling test's
/// in-flight sibling outlives four, and the test's own edits between them - so
/// the marker wait is a patience longer than four: a stuck run is reported by
/// the wait that watches it, never by a fixture agent that gave up first. It is
/// paid only when a marker never comes.
const MARKER_PATIENCE: Duration = Duration::from_secs(5 * PROVIDER_WAIT_PATIENCE.as_secs());

/// The fixture profile's timeout: a patience past the marker wait, so a marker
/// that never comes ends the agent with the fixture's own error, not a timeout.
const FIXTURE_TIMEOUT: Duration =
    Duration::from_secs(MARKER_PATIENCE.as_secs() + PROVIDER_WAIT_PATIENCE.as_secs());

/// How long a harness edit waits out a refusal by another open handle. It is
/// paid only while refused, so it is sized for a loaded Windows runner.
const REFUSAL_PATIENCE: Duration = Duration::from_secs(10);

/// A live `rhei run` and the files in the test's own directory that its stdout
/// and stderr go to, so a run that dies or stalls says why instead of leaving a
/// wait to time out: `rhei run` tells what it spawned and advanced on stdout, and
/// its warnings and errors - an agent's timeout, a halt - on stderr.
pub(super) struct RunningChild {
    child: Option<Child>,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl RunningChild {
    /// Adopt a run whose stdout and stderr the caller already sent to `output`.
    pub(super) fn new(child: Child, output: PathBuf) -> Self {
        Self { child: Some(child), stdout: output.clone(), stderr: output }
    }

    /// Spawn `command`, keeping its stdout in `<name>.stdout` and its stderr in
    /// `<name>.stderr`, both in `dir`.
    pub(super) fn spawn(command: &mut Command, dir: &Path, name: &str) -> Self {
        let stdout = dir.join(format!("{name}.stdout"));
        let stderr = dir.join(format!("{name}.stderr"));
        let out = fs::File::create(&stdout).expect("create the run's stdout file");
        let err = fs::File::create(&stderr).expect("create the run's stderr file");
        let child = command.stdout(out).stderr(err).spawn().expect("spawn rhei run");
        Self { child: Some(child), stdout, stderr }
    }

    pub(super) fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("run child")
    }

    /// What the run has printed so far, for the message of a wait that failed:
    /// its stdout and its stderr, each read through the harness's seam.
    /// §FS-rhei-errors.2
    pub(super) fn output(&self) -> String {
        let stderr = stderr_from_file(&self.stderr);
        if self.stdout == self.stderr {
            return format!("its output:\n{stderr}");
        }
        format!("its stdout:\n{}\nits stderr:\n{stderr}", stdout_from_file(&self.stdout))
    }

    /// Wait for the run to exit, as long as the slowest runner needs.
    pub(super) fn wait_for_exit(&mut self, what: &str) -> ExitStatus {
        let deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
        loop {
            if let Some(status) = self.child().try_wait().expect("inspect run status") {
                return status;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {what}: the run stayed live for {PROVIDER_WAIT_PATIENCE:?}; {}",
                    self.output()
                );
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    pub(super) fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
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

/// Wait for `condition` while `run` is live. The exit is inspected before the
/// condition, so a condition that holds once the run has finished is still met;
/// a run that exited without it fails the wait at once, with its exit status and
/// its output, rather than spinning out the patience as a timeout. §FS-rhei-run.3.3
pub(super) fn wait_for(what: &str, run: &mut RunningChild, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
    loop {
        let exited = run.child().try_wait().expect("inspect run status");
        if condition() {
            return;
        }
        if let Some(status) = exited {
            panic!("the run exited {status} before {what}; {}", run.output());
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for {what}: the run stayed live for {PROVIDER_WAIT_PATIENCE:?}; {}",
                run.output()
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Repeat `attempt` - a read or a replacement of a file the live run also reads
/// and replaces - while the run's open handle refuses it, as Windows does, and
/// panic with `what` and the last error once the refusal outlasts
/// `REFUSAL_PATIENCE`. A refusal is judged the way rhei's own writer judges
/// one, and any other failure panics at once. §FS-rhei-transition-cmd.3
fn waiting_out_refusals<T>(what: &str, mut attempt: impl FnMut() -> std::io::Result<T>) -> T {
    let deadline = Instant::now() + REFUSAL_PATIENCE;
    loop {
        match attempt() {
            Ok(value) => return value,
            Err(error) if refused(&error) && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("{what}: {error:?}"),
        }
    }
}

/// `PermissionDenied` everywhere, and Windows's sharing and lock violations
/// (os errors 32 and 33) on Windows only, where std gives them no kind.
fn refused(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
        || (cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33)))
}

pub(super) fn markdown_text(path: &Path) -> String {
    fn read_markdown(path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match fs::read_to_string(path) {
                Ok(text) => return text,
                Err(_error) if Instant::now() < deadline => {
                    // Windows briefly locks a task file while the parallel
                    // workers atomically replace it. The assertion observes
                    // the settled workspace, not that transient lock.
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("read markdown '{}': {error}", path.display()),
            }
        }
    }

    fn visit(path: &Path, text: &mut String) {
        for entry in fs::read_dir(path).expect("read workspace") {
            let path = entry.expect("workspace entry").path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("runtime") {
                    visit(&path, text);
                }
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
                text.push_str(&read_markdown(&path));
            }
        }
    }

    let mut text = String::new();
    visit(path, &mut text);
    text
}

fn try_markdown_text(path: &Path) -> Result<String, String> {
    fn visit(path: &Path, text: &mut String) -> Result<(), String> {
        let entries = fs::read_dir(path)
            .map_err(|error| format!("read workspace '{}': {error}", path.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("read workspace entry '{}': {error}", path.display()))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| {
                format!("inspect workspace entry '{}': {error}", path.display())
            })?;
            if file_type.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) != Some("runtime") {
                    visit(&path, text)?;
                }
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
                text.push_str(
                    &fs::read_to_string(&path)
                        .map_err(|error| format!("read markdown '{}': {error}", path.display()))?,
                );
            }
        }
        Ok(())
    }

    let mut text = String::new();
    visit(path, &mut text)?;
    Ok(text)
}

/// Observe every durable, state-qualified provider wait while the run remains
/// live. The patience bounds a stall, not the batch: it starts again whenever
/// one more wait is durable, because a loaded runner persists a burst of
/// refusals one at a time and slowly, and only a run that persists nothing more
/// for a whole patience is stuck. Transient Markdown locks consume the same
/// deadline rather than starting a new per-file clock. §FS-rhei-run.3.3
pub(super) fn wait_for_provider_waits(
    path: &Path,
    run: &mut RunningChild,
    expected: usize,
) -> String {
    let mut deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
    let mut most_observed = 0;

    loop {
        let (observation, read_error) = match try_markdown_text(path) {
            Ok(text) => {
                let observed = text.matches("nextAttemptAt:").count();
                if observed > most_observed {
                    most_observed = observed;
                    deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
                }
                (Some((observed, text)), None)
            }
            Err(error) => (None, Some(error)),
        };

        if let Some(status) = run.child().try_wait().expect("inspect run status") {
            panic!(
                "recognized provider limits followed the ordinary failure path: the {expected}-worker run exited {status} instead of parking; {}",
                run.output()
            );
        }
        if let Some((_, text)) = observation.filter(|(observed, _)| *observed == expected) {
            return text;
        }

        if Instant::now() >= deadline {
            let read_detail = read_error
                .as_deref()
                .map(|error| format!("; last Markdown read error: {error}"))
                .unwrap_or_default();
            panic!(
                "the run stayed live but persisted only {most_observed} of {expected} provider waits, none more within {PROVIDER_WAIT_PATIENCE:?}{read_detail}; {}",
                run.output()
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Observe the run journal only once it holds every parked invocation's
/// `end@<state> … outcome=provider_limited` line. A parked invocation's durable
/// wait and its retained spawn record are both observable before the release
/// carrying that line is emitted, so an observer that has seen either must wait
/// for the line itself rather than read the journal once. The patience starts
/// again with every new line, as for the waits. §FS-rhei-run-tui.1.7
pub(super) fn wait_for_parked_journal_lines(
    path: &Path,
    run: &mut RunningChild,
    expected: usize,
) -> String {
    const OUTCOME: &str = "outcome=provider_limited";

    let journal = path.join("runtime/transitions.log");
    let mut deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
    let mut most_observed = 0;

    loop {
        // The exit is inspected before the read, not after it: a journal read
        // that follows an observed exit already holds everything the run will
        // ever write, so a complete journal is never reported as a dead run.
        let exited = run.child().try_wait().expect("inspect run status");
        let (observation, read_error) = match fs::read_to_string(&journal) {
            Ok(text) => {
                let observed = text.matches(OUTCOME).count();
                if observed > most_observed {
                    most_observed = observed;
                    deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
                }
                (Some((observed, text)), None)
            }
            Err(error) => {
                (None, Some(format!("read run journal '{}': {error}", journal.display())))
            }
        };

        if let Some((_, text)) = observation.filter(|(observed, _)| *observed >= expected) {
            return text;
        }

        let read_detail = read_error
            .as_deref()
            .map(|error| format!("; last journal read error: {error}"))
            .unwrap_or_default();
        if let Some(status) = exited {
            panic!(
                "the run exited {status} having written only {most_observed} of {expected} `{OUTCOME}` journal lines{read_detail}; {}",
                run.output()
            );
        }
        if Instant::now() >= deadline {
            panic!(
                "the run stayed live but wrote only {most_observed} of {expected} `{OUTCOME}` journal lines, none more within {PROVIDER_WAIT_PATIENCE:?}{read_detail}; {}",
                run.output()
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn expire_provider_deadlines(path: &Path) {
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
        let original = waiting_out_refusals("read markdown", || fs::read_to_string(&path));
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
            let staged = path.with_extension("md.tmp");
            fs::write(&staged, format!("{rewritten}\n")).expect("expire provider deadline");
            waiting_out_refusals("replace provider deadline atomically", || {
                fs::rename(&staged, &path)
            });
        }
    }
}

pub(super) const SINGLE_TASK: &[(&str, &str)] =
    &[("01.md", "### Task 1: Provider worker\n**State:** working\n")];

pub(super) const SIMPLE_MACHINE: &str = r#"name: provider-controls
version: 1
states:
  working:
    initial: true
    target: codex:openai:alpha
    attempts: 1
  completed:
    final: true
transitions:
  - from: working
    to: completed
"#;

pub(super) struct ProviderFixture {
    pub dir: TestDir,
    pub root: PathBuf,
    pub machine: PathBuf,
    pub agent: PathBuf,
}

impl ProviderFixture {
    pub fn new(name: &str, tasks: &[(&str, &str)], machine_text: &str, body: &str) -> Self {
        let (dir, root, machine) = create_workspace(name, "# Rhei: Provider controls\n", tasks);
        let body = format!(
            r#"def await_marker(path):
    until = time.monotonic() + {marker_patience}
    while not path.exists():
        if time.monotonic() >= until:
            raise RuntimeError('missing fixture marker: ' + str(path))
        time.sleep(.025)

{body}"#,
            marker_patience = MARKER_PATIENCE.as_secs()
        );
        let agent = write_python_agent(&dir, "agent.py", &body);
        let settings = root.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings).unwrap();
        let profile = serde_json::json!({
            "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&agent)).unwrap(),
            "stdin_prompt": true,
            "timeout": format!("{}s", FIXTURE_TIMEOUT.as_secs())
        });
        fs::write(
            settings.join("settings.json"),
            serde_json::json!({
                "agents": {"codex": profile, "mock": profile}
            })
            .to_string(),
        )
        .unwrap();
        fs::write(&machine, machine_text).unwrap();
        Self { dir, root, machine, agent }
    }

    /// Start the run; its stdout events and its stderr share `run.jsonl`.
    pub fn start(&self, extra: &[&str]) -> RunningChild {
        let output = fs::File::create(self.root.join("run.jsonl")).unwrap();
        let mut command = rhei_command(self.root.join(".home"));
        command
            .arg("--state-machine")
            .arg(&self.machine)
            .arg("run")
            .arg(&self.root)
            .args(["--no-tui", "--no-dashboard", "--json"])
            .args(extra)
            .stdout(output.try_clone().unwrap())
            .stderr(output);
        let child = command.spawn().expect("start controlled provider run");
        RunningChild::new(child, self.root.join("run.jsonl"))
    }

    pub fn output(&self) -> String {
        fs::read_to_string(self.root.join("run.jsonl")).unwrap_or_default()
    }

    pub fn events(&self) -> Vec<serde_json::Value> {
        self.output().lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
    }

    pub fn parked(&self, run: &mut RunningChild) {
        wait_for("provider-limited release event", run, || {
            self.events().iter().any(|event| event["outcome"] == "provider_limited")
        });
        assert!(run.child().try_wait().unwrap().is_none(), "{}", self.output());
        assert!(markdown_text(&self.root).contains("providerLimits:"));
    }

    pub fn finish(&self, run: &mut RunningChild) -> ExitStatus {
        run.wait_for_exit("controlled run completion")
    }

    pub fn finish_success(&self, run: &mut RunningChild) {
        assert!(self.finish(run).success(), "{}", self.output());
        assert_all_tasks_in_state(&self.root, &self.machine, "completed");
    }

    pub fn records(&self) -> Vec<serde_json::Value> {
        fs::read_dir(self.root.join("runtime/spawns"))
            .unwrap()
            .map(|entry| {
                serde_json::from_str(&fs::read_to_string(entry.unwrap().path()).unwrap()).unwrap()
            })
            .collect()
    }
}

pub(super) fn epoch_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

pub(super) fn utc_at(epoch: u64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp(epoch as i64).unwrap();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        t.year(),
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

pub(super) fn limit_record(deadline: u64) -> serde_json::Value {
    serde_json::json!({
        "identity": {"agent": "codex", "provider": "openai"},
        "signal": LIMIT_SIGNAL,
        "observedAt": utc_at(epoch_now()),
        "nextAttemptAt": utc_at(deadline)
    })
}

pub(super) fn metadata(root: &Path) -> serde_json::Value {
    let text = waiting_out_refusals("read index metadata", || {
        fs::read_to_string(root.join("index.rhei.md"))
    });
    serde_json::to_value(
        rhei_core::parser::parse_workspace_index(&text).unwrap().metadata.unwrap_or_default(),
    )
    .unwrap()
}

pub(super) fn yaml_metadata(value: &serde_json::Value) -> String {
    let mut yaml = serde_yaml::to_value(value).unwrap();
    // Local numeric task ids are numeric YAML keys in the runtime contract.
    // JSON is convenient for edits, but would quote those keys on its own.
    if let Some(tasks) = yaml["metadata"]["tasks"].as_mapping_mut() {
        for (key, value) in std::mem::take(tasks) {
            let key = key
                .as_str()
                .and_then(|key| key.parse::<u64>().ok())
                .map(|key| serde_yaml::Value::Number(key.into()))
                .unwrap_or(key);
            tasks.insert(key, value);
        }
    }
    serde_yaml::to_string(&yaml).unwrap()
}

/// Edit only an idle scheduler's durable clock inputs, using an atomic file
/// replacement so its next scan cannot observe a partial record. §FS-rhei-run.3.3
pub(super) fn edit_metadata(root: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = root.join("index.rhei.md");
    let raw = waiting_out_refusals("read index metadata", || fs::read_to_string(&path));
    let mut value = metadata(root);
    edit(&mut value);
    let (heading, tail) = match raw.split_once("\n---\n") {
        Some((heading, rest)) => (heading, rest.split_once("\n---").unwrap().1),
        None => (raw.trim_end(), "\n"),
    };
    let staged = root.join("clock-edit.md.tmp");
    fs::write(&staged, format!("{heading}\n---\n{}---{tail}", yaml_metadata(&value))).unwrap();
    waiting_out_refusals("replace index metadata atomically", || fs::rename(&staged, &path));
}

pub(super) fn assert_stays_parked(fixture: &ProviderFixture, run: &mut RunningChild) {
    thread::sleep(Duration::from_millis(1200));
    assert!(run.child().try_wait().unwrap().is_none(), "{}", fixture.output());
    assert!(!fixture.events().iter().any(|event| event["event"] == "run_finished"));
}
