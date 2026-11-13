//! What an idle stop leaves on the records a reader consults after the run:
//! `runtime/run.json`, its registry copy and the evidence manifest.
//!
//! An idle stop is the plan working as authored, so all three say `finished`
//! and keep `exit_code: 3`, the one field that tells idle from complete. The
//! same plan run without `--until-idle` is already recorded `finished`; the
//! flag splits the exit code and reclassifies nothing (agent-grounds/rhei#677).
// §FS-rhei-run-headless.2 §FS-rhei-evidence.9.1 §FS-rhei-run-json.5

use super::until_idle_support::*;
use super::*;

/// The issue's shape: a program state whose exit `0` routes the ticket to a
/// gating state, so the run does real work first and then stops idle at a gate.
fn work_then_gate_machine(dir: &Path) -> String {
    let work =
        fixture_command(&write_python_agent(dir, "work.py", "result('## Result\\n\\nDone.\\n')\n"));
    format!(
        r#"name: idle-at-gate
version: 1
states:
  work:
    initial: true
    description: Work, then wait for a person
    program:
      command: {work}
    program_timeout: 2m
  review:
    description: Waiting on a human decision
    gating: true
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: review
    exit_code: 0
  - from: review
    to: completed
"#
    )
}

fn read_json(path: &Path, what: &str) -> serde_json::Value {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {what} at {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {what}: {error}\n{text}"))
}

/// An `--until-idle` run that does its work and stops with every ticket at a
/// gate exits `3`, and run.json, the registry copy and the manifest each record
/// it `finished` with `exit_code: 3` — never `failed`.
// §FS-rhei-run-headless.2 §FS-rhei-evidence.9.1
#[test]
fn until_idle_stop_at_a_gate_is_recorded_finished_with_exit_three() {
    let plan = IdlePlan::new(
        "until-idle-record",
        "## Tasks\n\n### Task 1: Work, then wait for a person\n**State:** work\n\n\
         ### Task 2: The same\n**State:** work\n",
    );
    write_fixture_file(plan.root(), "states.yaml", &work_then_gate_machine(plan.root()));

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "every ticket ends at a gate, so the run stops idle");
    assert_idle_line(&result, "gate", "none");
    assert_task_state(&plan.plan, &plan.machine, "1", "review");
    assert_task_state(&plan.plan, &plan.machine, "2", "review");

    let descriptor = read_json(&plan.root().join("runtime/run.json"), "run.json");
    assert_eq!(descriptor["status"], "finished", "run.json of an idle stop: {descriptor:#}");
    assert_eq!(descriptor["exit_code"], EXIT_IDLE, "run.json keeps the idle code: {descriptor:#}");

    let id = descriptor["id"].as_str().expect("run.json names its run id");
    let registry_path = plan.home.join("state/rhei/runs").join(format!("{id}.json"));
    let registry = read_json(&registry_path, "the registry copy");
    assert_eq!(registry["status"], "finished", "registry copy of an idle stop: {registry:#}");
    assert_eq!(registry["exit_code"], EXIT_IDLE, "registry copy keeps the idle code");

    let manifest_path = plan.root().join("runtime/evidence/runs").join(id).join("manifest.json");
    let manifest = read_json(&manifest_path, "the evidence manifest");
    assert_eq!(manifest["status"], "finished", "manifest of an idle stop: {manifest:#}");
    assert_eq!(manifest["exit_code"], EXIT_IDLE, "the manifest keeps the idle code");
}
