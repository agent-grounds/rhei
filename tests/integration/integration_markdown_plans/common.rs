use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rhei_core::ast::TaskId;
use rhei_core::parse;
use rhei_core::parser::parse_workspace_index;
use rhei_core::workspace;
use rhei_cli::rhei_output::{to_github_markdown, to_json_value, ProgressReportOutput};
use rhei_cli::rhei_validator::{validate_with_machine, StateMachine};
use serde_yaml::Value as YamlValue;

// The same guard and fixture helpers the e2e harness uses; `include!` rather
// than `mod`, because this harness is one flat module assembled by `include!`.
include!("../../support/test_dir.rs");
include!("../../support/unique_dir_name.rs");
include!("../../support/python_fixture.rs");

#[allow(dead_code)]
#[path = "../../../crates/rhei-core/tests/fixtures.rs"]
mod fixtures;

// `binaries.rs` opens with its own `#![allow(dead_code)]`, valid only at the
// top of a module — `mod`, not `include!`, so that attribute lands on a module
// of its own rather than splicing into this file's.
#[path = "../../support/binaries.rs"]
mod binaries;
use binaries::rhei_binary;

/// Every `rhei` this harness spawns, with **both** state locations pinned into
/// a directory of its own.
///
/// The run registry is machine-wide, and every non-dry run publishes an entry
/// into it. Unpinned, one `cargo test` wrote a couple of hundred entries into
/// the developer's real `~/.local/state/rhei/runs` — and `HOME` alone is not
/// enough, because the registry prefers `XDG_STATE_HOME` wherever the
/// developer's environment happens to set it.

// §FS-rhei-run-headless.2
fn rhei_command() -> Command {
    static HARNESS_HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let home = HARNESS_HOME.get_or_init(|| {
        // Deliberately not a `TestDir`: it lives for the whole harness, and a
        // guard in a `OnceLock` is never dropped. One name per process,
        // emptied on the way in, so a run leaves one directory behind rather
        // than one more every time — and only its own. The pid is not
        // decoration: `cargo test --workspace --all-targets` runs several test
        // binaries at once, and under a shared name the second one to start
        // deletes the first one's home out from under it.
        let home = std::env::temp_dir().join(format!("rhei-harness-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("state")).expect("isolated state directory");
        home
    });
    let mut cmd = Command::new(rhei_binary());
    cmd.env("HOME", home);
    cmd.env("XDG_STATE_HOME", home.join("state"));
    cmd
}

fn unique_temp_dir(prefix: &str) -> TestDir {
    // §REQ-cross-platform.6
    TestDir::create(std::env::temp_dir().join(unique_dir_name(&format!("rhei-{prefix}"))))
}

fn write_fixture_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("fixture file should be written");
    path
}

/// A `cli:` callback that runs one line of Python, spelled as a YAML scalar.
///
/// A callback is a command line for the platform's own shell, and the two
/// shells share almost no vocabulary — `printf` and `cat` are `sh`, not `cmd`.
/// The code goes inside one pair of double quotes and quotes its own strings
/// with `'…'`, which both shells hand through unchanged, and `serde_json`
/// spells the result as a YAML double-quoted scalar.
// §FS-rhei-programs.1.1
fn python_callback_yaml(code: &str) -> String {
    serde_json::to_string(&format!("cli:{} -c \"{code}\"", python_command()))
        .expect("callback should serialize")
}

fn yaml_key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

fn visit_count_from_metadata(
    metadata: Option<&rhei_core::ast::Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<u64> {
    let metadata = metadata?;
    let metadata_section = metadata.get(yaml_key("metadata"))?.as_mapping()?;
    let tasks = metadata_section.get(yaml_key("tasks"))?.as_mapping()?;
    let task_key = if let Some(n) = task_id.as_number() {
        serde_yaml::to_value(n).ok()?
    } else if let Some(name) = task_id.as_named() {
        yaml_key(name)
    } else {
        // Dotted ids are serialized as their dotted string form.
        yaml_key(&task_id.to_string())
    };
    let task = tasks.get(task_key)?.as_mapping()?;
    let state_visits = task.get(yaml_key("stateVisits"))?.as_mapping()?;
    state_visits.get(yaml_key(state_name))?.as_u64()
}

const CLI_VALID_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Task 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

#### Task 1.2: Record rollback contract
**State:** completed
```yaml
rollback:
  enabled: true
```

### Task 2: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Task 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Task 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task 2

#### Task 3.1: Dry run in staging
**State:** pending
Run the bot in dry-run mode against staging.
"#;

// The first parse error the parser should surface is the malformed `### Tak 3:`
// heading at line 20 (unknown node kind). Earlier tasks are intentionally
// well-formed so this regression test can confirm that the malformed top-level
// heading is reported before any later child-id extension concerns.
const CLI_PRIMARY_ERROR_REGRESSION_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Task 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

### Task 2: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Task 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Tak 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task 2

#### Task 3.1: Dry run in staging
**State:** pending
Run the bot in dry-run mode against staging.
"#;

struct CliRun {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_validate(plan: &str, machine: &str, prefix: &str) -> CliRun {
    let temp_dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&temp_dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&temp_dir, "states.yaml", machine);

    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    let result = CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };


    result
}

fn run_cli_without_args() -> CliRun {
    let output =
        rhei_command().output().expect("rhei command should run");

    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn normalize_for_assertions(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_contains_in_order(haystack: &str, fragments: &[&str], context: &str, rendered: &str) {
    let mut search_start = 0usize;

    for fragment in fragments {
        let Some(relative_index) = haystack[search_start..].find(fragment) else {
            panic!("expected {context} fragment {:?} in order, got:\n{}", fragment, rendered);
        };
        search_start += relative_index + fragment.len();
    }
}

fn assert_parse_failure(
    result: &CliRun,
    parser_message_fragments: &[&str],
    line_hint: Option<&str>,
    excerpt: Option<&str>,
    unrelated_messages: &[&str],
) {
    let normalized_stderr = normalize_for_assertions(&result.stderr);

    assert!(
        !result.status.success(),
        "expected parse failure\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        normalized_stderr.contains("PARSE ERROR"),
        "expected Elm-style parse header in stderr, got:\n{}",
        result.stderr
    );
    assert_contains_in_order(
        &normalized_stderr,
        parser_message_fragments,
        "parser message",
        &result.stderr,
    );

    if let Some(line_hint) = line_hint {
        assert!(
            normalized_stderr.contains(&normalize_for_assertions(line_hint)),
            "expected line hint {:?}, got:\n{}",
            line_hint,
            result.stderr
        );
    }

    if let Some(excerpt) = excerpt {
        assert!(
            normalized_stderr.contains(&normalize_for_assertions(excerpt)),
            "expected source excerpt {:?}, got:\n{}",
            excerpt,
            result.stderr
        );
    }

    assert!(
        !normalized_stderr.contains("VALIDATION ERROR"),
        "parse failures should not fall through to validation output, got:\n{}",
        result.stderr
    );

    for unrelated in unrelated_messages {
        assert!(
            !normalized_stderr.contains(&normalize_for_assertions(unrelated)),
            "unexpected unrelated validator noise {:?} in stderr:\n{}",
            unrelated,
            result.stderr
        );
    }
}

// Directory-workspace and multi-rhei project fixtures, shared by the
// `workspace_validation*` siblings. §AR-source-file-size.3

const WORKSPACE_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

/// A machine that declares an artifact contract, so a reset can be checked
/// against the paths a ticket actually writes. §FS-rhei-states.6
const ARTIFACT_CONTRACT_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
    outputs:
      - name: notes
        path: runtime/notes/{task_id}.md
  in-progress:
    description: Task currently being worked on
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

/// Helper: create a directory workspace with the given index content and a set
/// of task files. Returns (temp_dir, workspace_root, machine_path); the
/// workspace lives inside the temp directory, so the first element is what has
/// to stay bound for the tree to outlive the setup call.
fn create_workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
    state_machine: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        let path = tasks_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create task parent dir");
        }
        fs::write(path, content).expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", state_machine);
    (dir, ws, machine_path)
}

/// A second machine with disjoint state names, so a project mixing it with
/// `workspace-test-machine` proves each ticket is judged under the machine of
/// the rhei that owns it. §DA-per-rhei-state-machines
const CHILD_FLOW_STATE_MACHINE: &str = r#"name: child-flow
version: 1
states:
  open:
    description: Ticket waiting for work
    initial: true
  done:
    description: Ticket finished
    final: true
transitions:
  - from: open
    to: done
"#;

/// A project machine whose initial state carries autonomous agent work, so
/// `rhei run` takes the orchestrated (agent-mode) scheduling path.
const AGENT_WORK_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
    agent: fake
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: completed
"#;

fn create_panta_project(
    prefix: &str,
    manifest: &str,
    files: &[(&str, &str)],
    state_machine: &str,
) -> TestDir {
    let dir = unique_temp_dir(prefix);
    fs::write(dir.join("index.panta.md"), manifest).expect("write panta manifest");
    for (name, content) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create panta parent dir");
        }
        fs::write(path, content).expect("write panta file");
    }
    fs::write(dir.join("states.yaml"), state_machine).expect("write panta states");
    dir
}
