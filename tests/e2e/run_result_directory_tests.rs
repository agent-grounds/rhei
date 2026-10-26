//! A fresh workspace permits the documented direct write to the canonical result.
//! §FS-rhei-programs.2 §FS-rhei-states.3.3

use super::*;

fn direct_result_write(parallel: usize) {
    let dir = unique_temp_dir("program-result-directory");
    let plan = dir.join("plan");
    fs::create_dir_all(plan.join("tasks")).unwrap();
    write_fixture_file(&plan, "index.rhei.md", "# Rhei: Direct result write\n");
    write_fixture_file(
        &plan.join("tasks"),
        "01.md",
        "### Task 1: Write the result file\n**State:** run\n",
    );
    // Use the native shell, without the Python fixture's directory-creating result helper.
    let command = if cfg!(windows) {
        r#"echo done> "%RHEI_RESULT_PATH%""#
    } else {
        r#"echo done > "$RHEI_RESULT_PATH""#
    };
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: result-directory
version: 1
states:
  run:
    initial: true
    program: {}
    program_timeout: {FIXTURE_MARGIN}
  completed:
    final: true
  failed:
    final: true
transitions:
  - from: run
    to: completed
    exit_code: 0
  - from: run
    to: failed
    exit_code: 2
"#,
            serde_json::to_string(command).unwrap()
        ),
    );
    let result_path = plan.join("runtime/results/plan.1.md");
    assert!(!plan.join("runtime").exists(), "premise: fresh directory workspace");
    assert!(!result_path.parent().unwrap().exists(), "premise: no result directory");

    let home = dir.join("home");
    let mut cmd = rhei_command(&home);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("RHEI_") {
            cmd.env_remove(name);
        }
    }
    cmd.env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GIT_CEILING_DIRECTORIES", &*dir)
        .current_dir(&dir)
        .arg("--state-machine")
        .arg(&machine)
        .arg("run")
        .arg(&plan)
        .args(["--no-tui", "--no-dashboard", "--parallel", &parallel.to_string()]);
    let output = cmd.output().expect("run the direct-write program");
    let result = CliRun::from(&output);
    let log_path = plan.join("runtime/logs/task-plan.1-run.log");
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        result.status.success(),
        "direct result write failed: exit={:?}\nresult: {}\nprogram log ({}):\n{}\nstdout:\n{}\nstderr:\n{}",
        result.status.code(),
        result_path.display(),
        log_path.display(),
        log,
        result.stdout,
        result.stderr
    );
    if parallel > 1 {
        assert!(result.stdout.contains("Write the result file (parallel)"), "{}", result.stdout);
        assert!(!result.stderr.contains("Falling back to sequential"), "{}", result.stderr);
    }
    assert_task_state(&plan, &machine, "1", "completed");
    let content = fs::read_to_string(&result_path).expect("canonical result was written");
    assert_eq!(content.lines().collect::<Vec<_>>(), ["done"], "program-authored result");
}

#[test]
fn fresh_program_result_directory_sequential() {
    direct_result_write(1);
}

#[test]
fn fresh_program_result_directory_worker_pool() {
    direct_result_write(2);
}
