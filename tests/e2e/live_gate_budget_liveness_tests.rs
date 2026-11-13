//! A gated task left by a dead scheduler cannot establish live readiness.
//! §FS-rhei-transition-cmd.3.4 §REQ-cross-platform.4.1

use super::live_gate_budget_support::*;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn exited_run_is_not_ready(project: bool) {
    let fixture = BudgetFixture::new(project);
    let input = if project { &*fixture.dir } else { &fixture.input };
    fs::create_dir_all(fixture.dir.join("runtime/run.json")).unwrap();
    let mut live = fixture.start(input, "exited-readiness");
    wait_for_budget_run(&mut live, "initial live gate", || {
        fixture.gated() && fixture.visits() == 2
    });
    // Reproduce the scheduler exit caused by the unsafe publication window.
    // Waiting for exit makes the readiness check independent of scheduling.
    fs::write(&fixture.machine, "").unwrap();
    let status = live.wait_for_exit("empty machine rejection");
    assert!(!status.success());
    let output = live.output();
    assert!(output.contains("the document must be a mapping"), "{output}");
    assert!(fixture.gated() && fixture.visits() == 2, "stale readiness must still hold");

    let readiness = catch_unwind(AssertUnwindSafe(|| {
        wait_for_budget_run(&mut live, "a live gate release", || {
            fixture.gated() && fixture.visits() == 2
        });
    }));
    let failure = readiness.expect_err(&format!(
        "accepted exited run as live readiness (project={project}, {status}); {output}"
    ));
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .expect("readiness failure must explain the exit");
    assert!(
        message.contains(&format!("{status:?}")) || message.contains(&status.to_string()),
        "missing observed exit status: {message}"
    );
    assert!(message.contains(&output), "missing scheduler stdout/stderr: {message}");
}

#[test]
#[ignore = "red until #702: observed standalone exit must reject live readiness; implement removes this"]
fn issue_702_standalone_readiness_rejects_an_exited_run() {
    exited_run_is_not_ready(false);
}

#[test]
#[ignore = "red until #702: observed project exit must reject live readiness; implement removes this"]
fn issue_702_project_readiness_rejects_an_exited_run() {
    exited_run_is_not_ready(true);
}
