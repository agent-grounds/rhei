// §AR-source-file-size.3: `rhei run` behavior — scheduling, workflows,
// callbacks, overrides, and programs. Signal and shutdown supervision, which
// carries its own process harness, is in `run_signals_tests.rs`.
use std::fs;

use super::*;

#[test]
fn run_builtin_default_refuses_manual_pending_tasks() {
    let dir = unique_temp_dir("run-builtin-manual-pending");
    let plan = r#"# Rhei: Manual Default

## Tasks

### Task 1: Do manually
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);

    let result = run_cli_without_machine("run", &plan_path, &[]);
    assert!(
        !result.status.success(),
        "run should refuse manual-only built-in pending task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "Task plan.1 is in manual-only initial state 'pending'");
    assert_stderr_contains(&result, "rhei next");
    assert_stderr_contains(&result, "rhei complete");
    let content = fs::read_to_string(&plan_path).expect("read plan after failed run");
    assert!(
        content.contains("**State:** pending") && !content.contains("**State:** completed"),
        "run must not complete manual task; got:\n{}",
        content
    );
}

/// §FS-rhei-run.4: `--dry-run` exists to show what would happen. Aborting on
/// the first manual-only task made it fail before printing anything under the
/// built-in machine, whose initial state is manual-only.
#[test]
fn run_dry_run_reports_every_manual_only_task_instead_of_aborting() {
    let dir = unique_temp_dir("run-dry-run-manual-pending");
    let plan = r#"# Rhei: Manual Default

## Tasks

### Task 1: Do manually
**State:** pending

### Task 2: Also manually
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);

    let result = run_cli_without_machine("run", &plan_path, &["--dry-run", "--no-tui"]);
    assert!(
        !result.status.success(),
        "a dry run that found manual-only tasks still exits non-zero\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    for id in ["plan.1", "plan.2"] {
        assert!(
            combined.contains(&format!("manual-only: Task {id}")),
            "the scan must continue and report {id}; got:\n{combined}"
        );
    }
    assert!(
        combined.contains("rhei next") && combined.contains("rhei complete"),
        "the report must name the manual worker loop; got:\n{combined}"
    );

    let content = fs::read_to_string(&plan_path).expect("read plan after dry run");
    assert!(
        !content.contains("**State:** completed"),
        "a dry run must not rewrite the plan; got:\n{content}"
    );
}

#[test]
fn run_single_file_linear_to_completion() {
    let (_dir, plan_path, machine_path) = setup_single_file("run-linear", LINEAR_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("Running plan 'Linear Chain' with 3 task(s)"),
        "expected run header; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Pass 1: 1 ready, 0 terminal, 3 total."),
        "expected pass summary; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Final states: completed=3"),
        "expected final state summary; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions (2 per task); got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("3/3 tasks in terminal state"),
        "expected all tasks terminal; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_writes_durable_report_and_points_at_it() {
    // §FS-rhei-run-report.1: every run leaves a durable Markdown report and the
    // non-TTY output gains a greppable `Report:` pointer.
    let (dir, plan_path, machine_path) = setup_single_file("run-report", LINEAR_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert!(
        result.stdout.contains("Report: runtime/run-report.md"),
        "expected non-TTY Report pointer; got:\n{}",
        result.stdout
    );

    let runtime = dir.join("runtime");
    let report = fs::read_to_string(runtime.join("run-report.md")).expect("durable report written");
    assert!(report.starts_with("# Run Report: Linear Chain"), "got:\n{report}");
    assert!(report.contains("## Transition Ledger"), "got:\n{report}");
    assert!(report.contains("## Task Final States"), "got:\n{report}");
    assert!(report.contains("Result: completed"), "got:\n{report}");

    let history: Vec<_> = fs::read_dir(runtime.join("run-reports"))
        .expect("history dir exists")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(history.len(), 1, "one timestamped history entry written");
}

#[test]
fn run_single_file_parallel_to_completion() {
    let (_dir, plan_path, machine_path) = setup_single_file("run-parallel", PARALLEL_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_single_file_independent_to_completion() {
    let (_dir, plan_path, machine_path) = setup_single_file("run-independent", INDEPENDENT_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_workspace_linear_to_completion() {
    let (_dir, ws, machine_path) = create_workspace(
        "run-ws-linear",
        "# Rhei: Workspace Linear\n",
        &[
            ("a.md", "### Task 1: First\n**State:** draft\n"),
            ("b.md", "### Task 2: Second\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Third\n**State:** draft\n**Prior:** Task 2\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    // Verify via CLI render.
    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    // Verify individual task files contain the updated state.
    for name in &["a.md", "b.md", "c.md"] {
        let content = fs::read_to_string(ws.join("tasks").join(name)).expect("read task file");
        assert!(
            content.contains("**State:** completed"),
            "{} should contain completed state: {}",
            name,
            content
        );
    }

    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_workspace_parallel_to_completion() {
    let (_dir, ws, machine_path) = create_workspace(
        "run-ws-parallel",
        "# Rhei: Workspace Parallel\n",
        &[
            ("a.md", "### Task 1: Root\n**State:** draft\n"),
            ("b.md", "### Task 2: Branch A\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Branch B\n**State:** draft\n**Prior:** Task 1\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&ws, &machine_path, "completed");
}

#[test]
fn run_script_agent_team_fixture_to_completion() {
    let (_dir, workspace_path, machine_path) =
        copy_workspace_fixture("run-script-agent-team", "script-agent-team");

    assert!(
        workspace_path.starts_with(repo_root().join("scratchpad")),
        "fixture copy should live under the shared gitignored scratchpad"
    );

    let result = run_cli("run", &workspace_path, &machine_path, &[]);
    assert_success(&result);

    assert_all_tasks_in_state(&workspace_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6/6 tasks in terminal state"),
        "expected all tasks terminal; got:\n{}",
        result.stdout
    );

    let team_log =
        fs::read_to_string(workspace_path.join("runtime/logs/team.log")).expect("read team log");
    assert!(
        team_log.contains("mock kickoff command executed"),
        "expected kickoff log entry; got:\n{}",
        team_log
    );
    assert!(
        team_log.contains("reviewer finalized task"),
        "expected finalize log entry; got:\n{}",
        team_log
    );

    // Callbacks receive the project-qualified id, so ticket-keyed artifact
    // paths carry it — the same key space a narrowed reset matches on.
    for task_id in &["script-agent-team.1", "script-agent-team.2", "script-agent-team.3"] {
        let artifact_dir = workspace_path.join(format!("runtime/artifacts/task-{task_id}"));
        assert!(
            artifact_dir.join("40-complete.txt").exists(),
            "task {} should have a completion artifact",
            task_id
        );
    }
}

#[test]
fn run_living_review_loop_fixture_to_completion() {
    let (_dir, workspace_path, machine_path) =
        copy_workspace_fixture("run-living-review-loop", "living-review-loop");

    let result = run_cli("run", &workspace_path, &machine_path, &["--no-agent"]);
    assert_success(&result);

    assert_all_tasks_in_state(&workspace_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("Workspace expanded: discovered 3 new task(s)"),
        "expected dynamic workspace expansion output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("6/6 tasks in terminal state"),
        "expected dynamically expanded tasks to complete; got:\n{}",
        result.stdout
    );

    let findings = fs::read_to_string(workspace_path.join("runtime/findings/review-findings.md"))
        .expect("read findings file");
    assert!(
        findings.contains("## Model claude"),
        "expected consolidated findings file; got:\n{}",
        findings
    );

    let verify_irrelevant =
        fs::read_to_string(workspace_path.join("runtime/verifications/F-002.md"))
            .expect("read verification file");
    assert!(
        verify_irrelevant.contains("- Relevant: no"),
        "expected non-relevant verification outcome; got:\n{}",
        verify_irrelevant
    );

    assert!(
        !workspace_path.join("tasks/13-fix-cli-help.md").exists(),
        "non-relevant finding should not produce a fix task"
    );
    assert!(
        workspace_path.join("tasks/11-fix-cache-key.md").exists(),
        "relevant finding F-001 should produce a fix task"
    );
    assert!(
        workspace_path.join("tasks/12-fix-timeout-details.md").exists(),
        "relevant finding F-003 should produce a fix task"
    );

    let team_log =
        fs::read_to_string(workspace_path.join("runtime/logs/team.log")).expect("read team log");
    assert!(
        team_log.contains("spawned verification tasks"),
        "expected review expansion in team log; got:\n{}",
        team_log
    );
    assert!(
        team_log.contains("spawned a fix task"),
        "expected selective fix expansion in team log; got:\n{}",
        team_log
    );
}

#[test]
fn run_applies_task_model_and_target_overrides_to_agent_processes() {
    let dir = unique_temp_dir("run-task-execution-overrides");
    let agent_script = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"workspace = pathlib.Path(env('RHEI_PLAN_PATH')).parent
append(
    workspace / 'runtime' / 'logs' / 'override-agent.log',
    'task={} model={} target={} mode={} agent={} provider={} name={}\n'.format(
        env('RHEI_TASK_ID'),
        env('RHEI_MODEL'),
        env('RHEI_TARGET'),
        env('RHEI_AGENT_MODE'),
        env('RHEI_AGENT'),
        env('RHEI_MODEL_PROVIDER'),
        env('RHEI_MODEL_NAME'),
    ),
)
result('## Result\n\nMock agent finished.\n')
"#,
    );
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&agent_script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{
        "yolo": [],
        "slow": []
      }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }},
    "special-model": {{ "provider": "mock", "model": "special-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: task-overrides
version: 1
models: [default-model, special-model]
states:
  work:
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let plan = r#"# Rhei: Task Overrides

## Tasks

### Task model-override: Use a task model
**State:** work
**Model:** special-model

### Task target-override: Use a task target
**State:** work
**Target:** mock[slow]:mock:target-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let log =
        fs::read_to_string(dir.join("runtime/logs/override-agent.log")).expect("read override log");
    assert!(
        log.contains(
            "task=plan.model-override model=special-model target=mock[yolo]:mock:special-model mode=yolo agent=mock provider=mock name=special-model"
        ),
        "model override did not preserve state target identity with swapped model; log:\n{}",
        log
    );
    assert!(
        log.contains(
            "task=plan.target-override model=target-model target=mock[slow]:mock:target-model mode=slow agent=mock provider=mock name=target-model"
        ),
        "target override did not replace full identity; log:\n{}",
        log
    );
    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
}

#[test]
fn validate_rejects_task_override_on_fanout_state() {
    let dir = unique_temp_dir("validate-task-override-fanout");
    let machine = r#"name: task-overrides-fanout
version: 1
models: [default-model, special-model]
states:
  review:
    all_models: [default-model, special-model]
    agent: codex
  completed:
    final: true
transitions: [{ from: review, to: completed }]
"#;
    let plan = r#"# Rhei: Task Override Fanout

## Tasks

### Task 1: Invalid override
**State:** review
**Model:** special-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(
        !result.status.success(),
        "validate should reject task override on fanout state\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "Task plan.1 declares a task execution override");
    assert_stderr_contains(&result, "fanout state");
}

#[test]
fn run_uses_task_override_for_transition_output_artifact_checks() {
    let dir = unique_temp_dir("run-task-override-output-checks");
    let agent_script = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"workspace = pathlib.Path(env('RHEI_PLAN_PATH')).parent
model = env('RHEI_MODEL')
write(workspace / 'runtime' / 'outputs' / (model + '.txt'), 'model={}\n'.format(model))
result('## Result\n\nMock agent finished.\n')
"#,
    );
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&agent_script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }},
    "special-model": {{ "provider": "mock", "model": "special-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: output-checks
version: 1
models: [default-model, special-model]
states:
  work:
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
    outputs:
      - name: model-output
        path: runtime/outputs/{model}.txt
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let plan = r#"# Rhei: Task Override Output Checks

## Tasks

### Task 1: Use special output
**State:** work
**Model:** special-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(dir.join("runtime/outputs/special-model.txt").exists());
    assert!(!dir.join("runtime/outputs/default-model.txt").exists());
}

#[test]
fn run_does_not_create_agent_work_from_task_override_in_callback_state() {
    let dir = unique_temp_dir("run-task-override-callback-only");
    let agent_script = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"workspace = pathlib.Path(env('RHEI_PLAN_PATH')).parent
append(workspace / 'runtime' / 'logs' / 'agent.log', 'unexpected agent spawn\n')
"#,
    );
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&agent_script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: callback-only
version: 1
states:
  step: {}
  completed:
    final: true
transitions:
  - from: step
    to: completed
"#;
    let plan = r#"# Rhei: Callback Only Override

## Tasks

### Task 1: Callback transition
**State:** step
**Target:** mock[yolo]:mock:target-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(
        !dir.join("runtime/logs/agent.log").exists(),
        "task override should not spawn an agent for callback-only states"
    );
}

#[test]
fn run_cli_model_override_supersedes_task_target_model() {
    let dir = unique_temp_dir("run-cli-model-over-task-target");
    let agent_script = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"workspace = pathlib.Path(env('RHEI_PLAN_PATH')).parent
append(
    workspace / 'runtime' / 'logs' / 'agent.log',
    'model={} target={}\n'.format(env('RHEI_MODEL'), env('RHEI_TARGET')),
)
result('## Result\n\nMock agent finished.\n')
"#,
    );
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&agent_script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{
        "yolo": [],
        "slow": []
      }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }},
    "cli-model": {{ "provider": "mock", "model": "cli-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: cli-model-over-target
version: 1
models: [default-model, cli-model]
states:
  work:
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let plan = r#"# Rhei: CLI Model Over Task Target

## Tasks

### Task 1: Target with CLI model
**State:** work
**Target:** mock[slow]:mock:task-target-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--model", "cli-model"],
    );
    assert_success(&result);

    let log = fs::read_to_string(dir.join("runtime/logs/agent.log")).expect("read agent log");
    assert!(
        log.contains("model=cli-model target=mock[slow]:mock:cli-model"),
        "CLI model override should replace the task target model segment; log:\n{}",
        log
    );
}

#[test]
fn run_executes_program_states_and_routes_on_exit_code() {
    let plan = r#"# Rhei: Program State Run

## Tasks

### Task 1: Build artifact
**State:** build
"#;
    let dir = unique_temp_dir("run-program-state");
    let program = write_python_agent(
        &dir,
        "build.py",
        r#"write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'program-1.txt', 'ok\n')
result('## Result\n\nBuilt the artifact.\n')
"#,
    );
    let machine = format!(
        r#"name: program-demo
version: 1
states:
  build:
    description: Build the artifact
    program:
      command: {command}
  completed:
    description: Done
    final: true
  failed:
    description: Failed
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
  - from: build
    to: failed
    exit_code: nonzero
"#,
        command = fixture_command(&program)
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(
        dir.join("runtime/program-1.txt").exists(),
        "program should have produced its output artifact"
    );
    assert!(
        result.stdout.contains("program(s) spawned"),
        "expected program summary in output; got:\n{}",
        result.stdout
    );
}

/// §FS-rhei-programs.3.2/3.3: an exit_code-less transition is a forward edge
/// available only via `rhei transition` or the program itself, never picked
/// by exit-code evaluation, even on a non-zero exit with no other rule.
#[test]
fn run_program_exit_nonzero_does_not_select_an_exit_code_less_transition() {
    let plan = r#"# Rhei: Program No Exit Code Rule

## Tasks

### Task 1: Build artifact
**State:** build
"#;
    let dir = unique_temp_dir("run-program-no-exit-code-rule");
    let program = write_python_agent(&dir, "build.py", "sys.exit(1)\n");
    let machine = format!(
        r#"name: program-no-exit-code-rule
version: 1
states:
  build:
    description: Build the artifact
    program:
      command: {command}
    attempts: 1
  next:
    description: Reachable only by a declared exit_code match
    final: true
transitions:
  - from: build
    to: next
    description: No exit_code field — never selected on a non-zero exit
"#,
        command = fixture_command(&program)
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        !result.status.success(),
        "no transition matches the non-zero exit, so the run halts; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "build");
    assert_stderr_contains(&result, "error: program exited with code 1");
}

#[test]
fn run_counted_self_loop_terminates_at_visit_budget() {
    // Regression: under `rhei run`, a counted state that loops to ITSELF
    // (`tick -> tick`) used to spin forever. The orchestrator compared the
    // reloaded raw state (`tick-2`) against the normalized current state
    // (`tick`) and mistook the visit suffix for forward progress, skipping the
    // real transition logic — so `visitCount` never advanced past 2 and the
    // `visitCount >= visits` exit could never fire.
    let plan = r#"# Rhei: Counted Self Loop

## Tasks

### Task 1: Tick
**State:** tick
"#;
    let dir = unique_temp_dir("run-counted-self-loop");
    let program = write_python_agent(&dir, "tick.py", "result('## Result\\n\\nTicked.\\n')\n");
    let machine = format!(
        r#"name: counted-self-loop
version: 1
states:
  tick:
    initial: true
    description: Counted program self-loop
    program:
      command: {command}
    visits: 3
  done:
    description: Done
    final: true
transitions:
  - {{ from: tick, to: tick, condition: visitCount < visits }}
  - {{ from: tick, to: done, condition: visitCount >= visits }}
"#,
        command = fixture_command(&program)
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "done");
}

#[test]
fn run_callback_mode_stops_at_human_review() {
    let plan = r#"# Rhei: Human Review Gate

## Tasks

### Task 1: Aggregate findings
**State:** aggregate
"#;
    let machine = r#"name: human-review-gate
version: 1
states:
  aggregate:
    initial: true
    description: Aggregate findings
  human-review:
    description: Wait for a human decision
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: aggregate
    to: human-review
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir("run-human-review-gate");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "human-review");
    assert!(
        !result.stdout.contains("'human-review' → 'completed'"),
        "run should stop at the gating state; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_callback_mode_waits_for_other_branches_before_halting_at_human_review() {
    let plan = r#"# Rhei: Human Review Barrier

## Tasks

### Task 1: Human gate
**State:** aggregate

### Task 2: Independent cleanup
**State:** work

### Task 3: After approval
**State:** work
**Prior:** Task 1
"#;
    let machine = r#"name: human-review-barrier
version: 1
states:
  aggregate:
    description: Aggregate findings
  work:
    description: Ordinary autonomous work
  human-review:
    description: Wait for a human decision
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: aggregate
    to: human-review
  - from: work
    to: completed
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir("run-human-review-barrier");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "human-review");
    assert_task_state(&plan_path, &machine_path, "2", "completed");
    assert_task_state(&plan_path, &machine_path, "3", "work");
    assert!(
        !result.stdout.contains("Task plan.1 transitioned: 'human-review' → 'completed'"),
        "gating task must not advance autonomously; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Task plan.2 transitioned: 'work' → 'completed'"),
        "independent non-gating work should still complete before the run halts; got:\n{}",
        result.stdout
    );
}

#[test]
fn run_prefers_agent_mode_for_model_declared_workflows_without_falling_back_to_callbacks() {
    let (_dir, ws, machine_path) = create_workspace(
        "run-model-declared-agent-mode",
        "# Rhei: Review Workflow\n",
        &[("task.md", "### Task coordinate: Coordinate review\n**State:** split\n")],
    );

    let machine = r#"name: review-workflow
version: 1
models:
  - codex
states:
  split:
    initial: true
    description: Coordinator
    instructions: Write `{output.overview.path}`.
    outputs:
      - name: overview
        path: runtime/overview.md
  review:
    description: Review
    model: codex
  completed:
    final: true
    description: Done
transitions:
  - from: split
    to: completed
  - from: review
    to: completed
"#;
    fs::write(&machine_path, machine).expect("write machine");

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert!(
        !result.status.success(),
        "run should fail without a configured agent transport\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("no agent configured"),
        "expected explicit missing-agent error; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !result.stderr.contains("Missing required output artifact"),
        "run should not fall back to callback-only output validation; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

#[test]
fn reset_script_agent_team_fixture_restores_initial_state() {
    let (_dir, workspace_path, machine_path) =
        copy_workspace_fixture("reset-script-agent-team", "script-agent-team");
    let source_fixture = fixture_path("script-agent-team");

    let run_result = run_cli("run", &workspace_path, &machine_path, &[]);
    assert_success(&run_result);

    // §FS-rhei-reset.1.2: no terminal here, so the intent is stated explicitly.
    let reset_result = run_cli("reset", &workspace_path, &machine_path, &["-y"]);
    assert_success(&reset_result);

    assert_all_tasks_in_state(&workspace_path, &machine_path, "pending");
    assert!(
        !workspace_path.join("runtime").exists(),
        "reset should remove generated runtime output"
    );

    for task_file in &["01-brief.md", "02-research.md", "03-implementation.md"] {
        let actual = fs::read_to_string(workspace_path.join("tasks").join(task_file))
            .expect("read reset task file");
        let expected = fs::read_to_string(source_fixture.join("tasks").join(task_file))
            .expect("read source fixture task file");
        assert_eq!(actual, expected, "{} should match the checked-in fixture", task_file);
    }
}

#[test]
fn run_partially_advanced_completes_remaining() {
    let plan = r#"# Rhei: Partial Advance

## Tasks

### Task 1: Already done
**State:** completed

### Task 2: Needs work
**State:** draft
**Prior:** Task 1

### Task 3: Also needs work
**State:** draft
**Prior:** Task 2
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("run-partial", plan);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("4 transition(s) made"),
        "expected 4 transitions (2 each for Tasks 2 & 3); got:\n{}",
        result.stdout
    );
}

#[test]
fn run_already_completed_is_noop() {
    let plan = r#"# Rhei: All Done

## Tasks

### Task 1: Done
**State:** completed

### Task 2: Also done
**State:** completed
**Prior:** Task 1
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("run-noop", plan);
    let original = fs::read_to_string(&plan_path).expect("read plan");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "expected no-op message; got:\n{}",
        result.stdout
    );

    let after = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(original, after, "file should be unchanged");
}

#[test]
fn run_parallel_does_not_warn_for_a_ticket_with_subtasks_in_one_file() {
    // §FS-rhei-run.2.5: only top-level tickets count toward a file — a ticket
    // and its subtasks are one schedulable unit, not shared-file concurrency.
    let (_dir, ws, machine_path) = create_workspace(
        "run-parallel-subtasks",
        "# Rhei: Subtask Layout\n**States:** integration-test\n",
        &[
            (
                "one.md",
                "### Task 1: Alpha\n**State:** draft\n\n#### Task 1.1: Detail\n**State:** draft\n",
            ),
            ("two.md", "### Task 2: Beta\n**State:** draft\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert!(
        !result.stderr.contains("schedules tickets from the same rhei file"),
        "a ticket plus its subtasks is not a shared file:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Falling back to sequential"),
        "a multi-file plan keeps parallelism:\n{}",
        result.stderr
    );
}

#[test]
fn run_parallel_warns_when_one_of_several_files_owns_two_tickets() {
    let (_dir, ws, machine_path) = create_workspace(
        "run-parallel-shared",
        "# Rhei: Shared File\n**States:** integration-test\n",
        &[
            (
                "one.md",
                "### Task 1: Alpha\n**State:** draft\n\n### Task 2: Beta\n**State:** draft\n",
            ),
            ("two.md", "### Task 3: Gamma\n**State:** draft\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert!(
        result.stderr.contains("schedules tickets from the same rhei file")
            && result.stderr.contains("one.md"),
        "two top-level tickets in one of several files should warn and name it:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Falling back to sequential"),
        "other files still benefit from parallelism:\n{}",
        result.stderr
    );
}

#[test]
fn run_parallel_falls_back_to_sequential_when_all_tickets_share_one_file() {
    // §FS-rhei-run.2.5: with every ticket in one plan file, parallel slots
    // could only schedule same-file tickets — sequential, as for a bare file.
    let (_dir, ws, machine_path) = create_workspace(
        "run-parallel-single-file",
        "# Rhei: One File\n**States:** integration-test\n",
        &[(
            "one.md",
            "### Task 1: Alpha\n**State:** draft\n\n### Task 2: Beta\n**State:** draft\n",
        )],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert!(
        result.stderr.contains("Falling back to sequential execution"),
        "a single ticket-owning file cannot run in parallel:\n{}",
        result.stderr
    );
}

/// `rhei run` and `rhei next` share one eligibility rule, so the orchestrator
/// schedules a parent only after the subtree it integrates is terminal. The
/// central ledger records the order, which is what proves it.
// §FS-rhei-run.3 §FS-rhei-plan-language.3
#[test]
fn run_schedules_a_parent_only_after_its_subtree_is_terminal() {
    let plan = r#"# Rhei: Parent Scheduling

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: First subtask
**State:** draft

#### Task 1.2: Second subtask
**State:** draft

### Task 2: Dependent
**State:** draft
**Prior:** Task 1
"#;
    let (dir, plan_path, machine_path) = setup_single_file("run-parent-order", plan);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    let ledger = fs::read_to_string(dir.join("runtime").join("state-transitions.log"))
        .expect("read the central transition ledger");
    let position = |line: &str| {
        ledger
            .lines()
            .position(|entry| entry == line)
            .unwrap_or_else(|| panic!("ledger should contain {line:?}; got:\n{ledger}"))
    };
    let parent_done = position("plan.1 pending@completed");
    for child in ["plan.1.1 pending@completed", "plan.1.2 pending@completed"] {
        assert!(
            position(child) < parent_done,
            "the parent must not be stamped terminal before {child}; got:\n{ledger}"
        );
    }
    assert!(
        parent_done < position("plan.2 draft@pending"),
        "the dependent must not start before the parent is terminal; got:\n{ledger}"
    );
}

const PARENT_GATE_MACHINE: &str = r#"name: parent-gate
version: 1
states:
  draft:
    initial: true
    description: Start
  gate:
    description: Waiting on a human decision
    gating: true
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: completed
  - from: gate
    to: completed
"#;

/// The narrowing this rule buys: a parent with an open descendant is neither
/// scheduled nor stamped terminal, so the run cannot leave behind a plan that
/// fails `rhei validate`. The halt report names the parent and what holds it.
///
/// The run exits zero: one gate is holding the whole branch, and a gate is the
/// plan working as authored, not a failure to report.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn run_leaves_a_parent_alone_while_a_descendant_is_gated() {
    let plan = r#"# Rhei: Gated Subtree

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Gated subtask
**State:** gate
"#;
    let dir = unique_temp_dir("run-parent-gated-child");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PARENT_GATE_MACHINE);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "draft");
    let content = fs::read_to_string(&plan_path).expect("read plan after run");
    assert!(
        content.contains("#### Task 1.1: Gated subtask\n**State:** gate"),
        "the gated descendant must be untouched; got:\n{content}"
    );
    assert!(
        combined.contains("Task plan.1 (draft): waiting on open descendant Task plan.1.1 (gate)"),
        "the halt report must name the parent and the descendant holding it; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.1.1 (gate): gating state awaiting review"),
        "the descendant must still report its own cause; got:\n{combined}"
    );
}

/// The same plan plus one dependent of the parent. A `**Prior:**` that resolves
/// to a parent held open by a gated child inherits that gate: the run is
/// waiting on one human decision, not stalled, so it still exits zero and still
/// leaves every ticket where it found it.
///
/// Judging the prior by its own stored state instead saw a non-gating parent
/// with no priors of its own, read the plan as unadvanceable, and exited one.
// §FS-rhei-run.3 §FS-rhei-plan-language.3 §FS-rhei-run-report.3.1
#[test]
fn run_leaves_a_dependent_of_a_gate_held_parent_alone() {
    let plan = r#"# Rhei: Gated Subtree With Dependent

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Gated subtask
**State:** gate

### Task 2: Dependent task
**State:** draft
**Prior:** Task 1
"#;
    let dir = unique_temp_dir("run-parent-gated-child-dependent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PARENT_GATE_MACHINE);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "draft");
    assert_task_state(&plan_path, &machine_path, "2", "draft");
    let content = fs::read_to_string(&plan_path).expect("read plan after run");
    assert!(
        content.contains("#### Task 1.1: Gated subtask\n**State:** gate"),
        "the gated descendant must be untouched; got:\n{content}"
    );
    assert!(
        combined.contains("Task plan.1 (draft): waiting on open descendant Task plan.1.1 (gate)"),
        "the halt report must name the parent and the descendant holding it; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.2 (draft): waiting on Task plan.1"),
        "the dependent must name the parent it waits on; got:\n{combined}"
    );

    // The prediction and the outcome are one judgment: a dry run that exits
    // non-zero where the real run exits zero is not a prediction.
    // §FS-rhei-run-report.3.1
    let dry =
        run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui", "--dry-run"]);
    assert_success(&dry);
}

/// One gate, two siblings: the second child's `**Prior:**` is the first, which
/// is the gated one. Every open ticket in the plan traces back to the same
/// human decision, so the run exits zero and leaves the plan untouched.
///
/// The verdict must not depend on which child the walk reaches first. Sharing
/// one visited set between the descendant walk and the prior walk made it: the
/// gate was consumed by the parent's descendant loop, so the sibling's prior
/// walk saw an already-visited node, scored it as no reason to wait, and the
/// whole branch read as stuck.
// §FS-rhei-run.3 §FS-rhei-plan-language.3 §FS-rhei-run-report.3.1
#[test]
fn run_leaves_a_sibling_of_a_gated_sibling_alone() {
    let plan = r#"# Rhei: Gated Sibling

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Gated subtask
**State:** gate

#### Task 1.2: Waits on the gated subtask
**State:** draft
**Prior:** Task 1.1
"#;
    let dir = unique_temp_dir("run-gated-sibling");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PARENT_GATE_MACHINE);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "draft");
    let content = fs::read_to_string(&plan_path).expect("read plan after run");
    assert!(
        content.contains("#### Task 1.1: Gated subtask\n**State:** gate"),
        "the gated descendant must be untouched; got:\n{content}"
    );
    assert!(
        content.contains("#### Task 1.2: Waits on the gated subtask\n**State:** draft"),
        "the sibling must be untouched; got:\n{content}"
    );
    assert!(
        combined.contains("Task plan.1 (draft): waiting on open descendant Task plan.1.1 (gate)"),
        "the parent must name the descendants holding it; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.1.2 (draft): waiting on Task plan.1.1"),
        "the sibling must name the gated ticket it waits on; got:\n{combined}"
    );

    let dry =
        run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui", "--dry-run"]);
    assert_success(&dry);
}

/// A ticket whose worker finished without moving it must not starve its
/// siblings, and must not be re-spawned into every slot that frees up.
///
/// Both failed in the parallel worker pool. `stalled_tasks` was declared
/// outside the pass loop and never cleared, and the exit-0-with-missing-outputs
/// arm `continue`d past the pool refill — so the slot the stalled worker had
/// just freed stayed idle for the rest of the pass, and the run halted with "no
/// progress" while four ready tickets had never been spawned at all. Sibling
/// arms that also fail to advance (an auto-advance error, a timeout with no
/// rule, a non-zero exit under `--continue-on-error`) did not mark the ticket
/// stalled either, so the live-ready-set refill re-spawned it inside one pass,
/// without bound.
// §FS-rhei-run.3
#[test]
fn a_stalled_ticket_does_not_starve_its_siblings_or_respawn_without_bound() {
    let dir = unique_temp_dir("run-parallel-stall-refill");
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Stall Refill\n").expect("write index");
    for n in 1..=6 {
        fs::write(
            tasks_dir.join(format!("{n:02}-item.md")),
            format!("### Task {n}: Item {n}\n**State:** work\n"),
        )
        .expect("write task file");
    }

    let agent_script = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
append(root / 'runtime' / 'logs' / 'spawns.log', task + '\n')
# Tickets 1 and 2 exit 0 having written nothing: they fail the completion
# condition and stay put. Everyone else finishes.
if task in ('workspace.1', 'workspace.2'):
    sys.exit(0)
write(root / 'runtime' / 'out' / (task + '.md'), 'done\n')
result('## Result\n\nFinished.\n')
"#,
    );
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&agent_script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{ "mock": {{ "command": {command}, "stdin_prompt": true, "timeout": "10s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: stall-refill
version: 1
states:
  work:
    initial: true
    description: Do it
    agent: mock
    agent_timeout: 10s
    concurrent: true
    outputs:
      - name: out
        path: runtime/out/{task_id}.md
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#,
    );

    let result = run_cli(
        "run",
        &workspace,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--parallel", "2"],
    );
    assert!(
        !result.status.success(),
        "two tickets cannot finish, so the run halts non-zero\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // The siblings queued behind the stalled pair were scheduled and finished.
    for n in 3..=6 {
        assert_task_state(&workspace, &machine_path, &n.to_string(), "completed");
    }
    assert_task_state(&workspace, &machine_path, "1", "work");
    assert_task_state(&workspace, &machine_path, "2", "work");

    // Each ticket is spawned at most once per pass. The stalled pair is retried
    // on the second (and last) pass — the set of stalled tickets is scoped to a
    // pass, not written off for the run — and before the fix it was re-spawned
    // into every slot that freed up, without bound, inside pass one.
    let spawns =
        fs::read_to_string(workspace.join("runtime/logs/spawns.log")).expect("read spawn log");
    for n in 1..=6 {
        let id = format!("workspace.{n}");
        let count = spawns.lines().filter(|line| line.trim() == id).count();
        let bound = if n <= 2 { 2 } else { 1 };
        assert!(
            (1..=bound).contains(&count),
            "{id}: spawned {count} time(s), expected between 1 and {bound}\n{spawns}"
        );
    }
}
