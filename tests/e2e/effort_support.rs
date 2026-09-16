// Shared black-box fixtures for state effort. §FS-rhei-states.1.2

use std::fs;
use std::path::{Path, PathBuf};

use super::{write_fixture_file, TestDir};

pub const PLAN: &str = r#"# Rhei: Effort contract

## Tasks

### Task 1: Exercise effort
**State:** work
"#;

pub fn fixture_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_effort-fixture"))
}

pub fn effort_command(args: &[&str]) -> String {
    let mut command = vec![fixture_binary().display().to_string()];
    command.extend(args.iter().map(|arg| (*arg).to_string()));
    serde_json::to_string(&command).expect("fixture command JSON")
}

pub fn write_settings(root: &Path, body: &str) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).expect("create settings directory");
    write_fixture_file(&dir, "settings.json", body);
}

pub fn record_args(root: &Path, state: &str, identity: &str) -> Vec<String> {
    let path = record_path(root, state, identity);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
        .lines()
        .filter_map(|line| line.strip_prefix("arg=").map(str::to_string))
        .collect()
}

pub fn record_path(root: &Path, state: &str, identity: &str) -> PathBuf {
    root.join("runtime/effort-argv").join(format!("{state}-{identity}.txt"))
}

pub fn records_for_state(root: &Path, state: &str) -> Vec<Vec<String>> {
    let dir = root.join("runtime/effort-argv");
    let mut records = fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&format!("{state}-")))
        .map(|entry| {
            fs::read_to_string(entry.path())
                .expect("read argv record")
                .lines()
                .filter_map(|line| line.strip_prefix("arg=").map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    records.sort();
    records
}

pub fn write_case(dir: &TestDir, plan: &str, machine: &str) -> (PathBuf, PathBuf) {
    (write_fixture_file(dir, "plan.rhei.md", plan), write_fixture_file(dir, "states.yaml", machine))
}

pub fn one_state_machine(state_body: &str) -> String {
    format!(
        r#"name: effort-contract
version: 1
states:
  work:
{state_body}
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#
    )
}
