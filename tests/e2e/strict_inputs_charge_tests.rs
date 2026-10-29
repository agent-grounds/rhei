//! The charge half of a refused exit: whichever worker made it, with or without
//! `--parallel`, an exit whose callback a strict run refuses is recorded
//! interrupted and uncharged, and its slot is not released as completed.
//! §FS-rhei-seal.8.5

use std::fs;
use std::path::Path;

use super::*;

/// The one spawn record of ticket `<id>` under the rhei at `ws`.
pub fn spawn_record(ws: &Path, id: &str) -> serde_json::Value {
    let name = spawned(ws, id)
        .into_iter()
        .find(|name| name.ends_with(".json"))
        .unwrap_or_else(|| panic!("no spawn record for {id}"));
    read_json(&ws.join("runtime").join("spawns").join(name))
}

/// Assert the attempt of `<id>` was given back and its exit left unrouted.
pub fn assert_uncharged_interrupted(ws: &Path, id: &str) {
    let record = spawn_record(ws, id);
    assert_eq!(
        (record["attempt_charged"].as_bool(), record["ending"].as_str()),
        (Some(false), Some("interrupted")),
        "{id}: a refused exit is interrupted and uncharged: {record:#}"
    );
}

/// The `slot_released` outcome of `<task>` in a `--json` run's stdout.
fn released_outcome(stdout: &str, task: &str) -> String {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|record| record["event"] == "slot_released" && record["task"] == task)
        .map(|record| record["outcome"].to_string())
        .unwrap_or_else(|| panic!("no slot_released for {task} in:\n{stdout}"))
}

fn never_ran_callback() -> (TestDir, std::path::PathBuf, String) {
    let dir = unique_temp_dir("strict-charge-log");
    let log = dir.join("callbacks.log");
    let callback =
        python_callback_yaml(&format!("open(r'{}', 'a').write('ran\\n')", log.display()));
    (dir, log, callback)
}

/// Two program workers run at once and both exit after ticket 1 edits the
/// script. Each exit edge carries a callback, so both are refused: both tickets
/// stay in `step`, and neither attempt is charged.
// §FS-rhei-seal.8.5 §FS-rhei-run.5
#[test]
fn parallel_exits_whose_callbacks_are_refused_are_uncharged() {
    let (_log_dir, log, callback) = never_ran_callback();
    let tasks = [
        "### Task 1: Edits the script\n**State:** step\n",
        "### Task 2: Runs beside the edit\n**State:** step\n",
    ];
    let shape = MachineShape { concurrent: true, on_enter: Some(&callback) };
    let drift = lay_drift("strict-charge-parallel", &tasks, &shape);
    on_ticket(
        &drift.ws,
        1,
        &format!("wait_for('started-2')\n{EDIT_SCRIPT}write(ws / 'edited', 'x')\n"),
    );
    on_ticket(&drift.ws, 2, "write(ws / 'started-2', 'x')\nwait_for('edited')\n");

    let refused = drift.run(&["--strict-inputs", "--parallel", "2"]);
    assert_refused(&refused, "scripts/step.py");
    assert!(!log.exists(), "no refused callback ran: {:?}", fs::read_to_string(&log).ok());
    for (n, id) in [(1, "drift.1"), (2, "drift.2")] {
        assert_eq!(task_state(&drift.ws, n), "step", "{id}: the exit is left unrouted");
        assert_uncharged_interrupted(&drift.ws, id);
    }
}

/// An agent (a fake target) edits a sealed script and exits 0; the edge it
/// selects carries a callback, which is refused. The ticket stays in `work`,
/// the attempt is uncharged and interrupted, and the slot is not released as
/// completed.
// §FS-rhei-seal.8.5 §FS-rhei-run-json.2.1
#[test]
fn an_agent_exit_whose_callback_is_refused_is_uncharged() {
    let (log_dir, log, callback) = never_ran_callback();
    let drift = lay_drift("strict-charge-agent", &[&ticket_chain(1)], &MachineShape::default());
    let agent = write_python_agent(
        &log_dir,
        "agent.py",
        &format!(
            "ws = pathlib.Path(r'{}')\n{EDIT_SCRIPT}\
             write(ws / 'runtime' / 'results' / (env('RHEI_TASK_ID') + '.md'), 'done\\n')\n",
            drift.ws.display()
        ),
    );
    write_fixture_file(
        &drift.ws,
        "states.yaml",
        &format!(
            "name: drift-agent\nversion: 1\nstates:\n  step:\n    initial: true\n    description: \
             Work\n    target: fake:acme:model-a\n  completed:\n    final: true\n    description: \
             Done\ntransitions:\n  - from: step\n    to: completed\n    on_enter: {callback}\n"
        ),
    );
    let settings = drift.ws.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("settings directory");
    let command = fixture_command(&agent);
    write_fixture_file(
        &settings,
        "settings.json",
        &format!(
            r#"{{"agents": {{"fake": {{"command": {command}, "stdin_prompt": true, "model_flag": "--model", "timeout": "20s"}}}}}}"#
        ),
    );

    let refused = drift.run(&["--strict-inputs", "--json"]);
    assert_eq!(refused.status.code(), Some(1), "a refusal exits 1\n{}", refused.stderr);
    assert!(
        refused.stdout.contains(
            r#""changes":[{"kind":"modified","path":"scripts/step.py"}],"event":"seal_halt""#
        ),
        "the refusal is a seal_halt naming the sealed path:\n{}",
        refused.stdout
    );
    assert!(!log.exists(), "the refused callback never ran: {:?}", fs::read_to_string(&log).ok());
    assert_eq!(task_state(&drift.ws, 1), "step", "a refused callback leaves the exit unrouted");
    assert_uncharged_interrupted(&drift.ws, "drift.1");
    assert_ne!(released_outcome(&refused.stdout, "drift.1"), "\"completed\"");
}
