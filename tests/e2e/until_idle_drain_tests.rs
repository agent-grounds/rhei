//! That a selected run *drains* before it returns, and what `--rhei` narrows.
//!
//! The option suppresses the sleep; it does not stop after one scan. A single
//! scan would leave ready work behind and make the result depend on scheduler
//! timing, so a timer at interval *T* would need *N·T* to walk an *N*-step
//! chain of immediately-completing programs.
// §FS-rhei-run.3 §FS-rhei-run.5 §FS-rhei-panta.6.1

use super::until_idle_support::*;
use super::*;

/// (10) An A→B→C chain of immediately-completing programs finishes in **one**
/// invocation. Each completion makes the next task ready before the run can
/// reach a stopping decision, so nothing is left for a second call.
// §FS-rhei-run.3
#[test]
fn until_idle_drains_a_dependency_chain_in_one_invocation() {
    let plan = IdlePlan::new(
        "until-idle-drain-chain",
        "## Tasks\n\n\
         ### Task 1: A\n**State:** work\n\n\
         ### Task 2: B\n**State:** work\n**Prior:** Task 1\n\n\
         ### Task 3: C\n**State:** work\n**Prior:** Task 2\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, 0, "the whole chain finished, so the run is complete rather than idle");
    assert_all_tasks_in_state(&plan.plan, &plan.machine, "done");
}

/// (11) A parallel slot freed by a finished worker is refilled before any stop
/// decision is reachable, and the run then returns idle on what is genuinely
/// left: both work tickets ran in one invocation and the gate is what remains.
// §FS-rhei-run.5 §FS-rhei-run.3
#[test]
fn a_dependent_made_ready_by_its_prior_runs_before_the_idle_return() {
    let plan = IdlePlan::new(
        "until-idle-refill",
        "## Tasks\n\n\
         ### Task 1: First\n**State:** work\n\n\
         ### Task 2: Made ready by the first\n**State:** work\n**Prior:** Task 1\n\n\
         ### Task 3: Waiting on a reviewer\n**State:** gate\n",
    );

    let result = plan.run(&["--parallel", "2", "--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "only the gate is left, and a gate is a deliberate wait");
    assert_task_state(&plan.plan, &plan.machine, "1", "done");
    assert_task_state(&plan.plan, &plan.machine, "2", "done");
    assert_task_state(&plan.plan, &plan.machine, "3", "gate");
    assert_idle_line(&result, "gate", "none");
}

/// (16) `--rhei` narrows the candidate set, not the causal walk. The in-scope
/// ticket is blocked by a prior in another rhei that is itself deliberately
/// waiting, so the run is idle — but that prior's deadline contributes no next
/// attempt, because its expiry cannot make a ticket this command may not run
/// into a candidate.
// §FS-rhei-panta.6.1 §FS-rhei-run.5.1
#[test]
fn an_out_of_scope_timed_prior_names_the_wait_and_reports_no_next_attempt() {
    let dir = unique_temp_dir("until-idle-out-of-scope");
    let machine = write_fixture_file(&dir, "states.yaml", &idle_machine(&dir));
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    fs::write(project.join("index.panta.md"), "# Panta: Until Idle\n").expect("project manifest");
    fs::write(
        project.join("alpha.rhei.md"),
        format!(
            "# Rhei: Alpha\n\n---\nmetadata:\n  tasks:\n    1:\n      pollNextAttemptAt:\n        \
             poll: {FUTURE_POLL}\n---\n\n## Tasks\n\n### Task 1: Retries later\n**State:** poll\n"
        ),
    )
    .expect("out-of-scope rhei");
    fs::write(
        project.join("beta.rhei.md"),
        "# Rhei: Beta\n\n## Tasks\n\n### Task 1: Waits on Alpha\n**State:** work\n\
         **Prior:** Task alpha.1\n",
    )
    .expect("in-scope rhei");

    let mut command = rhei_command(dir.join(".home"));
    command.arg("--state-machine").arg(&machine).arg("run").arg(&project).args([
        "--rhei",
        "beta",
        "--until-idle",
        "--json",
        "--no-dashboard",
    ]);
    let result = bounded(command, "rhei run --rhei beta --until-idle", &dir);

    assert_exit(&result, EXIT_IDLE, "the in-scope ticket waits on a prior that waits on a clock");
    assert_eq!(
        stop_payload(&result)["next_attempt_at"],
        serde_json::Value::Null,
        "a deadline outside the candidate scope wakes this command for nothing"
    );
    assert!(
        !result.stdout.contains(&instant(FUTURE_POLL)),
        "the out-of-scope deadline must not be published; got:\n{}",
        result.stdout
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("outside the --rhei scope"),
        "the out-of-scope wait is named as such; got:\n{combined}"
    );
}
