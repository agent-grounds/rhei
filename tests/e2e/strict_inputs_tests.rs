//! `rhei run --strict-inputs` refuses to start a ticket whose rhei's sealed
//! inputs changed, starts nothing for it and charges nothing, names the file,
//! and goes on once `rhei seal` accepts the change. Ported from the triage
//! reproducer of #521, whose two cases are a script edited mid-run and a
//! machine edited between two runs. §FS-rhei-run.3.8

use std::fs;

use super::strict_inputs_support::*;
use super::*;

/// Ticket 1's program rewrites `scripts/step.py`. Ticket 2, in the same run,
/// is not started: it stays in `step` with no log and no spawn record, and the
/// run exits 1 naming the file and `rhei seal`. Ticket 1's exit has no
/// callback, so it routes normally. After `rhei seal` the next strict run
/// starts ticket 2 as its first attempt, on the accepted bytes.
// §FS-rhei-seal.8.2 §FS-rhei-seal.8.4 §FS-rhei-seal.8.5
#[test]
#[ignore = "red until #521: a script edited mid-run is executed by the next ticket; implement removes this"]
fn a_script_edited_mid_run_refuses_the_next_ticket_until_it_is_resealed() {
    let drift = lay_drift("strict-script", &[&ticket_chain(2)], &MachineShape::default());
    on_ticket(&drift.ws, 1, EDIT_SCRIPT);

    let refused = drift.run(&["--strict-inputs"]);
    assert_refused(&refused, "scripts/step.py");
    assert!(
        refused.stderr.contains("modified"),
        "the change is named by kind:\n{}",
        refused.stderr
    );
    assert!(drift.generation(1).is_file(), "the run sealed the rhei before its first spawn");
    assert_eq!(task_state(&drift.ws, 1), "completed", "an exit with no callback routes normally");
    assert_eq!(task_state(&drift.ws, 2), "step", "the refused ticket stays where it was");
    assert!(
        !drift.evidence().contains("mark=edited"),
        "nothing executed the edited script:\n{}",
        drift.evidence()
    );
    assert_eq!(spawned(&drift.ws, "drift.2"), Vec::<String>::new(), "a refusal starts nothing");

    assert_success(&drift.seal());
    assert_success(&drift.run(&["--strict-inputs"]));
    assert_eq!(task_state(&drift.ws, 2), "completed");
    assert!(drift.evidence().contains("ticket 2 mark=edited"), "{}", drift.evidence());
    let record = read_json(&drift.ws.join("runtime/spawns/task-drift.2-step.json"));
    assert_eq!(record["attempt"], 1, "the refusal charged no attempt: {record:#}");
}

/// The run seals, completes, and the machine is edited before the next run.
/// The second strict run refuses the new ticket and names `states.yaml`; a third
/// run without the flag executes the edit, because nothing changes without it.
// §FS-rhei-seal.8.1 §FS-rhei-seal.8.4
#[test]
#[ignore = "red until #521: a machine edited between runs is executed by the next run; implement removes this"]
fn a_machine_edited_between_runs_is_refused_and_only_a_non_strict_run_executes_it() {
    let drift = lay_drift("strict-machine", &[&ticket_chain(2)], &MachineShape::default());

    let first = drift.run(&["--strict-inputs"]);
    assert_success(&first);
    assert!(drift.generation(1).is_file(), "the first strict run took generation 1");

    let machine = drift.ws.join("states.yaml");
    let text = fs::read_to_string(&machine).unwrap();
    fs::write(&machine, text.replace("laid-machine", "edited-machine")).unwrap();
    let tasks = drift.ws.join("tasks/01-drift.md");
    let mut plan = fs::read_to_string(&tasks).unwrap();
    plan.push_str("### Task 3: A later run of the same plan\n**State:** step\n**Prior:** 1\n");
    fs::write(&tasks, plan).unwrap();

    let refused = drift.run(&["--strict-inputs"]);
    assert_refused(&refused, "states.yaml");
    assert_eq!(task_state(&drift.ws, 3), "step");
    assert!(!drift.evidence().contains("argv=edited-machine"), "{}", drift.evidence());
    assert_eq!(spawned(&drift.ws, "drift.3"), Vec::<String>::new());

    assert_success(&drift.run(&[]));
    assert!(
        drift.evidence().contains("ticket 3 mark=laid argv=edited-machine"),
        "without the flag the edit runs as it always has:\n{}",
        drift.evidence()
    );
}

/// The `--json` refusal is a record a script can read without parsing prose:
/// `seal_halt` with a stable `reason_code`, the rhei, the ticket, the
/// generation compared, and each changed path with its kind.
// §FS-rhei-seal.8.4 §FS-rhei-run-json.2.1
#[test]
#[ignore = "red until #521: a refusal has no machine-readable record; implement removes this"]
fn a_json_refusal_carries_a_stable_reason_code_and_the_changed_paths() {
    let drift = lay_drift("strict-json", &[&ticket_chain(2)], &MachineShape::default());
    on_ticket(&drift.ws, 1, EDIT_SCRIPT);

    let refused = drift.run(&["--strict-inputs", "--json"]);
    assert_eq!(
        refused.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        refused.stdout,
        refused.stderr
    );
    let halts: Vec<serde_json::Value> = refused
        .stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == "seal_halt")
        .collect();
    assert_eq!(halts.len(), 1, "one seal_halt record\nstdout:\n{}", refused.stdout);
    let halt = &halts[0];
    assert_eq!(halt["reason_code"], "sealed_input_changed", "{halt:#}");
    assert_eq!(halt["rhei"], "drift", "{halt:#}");
    assert_eq!(halt["task"], "drift.2", "{halt:#}");
    assert_eq!(halt["generation"], 1, "{halt:#}");
    assert_eq!(
        halt["changes"],
        serde_json::json!([{ "path": "scripts/step.py", "kind": "modified" }]),
        "{halt:#}"
    );
}

/// The guard on the opt-in: without `--strict-inputs` the run behaves as it
/// always has. The edited script runs, the run exits 0, and no seal is written.
/// It passes before the change and after it.
// §FS-rhei-run.3.8 §REQ-bounded-neural-work.2
#[test]
fn without_the_flag_an_edited_script_runs_and_nothing_is_sealed() {
    let drift = lay_drift("strict-off", &[&ticket_chain(2)], &MachineShape::default());
    on_ticket(&drift.ws, 1, EDIT_SCRIPT);

    assert_success(&drift.run(&[]));
    assert_eq!(task_state(&drift.ws, 2), "completed");
    assert!(drift.evidence().contains("ticket 2 mark=edited"), "{}", drift.evidence());
    assert!(!drift.ws.join("runtime/seal").exists(), "a non-strict run writes no seal");
}
