const RUN_STATE_MACHINE: &str = r#"name: run-test
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task being worked on
  completed:
    description: Task finished
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

fn run_run_command(plan_path: &Path, machine_path: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = rhei_command();
    if let Some(parent) = plan_path.parent() {
        cmd.current_dir(parent);
    }
    cmd.arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("run command should execute");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_run_command_with_env(
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
    envs: &[(&str, &str)],
) -> CliRun {
    let mut cmd = rhei_command();
    if let Some(parent) = plan_path.parent() {
        cmd.current_dir(parent);
    }
    cmd.arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run command should execute");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_run_command_in_dir(
    current_dir: &Path,
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = rhei_command();
    cmd.current_dir(current_dir).arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("run command should execute");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_run_command_in_dir_with_env(
    current_dir: &Path,
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
    envs: &[(&str, &str)],
) -> CliRun {
    let mut cmd = rhei_command();
    cmd.current_dir(current_dir).arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run command should execute");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_reset_command(plan_path: &Path, machine_path: &Path) -> CliRun {
    // §FS-rhei-reset.1.2: a test harness has no terminal to confirm on, so it
    // states the intent the way any unattended caller must.
    run_reset_command_with_args(plan_path, machine_path, &["-y"])
}

fn run_reset_command_with_args(plan_path: &Path, machine_path: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command();
    cmd.arg("--state-machine").arg(machine_path).arg("reset").arg(plan_path);
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("reset command should run");

    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn run_advances_linear_chain_to_completion() {
    let plan = r#"# Rhei: Linear Chain

## Tasks

### Task 1: First
**State:** pending

### Task 2: Second
**State:** pending
**Prior:** Task 1

### Task 3: Third
**State:** pending
**Prior:** Task 2
"#;

    let dir = unique_temp_dir("run-linear");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // All tasks should reach completed state (pending→in-progress→completed for each).
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    for task in &rhei.tasks {
        assert_eq!(
            task.state.as_str(),
            "completed",
            "Task {} should be completed, got {:?}",
            task.id,
            task.state
        );
    }

    // Should report 6 transitions (2 per task × 3 tasks).
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "should report 6 transitions; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("3/3 tasks in terminal state"),
        "should report all tasks terminal; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_advances_parallel_ready_tasks() {
    let plan = r#"# Rhei: Parallel Tasks

## Tasks

### Task 1: Root
**State:** completed

### Task 2: Branch A
**State:** pending
**Prior:** Task 1

### Task 3: Branch B
**State:** pending
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("run-parallel");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Both branches should complete.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    for task in &rhei.tasks {
        assert_eq!(task.state.as_str(), "completed", "Task {} should be completed", task.id);
    }

    // 4 transitions: 2 each for Task 2 and Task 3 (Task 1 already completed).
    assert!(
        result.stdout.contains("4 transition(s) made"),
        "should report 4 transitions; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_uses_counted_loop_exit_when_visit_budget_is_exhausted() {
    let plan = r#"# Rhei: Exhausted Loop

---
metadata:
  tasks:
    1:
      stateVisits:
        agent-review: 2
---

## Tasks

### Task 1: Needs escalation
**State:** agent-review
"#;

    let dir = unique_temp_dir("run-counted-loop");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task exists");
    assert_eq!(task.state, "completed");
    assert!(
        !result.stdout.contains("agent-review-fix"),
        "run should escalate instead of looping through fix once exhausted; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_dry_run_shows_transitions_without_changes() {
    let plan = r#"# Rhei: Dry Run Test

## Tasks

### Task 1: Alpha
**State:** pending

### Task 2: Beta
**State:** pending
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("run-dry");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &["--dry-run"]);

    assert!(
        result.status.success(),
        "dry run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    assert!(
        result.stdout.contains("would transition: Task plan.1  pending -> in-progress"),
        "should show what would be transitioned; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Dry run complete"),
        "should indicate no changes; got:\n{}",
        result.stdout
    );

    // File should be unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan, "dry run should not modify the file");
    assert!(!dir.join("runtime").exists(), "dry run must not create runtime artifacts");
    assert!(!dir.join(".rhei").exists(), "dry run must not create a run lock");
}

#[test]
fn run_dry_run_labels_agent_fanout_targets() {
    let plan = r#"# Rhei: Fanout Dry Run Test

## Tasks

### Task 1: Parallel review
**State:** review
"#;
    let machine = r#"name: fanout-dry-run-test
version: 1
states:
  review:
    description: Review in parallel
    all_targets:
      - codex[yolo]:openai:gpt-5.5
      - codex[yolo]:openai:gpt-5.4
  completed:
    description: Done
    final: true
transitions:
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("run-dry-fanout");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--dry-run"]);

    assert!(
        result.status.success(),
        "dry run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result
            .stdout
            .contains("would transition: Task plan.1  review -> completed [target=codex-yolo-openai-gpt-5.5]"),
        "should label the first fanout target; got:\n{}",
        result.stdout
    );
    assert!(
        result
            .stdout
            .contains("would transition: Task plan.1  review -> completed [target=codex-yolo-openai-gpt-5.4]"),
        "should label the second fanout target; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_callback_failure_halts_execution() {
    let machine = r#"name: run-callback-test
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "cli:exit 1"
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: Callback Failure

## Tasks

### Task 1: Blocked
**State:** pending
"#;

    let dir = unique_temp_dir("run-callback-fail");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        !result.status.success(),
        "run should fail when progress halts with a non-terminal task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Task should remain in pending since on_leave rejected.
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1");
    assert_eq!(task.state.as_str(), "pending", "task should remain pending after callback failure");

    assert!(
        result.stderr.contains("non-terminal tasks remaining"),
        "should report halted non-terminal work; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
