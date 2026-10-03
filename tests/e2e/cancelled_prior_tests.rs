//! A step whose `**Prior:**` is already cancelled can never become ready
//! (§FS-rhei-states.1.4). `rhei validate` says so while it is still being
//! written (§FS-rhei-validate.4), and when a run halts on it, the console and
//! the run report say the prior is cancelled and name the remedies that exist,
//! never "finish the prior first" (§FS-rhei-run-report.3.1, §FS-rhei-run.4).
//!
//! The fixture is agent-grounds/rhei#409's, cut down: Task 1 completed, review
//! round 2 and its fix (Tasks 2 and 3) cancelled by the supervisor, and review
//! round 3 (Task 4) appended behind all three.

use std::fs;
use std::path::PathBuf;

use super::*;

const ROUNDS_PLAN: &str = r#"# Rhei: A review round appended behind a cancelled one

## Tasks

### Task 1: Implement the change
**State:** completed

### Task 2: Review round 2
**State:** cancelled
**Prior:** Task 1

### Task 3: Fix round 2
**State:** cancelled
**Prior:** Task 2

### Task 4: Review round 3
**State:** review
**Prior:** Task 3, Task 2, Task 1
"#;

/// The rounds machine, with a custom abandonment terminal beside the reserved
/// name. `review` runs a program that writes `review-ran.txt`, so a test can
/// tell whether the run ever spawned Task 4.
fn rounds_workspace(prefix: &str, plan: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let program = write_python_agent(
        &dir,
        "review.py",
        "write(pathlib.Path(env('RHEI_ROOT')) / 'review-ran.txt', 'ran\\n')\n",
    );
    let machine = format!(
        r#"name: rounds
version: 1
states:
  review:
    description: Review the change.
    initial: true
    program:
      command: {command}
  completed:
    description: Done.
    final: true
  cancelled:
    description: Dropped.
    final: true
  abandoned:
    description: Abandoned under another name.
    final: true
    role: cancellation
transitions:
  - from: review
    to: completed
    exit_code: 0
  - from: "*"
    to: cancelled
  - from: "*"
    to: abandoned
"#,
        command = fixture_command(&program),
    );
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    (dir, plan_path, machine_path)
}

/// Hold one surface's line for Task 4 to the substance: the cancelled prior is
/// named as one that can never satisfy it, and the remedies are re-pointing
/// `**Prior:**` (and `**Consumes:**`) at a completed step, or cancelling it.
fn assert_tells_the_truth_about(line: &str, surface: &str) {
    assert!(
        !line.contains("finish the prior"),
        "{surface} advises finishing Task plan.3, which is cancelled and can never finish:\n{line}"
    );
    for needle in [
        "Task plan.3 (cancelled)",
        "can never satisfy",
        "re-point",
        "**Prior:**",
        "**Consumes:**",
        "completed step",
        "cancel Task plan.4",
    ] {
        assert!(line.contains(needle), "{surface} is missing {needle:?}:\n{line}");
    }
}

/// The run the issue describes. It still halts — the step can never run — but
/// neither the console summary nor `runtime/run-report.md` tells the operator
/// to finish a prior that is final. §FS-rhei-run-report.3.1 §FS-rhei-run.4
#[test]
fn a_run_halted_on_a_cancelled_prior_names_it_and_the_remedies_that_exist() {
    let (dir, plan_path, machine_path) = rounds_workspace("cancelled-prior-run", ROUNDS_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);

    assert!(!result.status.success(), "the step can never run, so the run still halts");
    assert!(!dir.join("review-ran.txt").exists(), "Task 4 must not have been spawned");
    let console = result
        .stdout
        .lines()
        .chain(result.stderr.lines())
        .find(|line| line.contains("Task plan.4 (review):"))
        .unwrap_or_else(|| {
            panic!(
                "no console line for Task plan.4\nstdout:\n{}\nstderr:\n{}",
                result.stdout, result.stderr
            )
        });
    assert_tells_the_truth_about(console, "the console summary");

    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    let attention = report
        .split("## Attention")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("an Attention section");
    let row = attention
        .lines()
        .find(|line| line.starts_with("| plan.4 "))
        .unwrap_or_else(|| panic!("no Attention row for plan.4:\n{attention}"));
    assert_tells_the_truth_about(row, "the run report");
}

/// Authoring the same step is accepted — a warning, not a refusal — and the
/// warning names the step, each cancelled prior, and the remedies.
/// §FS-rhei-validate.4
#[test]
fn validate_warns_about_a_step_behind_cancelled_priors() {
    let (_dir, plan_path, machine_path) = rounds_workspace("cancelled-prior-validate", ROUNDS_PLAN);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);

    assert_success(&result);
    let warning = result
        .stdout
        .lines()
        .find(|line| line.contains("can never satisfy"))
        .unwrap_or_else(|| panic!("no cancelled-prior warning; stdout:\n{}", result.stdout));
    for needle in [
        "warning: Task plan.4",
        "Task plan.3 (cancelled)",
        "Task plan.2 (cancelled)",
        "re-point",
        "cancel Task plan.4",
    ] {
        assert!(warning.contains(needle), "missing {needle:?} in:\n{warning}");
    }
}

/// `role: cancellation` is what makes a prior cancelled, not the name.
/// §FS-rhei-validate.4 §FS-rhei-states.1.4
#[test]
fn validate_warns_about_a_prior_in_a_custom_cancellation_role_state() {
    let plan = r#"# Rhei: Behind an abandoned round

## Tasks

### Task 1: Abandoned round
**State:** abandoned

### Task 2: Behind it
**State:** review
**Prior:** Task 1
"#;
    let (_dir, plan_path, machine_path) = rounds_workspace("cancelled-prior-role", plan);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);

    assert_success(&result);
    assert!(
        result.stdout.lines().any(|line| {
            line.contains("can never satisfy")
                && line.contains("Task plan.2")
                && line.contains("Task plan.1 (abandoned)")
        }),
        "no cancelled-prior warning for the custom role; stdout:\n{}",
        result.stdout
    );
}
