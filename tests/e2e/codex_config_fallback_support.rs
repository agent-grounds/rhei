//! A fake Codex that reports a configuration fallback the way a managed
//! workspace's Codex does, then exits 0. §FS-rhei-configuration-fallbacks

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// The message the issue's run printed, verbatim.
pub(super) const PERMISSION_FALLBACK: &str = "Configured value for `permission_profile` is disallowed by requirements; falling back to required value Managed { file_system: Restricted { entries: [FileSystemSandboxEntry { path: Special { value: Root }, access: Read }], glob_scan_max_depth: None }, network: Restricted }. Details: invalid value for `sandbox_mode`: `DangerFullAccess` is not in the allowed set [ReadOnly, WorkspaceWrite] (set by enterprise-managed requirements <workspace>)";

/// The same fallback for another field.
pub(super) const APPROVAL_FALLBACK: &str = "Configured value for `approval_policy` is disallowed by requirements; falling back to required value OnRequest.";

/// What every fallback message begins with, whatever the field.
pub(super) const FALLBACK_LEAD: &str =
    "Configured value for `permission_profile` is disallowed by requirements; falling back";

/// What the report and the console put in front of a fallback.
pub(super) const CLAUSE: &str = "Codex configuration fallback: ";

/// `item.completed` carrying an error item, as in the issue.
pub(super) fn item_error(message: &str) -> String {
    serde_json::json!({"type": "item.completed", "item": {"id": "item_2", "type": "error", "message": message}})
        .to_string()
}

/// A top-level error event.
pub(super) fn top_error(message: &str) -> String {
    serde_json::json!({"type": "error", "message": message}).to_string()
}

const SHIM: &str = r#"
# A family profile's version probe asks `<command> --version`; answer it as
# the CLI would, with no attempt counted. §FS-rhei-evidence.7
if sys.argv[-1:] == ['--version']:
    print('fake-codex 0.0.0')
    raise SystemExit(0)
attempt = int(pathlib.Path(COUNTER).read_text()) + 1 if pathlib.Path(COUNTER).exists() else 1
write(COUNTER, str(attempt))
if attempt <= EMIT_ATTEMPTS:
    for line in EVENTS:
        sys.stdout.write(line + '\n')
    sys.stdout.flush()
if WRITE:
    root = pathlib.Path(env('RHEI_ROOT'))
    write(root / 'runtime' / 'exports' / env('RHEI_TASK_ID') / 'working.md', 'finding\n')
    result('Wrote the finding.\n')
"#;

const HELPER: &str = r#"
# A family profile's version probe asks `<command> --version`; answer it as
# the CLI would, with no attempt counted. §FS-rhei-evidence.7
if sys.argv[-1:] == ['--version']:
    print('fake-codex 0.0.0')
    raise SystemExit(0)
root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'exports' / env('RHEI_TASK_ID') / 'working.md', 'helper\n')
result('Independent helper completed.\n')
"#;

/// What the fake agent does.
pub(super) struct Script<'a> {
    /// Lines printed to stdout, as they are.
    pub events: &'a [String],
    /// Whether it then writes its artifact and result.
    pub writes: bool,
    /// How many attempts print `events`; later ones print nothing.
    pub emit_attempts: u32,
    /// The family its profile declares.
    pub family: &'a str,
    /// The state's `attempts`.
    pub attempts: u32,
}

impl Default for Script<'_> {
    fn default() -> Self {
        Script { events: &[], writes: false, emit_attempts: u32::MAX, family: "codex", attempts: 1 }
    }
}

pub(super) struct FallbackFixture {
    pub dir: TestDir,
    pub plan: PathBuf,
    pub machine: PathBuf,
    pub home: PathBuf,
    pub parallel: bool,
}

/// One plan whose Task 1 runs the fake agent; with `parallel`, a Task 2 runs a
/// helper beside it in the same concurrent state.
pub(super) fn fallback_fixture(name: &str, script: &Script, parallel: bool) -> FallbackFixture {
    let dir = unique_temp_dir(name);
    let plan = dir.join("plan");
    fs::create_dir_all(plan.join("tasks")).unwrap();
    fs::write(plan.join("index.rhei.md"), "# Rhei: Codex fallback\n").unwrap();
    fs::write(plan.join("tasks/01.md"), "### Task 1: Review the change\n**State:** working\n")
        .unwrap();
    let counter = serde_json::to_string(&dir.join("attempts").display().to_string()).unwrap();
    let python_bool = if script.writes { "True" } else { "False" };
    let body = format!(
        "EVENTS = {}\nWRITE = {python_bool}\nCOUNTER = {counter}\nEMIT_ATTEMPTS = {}\n{SHIM}",
        serde_json::to_string(script.events).unwrap(),
        script.emit_attempts.min(1_000)
    );
    let agent = write_python_agent(&dir, "fake_codex.py", &body);
    let command =
        |path: &Path| serde_json::from_str::<serde_json::Value>(&fixture_command(path)).unwrap();
    let mut agents = serde_json::json!({
        "cdx": {"family": script.family, "command": command(&agent), "timeout": "60s"},
    });
    if parallel {
        let helper = write_python_agent(&dir, "helper.py", HELPER);
        agents["helper"] =
            serde_json::json!({"family": "codex", "command": command(&helper), "timeout": "60s"});
        fs::write(
            plan.join("tasks/02.md"),
            "### Task 2: Independent helper\n**State:** working\n**Target:** helper:openai:fixture\n",
        )
        .unwrap();
    }
    let settings = plan.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).unwrap();
    fs::write(settings.join("settings.json"), serde_json::json!({"agents": agents}).to_string())
        .unwrap();
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            "name: codex-fallback\nversion: 1\nstates:\n  working:\n    initial: true\n    \
             target: cdx:openai:fixture\n    attempts: {}\n    concurrent: {parallel}\n    \
             agent_timeout: 60s\n    outputs:\n      - name: finding\n        \
             path: runtime/exports/{{task_id}}/working.md\n  completed:\n    final: true\n\
             transitions:\n  - from: working\n    to: completed\n",
            script.attempts
        ),
    );
    let home = dir.join("home");
    FallbackFixture { dir, plan, machine, home, parallel }
}

impl FallbackFixture {
    fn command(&self, extra: &[&str]) -> CliRun {
        let output = rhei_command(&self.home)
            .current_dir(&self.dir)
            .arg("--state-machine")
            .arg(&self.machine)
            .arg("run")
            .arg(&self.plan)
            .args([
                "--no-tui",
                "--no-dashboard",
                "--parallel",
                if self.parallel { "2" } else { "1" },
            ])
            .args(extra)
            .output()
            .expect("rhei run should start");
        CliRun::from(&output)
    }

    /// A plain run, whose console is the line-oriented one.
    pub fn run_plain(&self) -> CliRun {
        self.command(&[])
    }

    /// A `--json` run, and its records.
    pub fn run_json(&self) -> (CliRun, Vec<serde_json::Value>) {
        let run = self.command(&["--json"]);
        let records =
            run.stdout.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
        (run, records)
    }

    pub fn report(&self) -> String {
        fs::read_to_string(self.plan.join("runtime/run-report.md")).unwrap_or_default()
    }

    /// The id the plan gives Task 1.
    pub fn task_id(&self) -> String {
        render_json(&self.plan, &self.machine)["tasks"][0]["id"]["path"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// The spawn records the run wrote for the fake Codex.
    pub fn spawn_records(&self) -> Vec<serde_json::Value> {
        let Ok(entries) = fs::read_dir(self.plan.join("runtime/spawns")) else { return Vec::new() };
        entries
            .map(|entry| {
                serde_json::from_str(&fs::read_to_string(entry.unwrap().path()).unwrap()).unwrap()
            })
            .filter(|record: &serde_json::Value| record["worker"] == "cdx")
            .collect()
    }
}

/// One `## <name>` section of a report, up to the next `## ` heading.
pub(super) fn section(report: &str, name: &str) -> String {
    let heading = format!("## {name}\n");
    let Some(start) = report.find(&heading) else { return String::new() };
    let body = &report[start + heading.len()..];
    let end = body.find("\n## ").unwrap_or(body.len());
    body[..end].to_string()
}

/// The `slot_released` record for `task`.
pub(super) fn released<'a>(records: &'a [serde_json::Value], task: &str) -> &'a serde_json::Value {
    records
        .iter()
        .find(|record| record["event"] == "slot_released" && record["task"] == task)
        .unwrap_or_else(|| panic!("no slot_released for {task} in {records:#?}"))
}
