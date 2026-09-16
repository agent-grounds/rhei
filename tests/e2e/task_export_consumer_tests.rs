//! A consumed export is required prompt input: all files are checked before a
//! worker starts, and a refusal is recoverable without a state rewrite.

use std::fs;

use super::terminal_result_tests::write_mock_agent_settings;
use super::*;

const CONSUMER_MACHINE: &str = r#"name: export-consumer
version: 1
states:
  pending:
    initial: true
    description: Consume the handoff
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

const CONSUMER_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
write(root / 'runtime' / ('spawned-' + task + '.txt'), agent_prompt())
result('consumed the export\n')
"#;

fn consumer_case(prefix: &str, plan: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CONSUMER_MACHINE);
    let agent = write_python_agent(&dir, "consumer.py", CONSUMER_AGENT);
    write_mock_agent_settings(&dir, &agent);
    (dir, plan_path, machine_path)
}

/// Missing, zero-byte, and whitespace-only references are reported in one
/// pre-spawn refusal. Repairing all of them makes the same task runnable.
// §FS-rhei-plan-language.3.12.2 §FS-rhei-agents.3.3 §FS-rhei-run.3
#[test]
fn run_preflights_all_consumed_exports_and_retries_after_repair() {
    let plan = r#"# Rhei: Consumer preflight

## Tasks

### Task 1: API producer
**State:** completed
**Provides:** api-contract

### Task 2: Review producer
**State:** completed
**Provides:** review-notes, zero-byte

### Task 3: Consumer
**State:** pending
**Prior:** 1, 2
**Consumes:** 1:api-contract, 2:review-notes, 2:zero-byte
"#;
    let (dir, plan_path, machine_path) = consumer_case("export-consumer-preflight", plan);
    let export_root = dir.join("runtime/exports/plan.2");
    fs::create_dir_all(&export_root).expect("producer export directory");
    fs::write(export_root.join("review-notes.md"), " \n\t").expect("blank export");
    fs::write(export_root.join("zero-byte.md"), "").expect("zero-byte export");

    let refused = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(
        !refused.status.success(),
        "a consumer with unavailable exports must fail before spawn\nstdout:\n{}\nstderr:\n{}",
        refused.stdout,
        refused.stderr
    );
    let said = format!("{}{}", refused.stdout, refused.stderr);
    for reference in ["plan.1:api-contract", "plan.2:review-notes", "plan.2:zero-byte"] {
        assert!(said.contains(reference), "batched refusal should name {reference}; got:\n{said}");
    }
    assert!(said.contains("missing or blank consumed exports"), "got:\n{said}");
    assert!(
        !dir.join("runtime/spawned-plan.3.txt").exists(),
        "prompt preflight must precede worker spawn"
    );
    assert_task_state(&plan_path, &machine_path, "3", "pending");

    let api_root = dir.join("runtime/exports/plan.1");
    fs::create_dir_all(&api_root).expect("API export directory");
    fs::write(api_root.join("api-contract.md"), "POST /sessions\n").expect("repair API");
    fs::write(export_root.join("review-notes.md"), "Approved\n").expect("repair notes");
    fs::write(export_root.join("zero-byte.md"), "No findings\n").expect("repair zero byte");

    let retried = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&retried);
    assert_task_state(&plan_path, &machine_path, "3", "completed");
    let prompt = fs::read_to_string(dir.join("runtime/spawned-plan.3.txt")).expect("spawn marker");
    for body in ["POST /sessions", "Approved", "No findings"] {
        assert!(prompt.contains(body), "repaired export should reach prompt: {body}\n{prompt}");
    }
}

/// A composition failure belongs to its task. With continuation enabled an
/// unrelated ready sibling still runs, while the refused consumer stays put.
// §FS-rhei-agents.3.3 §FS-rhei-run.3
#[test]
fn continue_on_error_runs_healthy_siblings_after_export_preflight_refusal() {
    let plan = r#"# Rhei: Consumer siblings

## Tasks

### Task 1: Producer
**State:** completed
**Provides:** contract

### Task 2: Broken consumer
**State:** pending
**Prior:** 1
**Consumes:** 1:contract

### Task 3: Healthy sibling
**State:** pending
"#;
    let (dir, plan_path, machine_path) = consumer_case("export-consumer-siblings", plan);

    let run = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--continue-on-error"],
    );
    assert!(!run.status.success(), "one refused non-terminal task keeps the run nonzero");
    assert_task_state(&plan_path, &machine_path, "2", "pending");
    assert_task_state(&plan_path, &machine_path, "3", "completed");
    assert!(!dir.join("runtime/spawned-plan.2.txt").exists(), "broken consumer did not spawn");
    assert!(dir.join("runtime/spawned-plan.3.txt").exists(), "healthy sibling did spawn");
}

/// A pre-qualification file is not accepted under the wrong id. The refusal
/// points at the existing legacy file so the repair is a rename, not a rewrite.
// §FS-rhei-plan-language.3.12.2 §FS-rhei-agents.3.3
#[test]
fn consumed_export_preflight_hints_at_a_prequalification_file() {
    let plan = r#"# Rhei: Legacy export id

## Tasks

### Task 1: Producer
**State:** completed
**Provides:** context

### Task 2: Consumer
**State:** pending
**Prior:** 1
**Consumes:** 1:context
"#;
    let (dir, plan_path, machine_path) = consumer_case("export-consumer-legacy-id", plan);
    let legacy = dir.join("runtime/exports/1/context.md");
    fs::create_dir_all(legacy.parent().expect("legacy parent")).expect("legacy directory");
    fs::write(&legacy, "legacy context\n").expect("legacy export");

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success(), "a legacy-id file must not satisfy the qualified path");
    let said = format!("{}{}", run.stdout, run.stderr);
    assert!(said.contains("rename the pre-qualification export"), "got:\n{said}");
    assert!(said.contains(&legacy.display().to_string()), "got:\n{said}");
    assert!(!dir.join("runtime/spawned-plan.2.txt").exists(), "consumer did not spawn");
    assert_task_state(&plan_path, &machine_path, "2", "pending");
}

/// Producer routing is independent of the narrowed candidate set: a consumer
/// reads a prior's export from that prior rhei's execution root.
// §FS-rhei-plan-language.3.12.2 §FS-rhei-panta.6.1
#[test]
fn narrowed_cross_rhei_consumer_reads_from_the_producer_execution_root() {
    let dir = unique_temp_dir("export-consumer-cross-rhei");
    let project = dir.join("project");
    let producer = project.join("producer");
    let consumer = project.join("consumer");
    fs::create_dir_all(producer.join("tasks")).expect("producer workspace");
    fs::create_dir_all(consumer.join("tasks")).expect("consumer workspace");
    write_fixture_file(&project, "index.panta.md", "# Panta: Cross-rhei exports\n");
    write_fixture_file(&producer, "index.rhei.md", "# Rhei: Producer\n");
    write_fixture_file(
        &producer.join("tasks"),
        "01.md",
        "### Task 1: Publish\n**State:** completed\n**Provides:** context\n",
    );
    write_fixture_file(&consumer, "index.rhei.md", "# Rhei: Consumer\n");
    write_fixture_file(
        &consumer.join("tasks"),
        "01.md",
        "### Task 1: Read\n**State:** pending\n**Prior:** producer.1\n**Consumes:** producer.1:context\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", CONSUMER_MACHINE);
    let agent = write_python_agent(&dir, "consumer.py", CONSUMER_AGENT);
    write_mock_agent_settings(&project, &agent);
    let export = producer.join("runtime/exports/producer.1/context.md");
    fs::create_dir_all(export.parent().expect("export parent")).expect("export directory");
    fs::write(&export, "producer-owned context\n").expect("producer export");

    let run = run_cli(
        "run",
        &project,
        &machine_path,
        &["--rhei", "consumer", "--no-tui", "--no-callbacks"],
    );
    assert_success(&run);
    let prompt = fs::read_to_string(consumer.join("runtime/spawned-consumer.1.txt"))
        .expect("consumer prompt");
    assert!(prompt.contains("producer-owned context"), "got:\n{prompt}");
}

/// Export availability gates worker spawn, not a human's explicit state
/// transition. The operator may deliberately advance an out-of-order consumer.
// §FS-rhei-plan-language.3.12.2 §FS-rhei-transition-cmd.3
#[test]
fn human_transition_keeps_the_out_of_order_consumer_escape_hatch() {
    let plan = r#"# Rhei: Human consumer transition

## Tasks

### Task 1: Producer
**State:** pending
**Provides:** context

### Task 2: Consumer
**State:** pending
**Prior:** 1
**Consumes:** 1:context
"#;
    let (_dir, plan_path, machine_path) = consumer_case("export-consumer-human-transition", plan);

    let transitioned = run_transition_with_result(
        &plan_path,
        &machine_path,
        "2",
        "pending",
        "completed",
        "Advanced deliberately by a human.",
    );
    assert_success(&transitioned);
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert_task_state(&plan_path, &machine_path, "2", "completed");
}
