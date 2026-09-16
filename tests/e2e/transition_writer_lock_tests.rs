//! Cross-platform writer-lock behavior at callback boundaries.
//! §FS-rhei-transition-cmd.3 §REQ-cross-platform.3

use super::*;

/// `on_leave` runs while writer exclusion is still held, but the plan pathname
/// itself stays readable. This is portable; the old dual-lock implementation's
/// mandatory destination lock makes the callback fail on Windows.
#[test]
fn issue_95_on_leave_can_read_the_plan_by_its_current_pathname() {
    let machine = format!(
        r#"name: callback-path-read
version: 1
states:
  pending:
    initial: true
    description: Pending
  working:
    description: Working
  completed:
    final: true
    description: Completed
transitions:
  - from: pending
    to: working
    on_leave: {callback}
  - from: working
    to: completed
"#,
        callback = python_callback_yaml(
            "import json,os,pathlib,sys;p=pathlib.Path(os.environ['RHEI_PLAN_PATH']);s=p.read_text(encoding='utf-8');sys.stdout.write(json.dumps({'success': '**State:** pending' in s}))"
        )
    );
    let dir = unique_temp_dir("transition-callback-path-read");
    let plan = "# Rhei: Callback path read\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n";
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "working"],
    );
    assert!(
        result.status.success(),
        "on_leave must read the locked plan by pathname\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "working");
}
