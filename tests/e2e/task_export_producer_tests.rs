//! A producer may not persist successful terminal completion until every
//! declared export contains non-whitespace text.

use std::fs;

use super::terminal_result_tests::{write_mock_agent_settings, RESULT_MESSAGE};
use super::*;

const PRODUCER_PLAN: &str = r#"# Rhei: Export producer

## Tasks

### Task 1: Publish
**State:** pending
**Provides:** report, notes
"#;

const PRODUCER_MACHINE: &str = r#"name: export-producer
version: 1
states:
  pending:
    initial: true
    description: Working
  completed:
    final: true
    description: Done
  failed:
    final: true
    description: Failed
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: completed
  - from: pending
    to: failed
  - from: pending
    to: cancelled
"#;

fn producer_case(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PRODUCER_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", PRODUCER_MACHINE);
    (dir, plan, machine)
}

fn write_export(dir: &Path, name: &str, body: &str) {
    let root = dir.join("runtime/exports/plan.1");
    fs::create_dir_all(&root).expect("export directory");
    fs::write(root.join(format!("{name}.md")), body).expect("export body");
}

/// The shared transition path batches absent and blank exports before every
/// durable completion effect, and the same edge succeeds after repair.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-transition-cmd.3.3
#[test]
fn transition_refuses_missing_and_blank_exports_before_terminal_persistence() {
    let (dir, plan_path, machine_path) = producer_case("export-producer-transition");
    write_export(&dir, "notes", " \n\t");

    let refused = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "pending",
        "completed",
        RESULT_MESSAGE,
    );
    assert!(
        !refused.status.success(),
        "terminal entry must wait for every export\nstdout:\n{}\nstderr:\n{}",
        refused.stdout,
        refused.stderr
    );
    assert_stderr_contains(&refused, "Task plan.1 cannot enter terminal state 'completed'");
    assert_stderr_contains(&refused, "Missing or blank declared exports");
    assert_stderr_contains(&refused, "report");
    assert_stderr_contains(&refused, "notes");
    let plan = fs::read_to_string(&plan_path).expect("refused plan");
    assert!(plan.contains("**State:** pending"), "state must be unchanged: {plan}");
    assert!(!plan.contains("> **Result:**"), "no result link on refusal: {plan}");
    assert!(!dir.join("runtime/results/plan.1.md").exists(), "no carried result recorded");
    assert!(!dir.join("runtime/state-transitions.log").exists(), "no ledger append");

    write_export(&dir, "report", "Release report\n");
    write_export(&dir, "notes", "No follow-up\n");
    let retried = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "pending",
        "completed",
        RESULT_MESSAGE,
    );
    assert_success(&retried);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
}

/// `complete` is only a front end to the same guarded transition and must not
/// persist its carried result when the producer contract is unmet.
// §FS-rhei-transition-cmd.3.3 §FS-rhei-complete.4
#[test]
fn complete_uses_the_shared_export_guard() {
    let (dir, plan_path, machine_path) = producer_case("export-producer-complete");
    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", RESULT_MESSAGE, "--no-callbacks"],
    );

    assert!(!result.status.success(), "complete must refuse an unfulfilled producer");
    assert_stderr_contains(&result, "Missing or blank declared exports");
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert!(!dir.join("runtime/results/plan.1.md").exists(), "result is not recorded");
}

/// A non-cancelled terminal state's name carries no semantics: `failed` owes
/// the same exports as `completed`.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-states.1.4
#[test]
fn a_custom_failed_terminal_state_still_owes_exports() {
    let (_dir, plan_path, machine_path) = producer_case("export-producer-failed");
    let result =
        run_transition_with_result(&plan_path, &machine_path, "1", "pending", "failed", "Failed");

    assert!(!result.status.success(), "a custom name is not cancellation");
    assert_stderr_contains(&result, "terminal state 'failed'");
    assert_stderr_contains(&result, "Missing or blank declared exports");
    assert_task_state(&plan_path, &machine_path, "1", "pending");
}

/// Reserved cancellation abandons the work contract but retains the terminal
/// result obligation.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-states.1.4
#[test]
fn reserved_cancellation_waives_exports_but_keeps_the_result() {
    let (dir, plan_path, machine_path) = producer_case("export-producer-cancelled");
    let cancelled = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "pending",
        "cancelled",
        "Work abandoned",
    );

    assert_success(&cancelled);
    assert_task_state(&plan_path, &machine_path, "1", "cancelled");
    let result = fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("result file");
    assert!(result.contains("Work abandoned"), "got:\n{result}");
}

/// An agent may write its own result and still be held in the source state;
/// the shared transition must not add a ledger entry or result link.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-run.3
#[test]
fn agent_exit_cannot_complete_a_producer_without_exports() {
    let (dir, plan_path, _machine_path) = producer_case("export-producer-agent");
    let agent = write_python_agent(&dir, "producer.py", "result('worker finished\\n')\n");
    write_mock_agent_settings(&dir, &agent);
    let machine = write_fixture_file(
        &dir,
        "agent-states.yaml",
        &PRODUCER_MACHINE.replace(
            "description: Working",
            "description: Working\n    agent: mock\n    agent_timeout: 10s",
        ),
    );

    let run = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success(), "the producer remains unfinished");
    assert_task_state(&plan_path, &machine, "1", "pending");
    let plan = fs::read_to_string(&plan_path).expect("plan");
    assert!(!plan.contains("> **Result:**"), "no completion link: {plan}");
    assert!(!dir.join("runtime/state-transitions.log").exists(), "no terminal ledger entry");
}

/// Program exits use the same transition executor as agent exits, including a
/// declared zero route into a terminal state.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-run.3
#[test]
fn program_exit_cannot_complete_a_producer_without_exports() {
    let dir = unique_temp_dir("export-producer-program");
    let program =
        write_python_agent(&dir, "producer-program.py", "result('program finished\\n')\n");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PRODUCER_PLAN);
    let command = fixture_command(&program);
    let machine_text = format!(
        r#"name: export-program
version: 1
states:
  pending:
    initial: true
    program:
      command: {command}
  completed:
    final: true
transitions:
  - from: pending
    to: completed
    exit_code: 0
"#
    );
    let machine = write_fixture_file(&dir, "states.yaml", &machine_text);

    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success(), "the producer remains unfinished");
    assert_task_state(&plan, &machine, "1", "pending");
    assert!(!dir.join("runtime/state-transitions.log").exists(), "no terminal ledger entry");
}

/// Callback-only advancement has no worker result to preserve, so refusal
/// precedes both the engine fallback result and the terminal transition.
// §FS-rhei-plan-language.3.12.3 §FS-rhei-run.3
#[test]
fn callback_only_advancement_checks_exports_before_engine_terminal_effects() {
    let (dir, plan_path, machine_path) = producer_case("export-producer-callback-only");

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!run.status.success(), "callback-only completion must be refused");
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert!(!dir.join("runtime/results/plan.1.md").exists(), "no fallback result");
    assert!(!dir.join("runtime/state-transitions.log").exists(), "no ledger entry");
}

/// `on_leave` settles the effective target before the export check, so a
/// redirect cannot smuggle terminal completion past the producer contract.
// §FS-rhei-transition-cmd.3.3
#[test]
fn callback_redirect_checks_exports_against_the_effective_terminal_target() {
    let dir = unique_temp_dir("export-producer-redirect");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PRODUCER_PLAN);
    let callback = python_callback_yaml(
        "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'completed'}))",
    );
    let machine_text = format!(
        r#"name: export-redirect
version: 1
states:
  pending: {{ initial: true }}
  review: {{}}
  completed: {{ final: true }}
transitions:
  - from: pending
    to: review
    on_leave: {callback}
  - from: pending
    to: completed
  - from: review
    to: completed
"#
    );
    let machine = write_fixture_file(&dir, "states.yaml", &machine_text);

    let moved = run_cli(
        "transition",
        &plan,
        &machine,
        &["--task", "1", "--from", "pending", "--to", "review", "--result", RESULT_MESSAGE],
    );
    assert!(!moved.status.success(), "redirected terminal entry must be refused");
    assert_stderr_contains(&moved, "terminal state 'completed'");
    assert_task_state(&plan, &machine, "1", "pending");
    assert!(!dir.join("runtime/results/plan.1.md").exists(), "no carried result");
    assert!(!dir.join("runtime/state-transitions.log").exists(), "no ledger entry");
}

/// Export refusal precedes target-state artifact resolution, so one edge never
/// hides a producer failure behind a later consumer-style input failure.
// §FS-rhei-transition-cmd.3.3
#[test]
fn export_refusal_precedes_target_input_resolution() {
    let (dir, plan_path, _machine_path) = producer_case("export-producer-input-order");
    let machine = write_fixture_file(
        &dir,
        "input-states.yaml",
        r#"name: input-order
version: 1
states:
  pending:
    initial: true
  completed:
    final: true
    inputs:
      - name: approval
        path: runtime/approval.md
transitions:
  - from: pending
    to: completed
"#,
    );

    let result = run_transition_with_result(
        &plan_path,
        &machine,
        "1",
        "pending",
        "completed",
        RESULT_MESSAGE,
    );
    assert!(!result.status.success(), "missing exports refuse the edge");
    assert_stderr_contains(&result, "Missing or blank declared exports");
    assert!(
        !result.stderr.contains("Missing required input artifact: approval"),
        "target inputs are later than exports: {}",
        result.stderr
    );
}

/// The guard runs before `on_enter`; a callback for a terminal state must not
/// observe a state that was refused for missing exports.
// §FS-rhei-transition-cmd.3.3
#[test]
fn export_refusal_precedes_on_enter_callbacks() {
    let dir = unique_temp_dir("export-producer-on-enter-order");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PRODUCER_PLAN);
    let marker = dir.join("on-enter-ran.txt");
    let marker_chars = marker
        .display()
        .to_string()
        .chars()
        .map(|character| u32::from(character).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let callback = python_callback_yaml(&format!(
        "import json,pathlib,sys;p=pathlib.Path(''.join(map(chr,[{marker_chars}])));p.write_text('ran');sys.stdout.write(json.dumps({{'success': True}}))"
    ));
    let machine_text = format!(
        r#"name: on-enter-order
version: 1
states:
  pending: {{ initial: true }}
  completed: {{ final: true }}
transitions:
  - from: pending
    to: completed
    on_enter: {callback}
"#
    );
    let machine = write_fixture_file(&dir, "states.yaml", &machine_text);

    let result = run_cli(
        "transition",
        &plan,
        &machine,
        &["--task", "1", "--from", "pending", "--to", "completed", "--result", RESULT_MESSAGE],
    );
    assert!(!marker.exists(), "on_enter must not run for a refused completion");
    assert!(!result.status.success(), "missing exports refuse the edge");
    assert_task_state(&plan, &machine, "1", "pending");
}

/// Existing state outputs remain the first, more specific contract on a
/// terminal edge that is missing both a state output and a task export.
// §FS-rhei-transition-cmd.3.3 §FS-rhei-states.3
#[test]
fn source_output_refusal_precedes_the_export_refusal() {
    let (dir, plan_path, _machine_path) = producer_case("export-producer-output-order");
    let machine = write_fixture_file(
        &dir,
        "output-states.yaml",
        r#"name: output-order
version: 1
states:
  pending:
    initial: true
    outputs:
      - name: state-report
        path: runtime/state-report.md
  completed:
    final: true
transitions:
  - from: pending
    to: completed
"#,
    );

    let result = run_transition_with_result(
        &plan_path,
        &machine,
        "1",
        "pending",
        "completed",
        RESULT_MESSAGE,
    );
    assert!(!result.status.success(), "missing source output refuses the edge");
    assert_stderr_contains(&result, "Missing required output artifact: state-report");
    assert!(
        !result.stderr.contains("Missing or blank declared exports"),
        "the first contract owns this refusal: {}",
        result.stderr
    );
}

/// Because the check follows `on_leave`, a callback may finish producing the
/// declared files and let the edge continue.
// §FS-rhei-transition-cmd.3.3
#[test]
fn on_leave_may_publish_exports_before_the_terminal_check() {
    let dir = unique_temp_dir("export-producer-on-leave");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PRODUCER_PLAN);
    let export_root = dir.join("runtime/exports/plan.1");
    let root_chars = export_root
        .display()
        .to_string()
        .chars()
        .map(|character| u32::from(character).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let callback = python_callback_yaml(&format!(
        "import json,pathlib,sys;p=pathlib.Path(''.join(map(chr,[{root_chars}])));p.mkdir(parents=True,exist_ok=True);(p/'report.md').write_text('report\\n');(p/'notes.md').write_text('notes\\n');sys.stdout.write(json.dumps({{'success': True}}))"
    ));
    let machine_text = format!(
        r#"name: export-on-leave
version: 1
states:
  pending: {{ initial: true }}
  completed: {{ final: true }}
transitions:
  - from: pending
    to: completed
    on_leave: {callback}
"#
    );
    let machine = write_fixture_file(&dir, "states.yaml", &machine_text);

    let result = run_cli(
        "transition",
        &plan,
        &machine,
        &["--task", "1", "--from", "pending", "--to", "completed", "--result", RESULT_MESSAGE],
    );
    assert_success(&result);
    assert_task_state(&plan, &machine, "1", "completed");
}
