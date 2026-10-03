//! The workspaces the unrecorded-spawn scenarios run in.
//!
//! Every spawn record, log and accounting record in them is written by a real
//! `rhei run`. Two profiles run the same fake Codex: `priced` declares
//! `family: codex`, so rhei binds the Codex extractor and writes a measured,
//! priced record, and `cdx` declares no family, so rhei writes no record at
//! all. The family is the only difference between them.
//! §FS-rhei-cost-accounting.6.2.1

use std::fs;
use std::path::{Path, PathBuf};

use super::{
    assert_success, fixture_command, run_cli, unique_temp_dir, write_python_agent, TestDir,
};

/// A fake Codex: the two `--json` events its usage extractor reads.
/// §FS-rhei-cost-accounting.4
const CODEX_USAGE_AGENT: &str = r#"import json

print(json.dumps({'type': 'thread.started', 'thread_id': 'thread-rhei-410'}), flush=True)
print(json.dumps({
    'type': 'turn.completed',
    'usage': {'input_tokens': 120000, 'cached_input_tokens': 100000, 'output_tokens': 3000},
}), flush=True)
result('## Result\n\nFake agent finished.\n')
"#;

/// A program that succeeds and writes nothing. A program writes no invocation
/// record, so its spawn record must never count as a missing one.
const SILENT_PROGRAM: &str = "result('## Result\\n\\nProgram finished.\\n')\n";

/// Two tickets through `implement -> describe`. `implement` runs on `cdx`,
/// which declares no family; `describe` runs on `priced`, on the model the
/// built-in price book covers. Four agent spawns, two records.
pub const NOFAMILY_PLAN: &str = r#"# Rhei: Mixed agents

## Tasks

### Task 1: First ticket
**State:** implement

### Task 2: Second ticket
**State:** implement
"#;

pub const NOFAMILY_MACHINE: &str = r#"name: mixed-agents
version: 1
states:
  implement:
    initial: true
    description: Implemented by cdx, which declares no family
    target: cdx[high]:openai:gpt-6.1-sol
    agent_timeout: 60s
    attempts: 1
  describe:
    description: Described by priced, family codex on a priced model
    target: priced[high]:anthropic:claude-sonnet-4-6
    agent_timeout: 60s
    attempts: 1
  completed:
    final: true
    description: Done
transitions:
  - from: implement
    to: describe
  - from: describe
    to: completed
"#;

/// One ticket on `priced`. The lost-write scenario runs it, makes the
/// invocations directory read-only, and runs a second ticket appended to it.
/// Unix only, like the scenario: read-only directories are a Unix permission.
#[cfg(unix)]
pub const LOSTWRITE_PLAN: &str = r#"# Rhei: Lost write

## Tasks

### Task 1: First ticket
**State:** work
"#;

#[cfg(unix)]
pub const LOSTWRITE_SECOND_TICKET: &str = r#"
### Task 2: Second ticket
**State:** work
"#;

pub const LOSTWRITE_MACHINE: &str = r#"name: lost-write
version: 1
states:
  work:
    initial: true
    description: Worked by priced, family codex on a priced model
    target: priced[high]:anthropic:claude-sonnet-4-6
    agent_timeout: 60s
    attempts: 1
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

/// A program step, then an agent step on `priced`: every agent spawn has its
/// record, and the program spawn has none and needs none.
pub const CONTROL_PLAN: &str = r#"# Rhei: All recorded

## Tasks

### Task 1: First ticket
**State:** prepare

### Task 2: Second ticket
**State:** prepare
"#;

fn control_machine(program: &str) -> String {
    format!(
        r#"name: all-recorded
version: 1
states:
  prepare:
    initial: true
    description: A program, which writes no record
    program:
      command: {program}
    program_timeout: 30s
  work:
    description: Worked by priced, family codex on a priced model
    target: priced[high]:anthropic:claude-sonnet-4-6
    agent_timeout: 60s
    attempts: 1
  completed:
    final: true
    description: Done
transitions:
  - from: prepare
    to: work
    exit_code: 0
  - from: work
    to: completed
"#
    )
}

/// One standalone workspace: a plan, its machine, and the two profiles.
pub struct SpawnWorkspace {
    /// Held so the tree outlives the test that built it.
    pub _dir: TestDir,
    pub root: PathBuf,
    pub plan: PathBuf,
    pub machine: PathBuf,
}

impl SpawnWorkspace {
    /// Run the plan to completion with the checkout's own rhei.
    pub fn run(&self) -> super::CliRun {
        let result = run_cli("run", &self.plan, &self.machine, &["--no-tui", "--no-callbacks"]);
        assert_success(&result);
        result
    }

    /// `rhei summary <plan> --details`, which must succeed.
    pub fn summary_details(&self) -> String {
        let result = run_cli("summary", &self.plan, &self.machine, &["--details"]);
        assert_success(&result);
        result.stdout
    }

    /// `rhei cost <plan> [args]` as text, which must succeed.
    pub fn cost_text(&self, args: &[&str]) -> String {
        let result = run_cli("cost", &self.plan, &self.machine, args);
        assert_success(&result);
        result.stdout
    }

    /// `rhei cost <plan> --json [args]`, parsed.
    pub fn cost_json(&self, args: &[&str]) -> serde_json::Value {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let text = self.cost_text(&all);
        serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("cost --json parses: {err}\n{text}"))
    }

    /// The spawn records of `kind: agent`, read as fields. §FS-rhei-agents.8.4
    pub fn agent_spawns(&self) -> Vec<serde_json::Value> {
        json_files(&self.root.join("runtime/spawns"))
            .into_iter()
            .filter(|spawn| spawn["kind"].as_str() == Some("agent"))
            .collect()
    }

    /// The accounting records the runs wrote.
    pub fn records(&self) -> Vec<serde_json::Value> {
        json_files(&self.root.join("runtime/accounting/invocations"))
    }

    #[cfg(unix)]
    pub fn invocations_dir(&self) -> PathBuf {
        self.root.join("runtime/accounting/invocations")
    }

    /// Fail on the fixture, not on the behaviour, when the runs did not leave
    /// the shape the scenario is about.
    pub fn assert_shape(&self, agent_spawns: usize, records: usize) {
        let spawns = self.agent_spawns();
        let held = self.records();
        assert_eq!(spawns.len(), agent_spawns, "fixture: agent spawn records {spawns:#?}");
        assert_eq!(held.len(), records, "fixture: accounting records {held:#?}");
    }
}

pub fn json_files(dir: &Path) -> Vec<serde_json::Value> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path).expect("read json file");
            serde_json::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
        })
        .collect()
}

/// A workspace whose `cdx` profile declares no family and whose `priced`
/// profile declares `family: codex`, both over the same fake Codex.
pub fn spawn_workspace(prefix: &str, plan: &str, machine: &str) -> SpawnWorkspace {
    let dir = unique_temp_dir(prefix);
    let root = dir.to_path_buf();
    write_profiles(&root);
    let plan_path = root.join("plan.rhei.md");
    fs::write(&plan_path, plan).expect("write plan");
    let machine_path = root.join("states.yaml");
    fs::write(&machine_path, machine).expect("write machine");
    SpawnWorkspace { _dir: dir, root, plan: plan_path, machine: machine_path }
}

/// A Panta project holding one member rhei, `billing`, whose two tickets run
/// on `priced`. `plan` is the member, so every reading through it is narrowed
/// to the member; `root` is the project, the run root, where `rhei run` writes
/// every agent spawn record. §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.6.2.1
pub fn member_workspace(prefix: &str) -> SpawnWorkspace {
    let dir = unique_temp_dir(prefix);
    let root = dir.to_path_buf();
    write_profiles(&root);
    fs::write(root.join("index.panta.md"), "# Panta: Billing\n").expect("write project");
    let member = root.join("billing");
    fs::create_dir_all(member.join("tasks")).expect("create member tasks");
    fs::write(member.join("index.rhei.md"), "# Rhei: Billing\n").expect("write member");
    fs::write(
        member.join("tasks/work.md"),
        "### Task 1: First ticket\n**State:** work\n\n### Task 2: Second ticket\n**State:** work\n",
    )
    .expect("write member tasks");
    let machine_path = root.join("states.yaml");
    fs::write(&machine_path, LOSTWRITE_MACHINE).expect("write machine");
    SpawnWorkspace { _dir: dir, root, plan: member, machine: machine_path }
}

/// The two profiles, `cdx` with no family and `priced` with `family: codex`,
/// over the same fake Codex, in `root`'s settings.
fn write_profiles(root: &Path) {
    let agent = write_python_agent(root, "fake-codex.py", CODEX_USAGE_AGENT);
    let command: serde_json::Value =
        serde_json::from_str(&fixture_command(&agent)).expect("fixture command is JSON");
    let profile = |family: Option<&str>| {
        let mut profile = serde_json::json!({
            "command": command,
            "model_flag": "--model",
            "stdin_prompt": true,
            "timeout": "60s",
            "modes": { "high": ["-c", "model_reasoning_effort=\"high\""] },
        });
        if let Some(family) = family {
            profile["family"] = serde_json::json!(family);
        }
        profile
    };
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let settings = serde_json::json!({
        "agents": { "cdx": profile(None), "priced": profile(Some("codex")) }
    });
    fs::write(settings_dir.join("settings.json"), settings.to_string()).expect("write settings");
}

/// The control: a program step before every agent step.
pub fn control_workspace(prefix: &str) -> SpawnWorkspace {
    let probe = spawn_workspace(prefix, CONTROL_PLAN, "");
    let program = write_python_agent(&probe.root, "silent-program.py", SILENT_PROGRAM);
    fs::write(&probe.machine, control_machine(&fixture_command(&program)))
        .expect("write control machine");
    probe
}

/// The one line of a summary that is its coverage row.
pub fn coverage_row(summary: &str) -> &str {
    summary
        .lines()
        .find(|line| line.starts_with("| coverage |"))
        .unwrap_or_else(|| panic!("the summary has a coverage row:\n{summary}"))
}
