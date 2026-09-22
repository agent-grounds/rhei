//! A live run validates the complete graph at every scheduling reread, not
//! only when a new Panta member appears. The invalid cases deliberately append
//! malformed text during the run; fresh validation is the control proving the
//! text itself, rather than fixture setup, is what must stop scheduling.

use std::fs;
use std::path::{Path, PathBuf};

use super::supervision_tests::setup_supervision_with_agent;
use super::*;

const INVALID_PRIOR: &str = "Task prereq";
const VALID_PRIOR: &str = "Task producer, Task prereq";

const AGENT_PLAN: &str = r#"# Rhei: Live checkpoint validation

---
structure:
  maxLevels: 3
---

## Tasks

### Task producer: Produce the report
**State:** completed
**Provides:** report

### Task prereq: Finish an unrelated prerequisite
**State:** completed

### Task supervisor: Append the consumer
**State:** supervising
"#;

const ROOT_APPENDER_PLAN: &str = r#"# Rhei: Live checkpoint validation

## Tasks

### Task producer: Produce the report
**State:** completed
**Provides:** report

### Task prereq: Finish an unrelated prerequisite
**State:** completed

### Task appender: Append the consumer
**State:** append
"#;

const SUPERVISION_MACHINE: &str = r#"name: live-checkpoint-agent
version: 1
states:
  supervising:
    initial: true
    description: Append and supervise the consumer
    execute_on: child-terminal
    concurrent: true
    agent: mock
    agent_timeout: 10s
    visits: 4
  work:
    description: Execute the appended consumer
    concurrent: true
    agent: mock
    agent_timeout: 10s
  block:
    description: Occupy a parallel slot
    concurrent: true
    agent: mock
    agent_timeout: 10s
  completed:
    description: Done
    final: true
transitions:
  - { from: supervising, to: completed, condition: openDescendants < 1 }
  - { from: supervising, to: supervising }
  - { from: work, to: completed }
  - { from: block, to: completed }
"#;

struct CheckpointFixture {
    _dir: TestDir,
    root: PathBuf,
    plan: PathBuf,
    machine: PathBuf,
    consumer_id: &'static str,
}

impl CheckpointFixture {
    fn marker(&self) -> PathBuf {
        self.root.join("runtime/consumer-executed")
    }

    fn result(&self) -> PathBuf {
        self.root.join(format!("runtime/results/{}.md", self.consumer_id))
    }
}

fn write_available_export(root: &Path) {
    let export = root.join("runtime/exports/plan.producer/report.md");
    fs::create_dir_all(export.parent().expect("export has a parent"))
        .expect("create producer export directory");
    fs::write(export, "available report\n").expect("write producer export");
}

fn appended_consumer(prior: &str, child: bool) -> String {
    let heading = if child { "#### Task supervisor.consumer" } else { "### Task consumer" };
    format!(
        "\n\n{heading}: Consume the report\n**State:** work\n**Prior:** {prior}\n\
         **Consumes:** producer:report\n"
    )
}

fn agent_fixture(prefix: &str, prior: &str, parallel: bool) -> CheckpointFixture {
    let child = appended_consumer(prior, true);
    let child_literal = serde_json::to_string(&child).expect("consumer text is JSON");
    let script = format!(
        r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
marker = root / 'runtime' / 'consumer-executed'

if local == 'supervisor':
    if {parallel}:
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline and not (root / 'runtime' / 'blocker-started').exists():
            time.sleep(0.02)
        if not (root / 'runtime' / 'blocker-started').exists():
            sys.exit(31)
    plan = pathlib.Path(env('RHEI_PLAN_PATH'))
    if plan.is_dir():
        plan = plan / 'tasks' / '03-supervisor.md'
    text = plan.read_text(encoding='utf-8')
    if 'Task supervisor.consumer' not in text:
        write(plan, text.rstrip('\n') + {child_literal})
        sys.exit(0)
    result('## Result\n\nSupervisor finished.\n')
elif local == 'blocker':
    write(root / 'runtime' / 'blocker-started', 'started\n')
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline and not marker.exists():
        time.sleep(0.02)
    result('## Result\n\nBlocker finished.\n')
elif local == 'supervisor.consumer':
    write(marker, 'executed\n')
    result('## Result\n\nConsumer executed.\n')
"#,
        parallel = if parallel { "True" } else { "False" },
    );
    if !parallel {
        let (dir, plan, machine) =
            setup_supervision_with_agent(prefix, AGENT_PLAN, SUPERVISION_MACHINE, &script);
        write_available_export(&dir);
        return CheckpointFixture {
            root: dir.to_path_buf(),
            _dir: dir,
            plan,
            machine,
            consumer_id: "plan.supervisor.consumer",
        };
    }

    let dir = unique_temp_dir(prefix);
    let root = dir.join("plan");
    let tasks = root.join("tasks");
    fs::create_dir_all(&tasks).expect("create parallel workspace");
    fs::write(
        root.join("index.rhei.md"),
        "# Rhei: Parallel live checkpoint validation\n\n---\nstructure:\n  maxLevels: 3\n---\n",
    )
    .expect("write parallel workspace index");
    fs::write(
        tasks.join("01-producer.md"),
        "### Task producer: Produce the report\n**State:** completed\n**Provides:** report\n",
    )
    .expect("write producer task");
    fs::write(
        tasks.join("02-prereq.md"),
        "### Task prereq: Finish an unrelated prerequisite\n**State:** completed\n",
    )
    .expect("write prerequisite task");
    fs::write(
        tasks.join("03-supervisor.md"),
        "### Task supervisor: Append the consumer\n**State:** supervising\n",
    )
    .expect("write supervisor task");
    fs::write(
        tasks.join("04-blocker.md"),
        "### Task blocker: Occupy the other worker slot\n**State:** block\n",
    )
    .expect("write blocker task");
    let machine = write_fixture_file(&root, "states.yaml", SUPERVISION_MACHINE);
    let agent = write_python_agent(&root, "mock-agent.py", &script);
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create parallel settings directory");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{ "mock": {{ "command": {}, "prompt_flag": "--prompt", "timeout": "10s" }} }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write parallel settings");
    write_available_export(&root);
    CheckpointFixture {
        root: root.clone(),
        _dir: dir,
        plan: root,
        machine,
        consumer_id: "plan.supervisor.consumer",
    }
}

fn program_fixture() -> CheckpointFixture {
    let dir = unique_temp_dir("issue-306-program-checkpoint");
    let child = appended_consumer(INVALID_PRIOR, false);
    let child_literal = serde_json::to_string(&child).expect("consumer text is JSON");
    let script = write_python_agent(
        &dir,
        "append-consumer.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
if local == 'appender':
    plan = pathlib.Path(env('RHEI_PLAN_PATH'))
    text = plan.read_text(encoding='utf-8')
    write(plan, text.rstrip('\n') + {child_literal})
    result('## Result\n\nConsumer appended.\n')
elif local == 'consumer':
    write(root / 'runtime' / 'consumer-executed', 'executed\n')
    result('## Result\n\nConsumer executed.\n')
"#,
        ),
    );
    let command = fixture_command(&script);
    let machine_text = format!(
        r#"name: live-checkpoint-program
version: 1
states:
  append:
    initial: true
    description: Append the consumer
    program:
      command: {command}
  work:
    description: Execute the appended consumer
    program:
      command: {command}
  completed:
    description: Done
    final: true
transitions:
  - {{ from: append, to: completed, exit_code: 0 }}
  - {{ from: work, to: completed, exit_code: 0 }}
"#,
    );
    let plan = write_fixture_file(&dir, "plan.rhei.md", ROOT_APPENDER_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", &machine_text);
    write_available_export(&dir);
    CheckpointFixture {
        root: dir.to_path_buf(),
        _dir: dir,
        plan,
        machine,
        consumer_id: "plan.consumer",
    }
}

fn callback_fixture() -> CheckpointFixture {
    let dir = unique_temp_dir("issue-306-callback-checkpoint");
    let child = appended_consumer(INVALID_PRIOR, false);
    let child_literal = serde_json::to_string(&child).expect("consumer text is JSON");
    let script = write_python_agent(
        &dir,
        "append-consumer-callback.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
if local == 'appender':
    plan = pathlib.Path(env('RHEI_PLAN_PATH'))
    text = plan.read_text(encoding='utf-8')
    write(plan, text.rstrip('\n') + {child_literal})
elif local == 'consumer':
    write(root / 'runtime' / 'consumer-executed', 'executed\n')
"#,
        ),
    );
    let callback = serde_json::to_string(&format!("cli:{}", fixture_command_line(&script)))
        .expect("callback is YAML");
    let machine_text = format!(
        r#"name: live-checkpoint-callback
version: 1
states:
  append:
    initial: true
    description: Append the consumer
  work:
    description: Execute the appended consumer
  completed:
    description: Done
    final: true
transitions:
  - from: append
    to: completed
    on_enter: {callback}
  - from: work
    to: completed
    on_enter: {callback}
"#,
    );
    let plan = write_fixture_file(&dir, "plan.rhei.md", ROOT_APPENDER_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", &machine_text);
    write_available_export(&dir);
    CheckpointFixture {
        root: dir.to_path_buf(),
        _dir: dir,
        plan,
        machine,
        consumer_id: "plan.consumer",
    }
}

fn run_fixture(fixture: &CheckpointFixture, args: &[&str]) -> CliRun {
    run_cli("run", &fixture.plan, &fixture.machine, args)
}

fn assert_invalid_append_is_rejected(fixture: &CheckpointFixture, run: &CliRun) {
    let diagnostic = format!(
        "Task {} consumes export 'report' from Task plan.producer and must list Task plan.producer directly in **Prior:**",
        fixture.consumer_id
    );
    let help =
        format!("rhei migrate export-priors {}", shell_quote(&fixture.plan.display().to_string()));

    let fresh = run_cli("validate", &fixture.plan, &fixture.machine, &[]);
    assert!(!fresh.status.success(), "fresh validation must reject the authored graph");
    assert_stderr_contains(&fresh, &diagnostic);
    assert_stderr_contains(&fresh, &help);

    assert!(
        !run.status.success()
            && run.stderr.contains(&diagnostic)
            && run.stderr.contains(&help)
            && !fixture.marker().exists()
            && !fixture.result().exists(),
        "the live checkpoint must reject the same graph before the consumer executes\n\
         exit: {:?}\nconsumer marker: {}\nconsumer result: {}\nstdout:\n{}\nstderr:\n{}",
        run.status.code(),
        fixture.marker().exists(),
        fixture.result().exists(),
        run.stdout,
        run.stderr
    );
}

/// The original reproducer: a supervising agent appends a consumer inside an
/// already initialized member. The next sequential ready scan must validate
/// that unchanged member set before the consumer can run.
// §FS-rhei-run.3 §FS-rhei-supervision.4.1 §FS-rhei-plan-language.3.12.1
#[test]
fn issue_306_sequential_checkpoint_rejects_an_invalid_supervisor_append() {
    let fixture = agent_fixture("issue-306-sequential-checkpoint", INVALID_PRIOR, false);
    let run = run_fixture(&fixture, &["--no-tui", "--parallel", "1"]);
    assert_invalid_append_is_rejected(&fixture, &run);
}

/// A completed supervisor frees one of two live slots. Validation happens
/// before that slot can be refilled from the newly authored ready set.
// §FS-rhei-run.3 §FS-rhei-supervision.4.1 §FS-rhei-plan-language.3.12.1
#[test]
fn issue_306_parallel_refill_rejects_an_invalid_supervisor_append() {
    let fixture = agent_fixture("issue-306-parallel-checkpoint", INVALID_PRIOR, true);
    let run = run_fixture(&fixture, &["--no-tui", "--parallel", "2"]);
    assert_invalid_append_is_rejected(&fixture, &run);
}

/// Program completion enters the same checkpoint as agent completion; it may
/// not hand malformed same-member text to the following ready scan.
// §FS-rhei-run.3 §FS-rhei-plan-language.3.12.1
#[test]
fn issue_306_program_completion_rejects_an_invalid_append() {
    let fixture = program_fixture();
    let run = run_fixture(&fixture, &["--no-tui", "--no-callbacks"]);
    assert_invalid_append_is_rejected(&fixture, &run);
}

/// Callback-only advancement also rereads before scanning again. Its callback
/// can author a task, but that task does not bypass the startup contract.
// §FS-rhei-run.3 §FS-rhei-plan-language.3.12.1
#[test]
fn issue_306_callback_only_advance_rejects_an_invalid_append() {
    let fixture = callback_fixture();
    let run = run_fixture(&fixture, &["--no-tui"]);
    assert_invalid_append_is_rejected(&fixture, &run);
}

/// The rejection is about declaration validity, not about live appends: with
/// the producer authored directly in `Prior`, the child still runs and wakes
/// its supervisor during the original invocation.
// §FS-rhei-run.3 §FS-rhei-supervision.4.1 §FS-rhei-plan-language.3.12.1
#[test]
fn issue_306_valid_supervisor_append_runs_in_the_same_invocation() {
    let fixture = agent_fixture("issue-306-valid-checkpoint", VALID_PRIOR, false);
    let run = run_fixture(&fixture, &["--no-tui", "--parallel", "1"]);
    assert_success(&run);
    assert!(fixture.marker().is_file(), "the valid appended consumer must execute");
    assert!(fixture.result().is_file(), "the valid appended consumer must leave its result");
    assert_success(&run_cli("validate", &fixture.plan, &fixture.machine, &[]));
}
