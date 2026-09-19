//! Controlled provider fixtures and durable wait edits. §FS-rhei-run.3.3

use super::*;
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

pub(super) const LIMIT_SIGNAL: &str =
    "You've hit your session limit · resets 10:20pm (Europe/Zurich)";

const PROVIDER_WAIT_PATIENCE: Duration = Duration::from_secs(30);

pub(super) struct RunningChild(pub(super) Option<Child>);

impl RunningChild {
    pub(super) fn child(&mut self) -> &mut Child {
        self.0.as_mut().expect("run child")
    }

    pub(super) fn stop(&mut self) {
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

pub(super) fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {what}");
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
/// live. Transient Markdown locks consume the same finite test deadline rather
/// than starting a new per-file clock. §FS-rhei-run.3.3
pub(super) fn wait_for_provider_waits(
    path: &Path,
    run: &mut RunningChild,
    expected: usize,
) -> String {
    let deadline = Instant::now() + PROVIDER_WAIT_PATIENCE;
    let mut most_observed = 0;

    loop {
        let (observation, read_error) = match try_markdown_text(path) {
            Ok(text) => {
                let observed = text.matches("nextAttemptAt:").count();
                most_observed = most_observed.max(observed);
                (Some((observed, text)), None)
            }
            Err(error) => (None, Some(error)),
        };

        if let Some(status) = run.child().try_wait().expect("inspect run status") {
            panic!(
                "recognized provider limits followed the ordinary failure path: the {expected}-worker run exited {status} instead of parking"
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
                "the run stayed live but persisted only {most_observed} of {expected} provider waits within {PROVIDER_WAIT_PATIENCE:?}{read_detail}"
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
            let staged = path.with_extension("md.tmp");
            fs::write(&staged, format!("{rewritten}\n")).expect("expire provider deadline");
            fs::rename(staged, path).expect("replace provider deadline atomically");
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
    until = time.monotonic() + 9
    while not path.exists():
        if time.monotonic() >= until:
            raise RuntimeError('missing fixture marker: ' + str(path))
        time.sleep(.025)

{body}"#
        );
        let agent = write_python_agent(&dir, "agent.py", &body);
        let settings = root.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings).unwrap();
        let profile = serde_json::json!({
            "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&agent)).unwrap(),
            "stdin_prompt": true,
            "timeout": "12s"
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
        RunningChild(Some(command.spawn().expect("start controlled provider run")))
    }

    pub fn output(&self) -> String {
        fs::read_to_string(self.root.join("run.jsonl")).unwrap_or_default()
    }

    pub fn events(&self) -> Vec<serde_json::Value> {
        self.output().lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
    }

    pub fn parked(&self, run: &mut RunningChild) {
        wait_for("provider-limited release event", || {
            assert!(run.child().try_wait().unwrap().is_none(), "{}", self.output());
            self.events().iter().any(|event| event["outcome"] == "provider_limited")
        });
        assert!(markdown_text(&self.root).contains("providerLimits:"));
    }

    pub fn finish(&self, run: &mut RunningChild) -> std::process::ExitStatus {
        let mut status = None;
        wait_for("controlled run completion", || {
            status = run.child().try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
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
    let text = fs::read_to_string(root.join("index.rhei.md")).unwrap();
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
    let raw = fs::read_to_string(&path).unwrap();
    let mut value = metadata(root);
    edit(&mut value);
    let (heading, tail) = match raw.split_once("\n---\n") {
        Some((heading, rest)) => (heading, rest.split_once("\n---").unwrap().1),
        None => (raw.trim_end(), "\n"),
    };
    let staged = root.join("clock-edit.md.tmp");
    fs::write(&staged, format!("{heading}\n---\n{}---{tail}", yaml_metadata(&value))).unwrap();
    fs::rename(staged, path).unwrap();
}

pub(super) fn assert_stays_parked(fixture: &ProviderFixture, run: &mut RunningChild) {
    thread::sleep(Duration::from_millis(1200));
    assert!(run.child().try_wait().unwrap().is_none(), "{}", fixture.output());
    assert!(!fixture.events().iter().any(|event| event["event"] == "run_finished"));
}
