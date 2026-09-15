//! The `Consumes` advisory at validation and run boundaries, plus the preserved
//! filesystem reachability that makes the advisory necessary.

use std::fs;

use super::*;

const ADVISORY: &str = "warning: **Consumes:** declares export data-flow for prompt injection, not filesystem visibility. Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule participants concurrently and brief them not to inspect sibling exports; neither measure enforces blindness once an export exists.";

const TWO_CONSUMERS: &str = r#"# Rhei: Consumes Advisory

## Tasks

### Task 1: Publish
**State:** completed
**Provides:** evidence

### Task 2: First consumer
**State:** draft
**Prior:** Task 1
**Consumes:** 1:evidence

### Task 3: Second consumer
**State:** draft
**Prior:** Task 1
**Consumes:** 1:evidence
"#;

const EXISTING_PRIOR_WARNING: &str = "warning: Task plan.2 is 'completed' but its prerequisites are unsatisfied: Task plan.1 (draft). The plan contradicts its own **Prior:** dependencies.";

const EXISTING_WARNING_PLAN: &str = r#"# Rhei: Existing Warning

## Tasks

### Task 1: Unfinished prior
**State:** draft

### Task 2: Finished too early
**State:** completed
**Prior:** Task 1
"#;

fn count(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

/// A graph-level condition produces one advisory, after success, rather than
/// one warning per consumer.
// §FS-rhei-validate.4 §FS-rhei-validate.6
#[test]
fn validate_reports_the_consumes_advisory_once_after_success() {
    let (_dir, plan, machine) = setup_single_file("validate-consumes-advisory", TWO_CONSUMERS);
    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_eq!(
        result.stdout,
        format!("Validation succeeded\n{ADVISORY}\n"),
        "the exact graph-level advisory must follow successful validation"
    );
    assert!(result.stderr.is_empty(), "validation writes its success report to stdout");
}

/// A graph without `Consumes` keeps its successful output byte for byte.
// §FS-rhei-validate.4 §FS-rhei-validate.6
#[test]
fn validate_without_consumes_preserves_the_existing_success_output() {
    let (_dir, plan, machine) = setup_single_file("validate-without-consumes", LINEAR_PLAN);
    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_eq!(result.stdout, "Validation succeeded\n");
    assert!(result.stderr.is_empty());
}

/// Adding the new advisory does not rewrite or duplicate an existing warning.
// §FS-rhei-validate.4 §FS-rhei-validate.6
#[test]
fn consumes_change_preserves_existing_validation_warning_wording_and_frequency() {
    let (_dir, plan, machine) =
        setup_single_file("validate-existing-warning", EXISTING_WARNING_PLAN);
    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_eq!(result.stdout, format!("Validation succeeded\n{EXISTING_PRIOR_WARNING}\n"));
}

/// Existing validation warnings take the same event path as the new advisory;
/// JSON receives one warn record and stderr stays empty.
// §FS-rhei-run.3 §FS-rhei-run-json.1
#[test]
fn consumes_change_routes_an_existing_warning_once_through_json() {
    let (dir, plan, machine) = setup_single_file("run-existing-warning", EXISTING_WARNING_PLAN);
    let result = run_cli("run", &plan, &machine, &["--json", "--dry-run"]);
    assert_success(&result);
    assert!(result.stderr.is_empty(), "warning bypassed JSON frontend:\n{}", result.stderr);
    let records: Vec<serde_json::Value> = result
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout must stay pure JSONL"))
        .collect();
    let warnings: Vec<_> = records
        .iter()
        .filter(|record| record["event"] == "message" && record["level"] == "warn")
        .collect();
    assert_eq!(warnings.len(), 1, "records: {records:#?}");
    assert_eq!(warnings[0]["text"], EXISTING_PRIOR_WARNING);
    assert!(!dir.join("runtime").exists());
}

/// The plain dry-run frontend writes the initial warning to stderr without
/// changing success or creating runtime state.
// §FS-rhei-run.3 §FS-rhei-run.4
#[test]
fn run_plain_dry_run_routes_the_consumes_advisory_once_to_stderr() {
    let (plain_dir, plain_plan, plain_machine) =
        setup_single_file("run-consumes-advisory-plain", TWO_CONSUMERS);
    let plain = run_cli("run", &plain_plan, &plain_machine, &["--no-tui", "--dry-run"]);
    assert_success(&plain);
    assert_eq!(count(&plain.stderr, ADVISORY), 1, "got stderr:\n{}", plain.stderr);
    assert!(!plain.stdout.contains(ADVISORY), "plain warnings belong on stderr");
    assert!(!plain_dir.join("runtime").exists(), "plain dry run must not create runtime files");
}

/// The JSON dry-run frontend retains the initial warning as one ordered record
/// while keeping stdout parseable and the filesystem unchanged.
// §FS-rhei-run.3 §FS-rhei-run.4 §FS-rhei-run-json.1
#[test]
fn run_json_dry_run_routes_the_consumes_advisory_once_after_run_started() {
    let (json_dir, json_plan, json_machine) =
        setup_single_file("run-consumes-advisory-json", TWO_CONSUMERS);
    let json = run_cli("run", &json_plan, &json_machine, &["--json", "--dry-run"]);
    assert_success(&json);
    assert!(json.stderr.is_empty(), "JSON warnings are records, not stderr:\n{}", json.stderr);
    let records: Vec<serde_json::Value> = json
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout must stay pure JSONL"))
        .collect();
    let warning_indexes: Vec<_> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record["event"] == "message" && record["level"] == "warn" && record["text"] == ADVISORY
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(warning_indexes.len(), 1, "records: {records:#?}");
    let started = records.iter().position(|record| record["event"] == "run_started").unwrap();
    let scheduled = records.iter().position(|record| record["event"] == "pass_started").unwrap();
    assert!(
        started < warning_indexes[0] && warning_indexes[0] < scheduled,
        "records: {records:#?}"
    );
    assert!(!json_dir.join("runtime").exists(), "JSON dry run must not create runtime files");
}

/// The warning documents existing reachability; it must not turn `Consumes`
/// into isolation. This consumer intentionally reads an undeclared sibling.
// §FS-rhei-plan-language.3.12 §FS-rhei-memory.1.1
#[test]
fn consumes_does_not_hide_an_undeclared_sibling_export_from_a_worker() {
    let dir = unique_temp_dir("undeclared-sibling-export");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Reachable Exports

## Tasks

### Task 1: Publish a secret
**State:** work
**Provides:** secret

### Task 2: Read the sibling directly
**State:** work
**Prior:** Task 1
"#,
    );
    let agent = write_python_agent(
        &dir,
        "sibling-reader.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
if task.endswith('.1'):
    write(root / 'runtime' / 'exports' / task / 'secret.md', 'VISIBLE SIBLING EXPORT\n')
else:
    producer = task.rsplit('.', 1)[0] + '.1'
    sibling = root / 'runtime' / 'exports' / producer / 'secret.md'
    write(root / 'runtime' / 'undeclared-read.txt', sibling.read_text(encoding='utf-8'))
result('worker finished\n')
"#,
    );
    let agent_command =
        serde_json::to_string(&vec![python_command().to_string(), agent.display().to_string()])
            .expect("serialize agent command");
    fs::create_dir_all(dir.join(".agent-grounds/rhei")).expect("settings directory");
    fs::write(
        dir.join(".agent-grounds/rhei/settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{ "mock": {{ "command": {agent_command}, "stdin_prompt": true }} }}
}}"#
        ),
    )
    .expect("settings");
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: sibling-reader
version: 1
states:
  work:
    initial: true
    agent: mock
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#,
    );

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_eq!(
        fs::read_to_string(dir.join("runtime/undeclared-read.txt")).expect("consumer evidence"),
        "VISIBLE SIBLING EXPORT\n"
    );
}
