//! What a strict run does with an exit it finds after a sealed input changed
//! under a worker that started on sealed bytes: a transition with no callback
//! routes normally, and a transition whose callback is refused leaves the exit
//! unrouted, interrupted and uncharged. §FS-rhei-seal.8.5

use std::fs;

use super::strict_inputs_support::*;
use super::*;

/// Two workers run at once; ticket 1 edits the script while ticket 2 is still
/// running. Neither exit has a callback, so both route to `completed`. Ticket 3,
/// which waits on both, is the next start in the rhei and is refused.
// §FS-rhei-seal.8.5 §FS-rhei-run.5
#[test]
fn parallel_workers_route_their_exits_and_the_next_start_is_refused() {
    let tasks = [
        "### Task 1: Edits the script\n**State:** step\n",
        "### Task 2: Runs beside the edit\n**State:** step\n",
        "### Task 3: Waits on both\n**State:** step\n**Prior:** 1, 2\n",
    ];
    let shape = MachineShape { concurrent: true, ..MachineShape::default() };
    let drift = lay_drift("strict-parallel", &tasks, &shape);
    on_ticket(
        &drift.ws,
        1,
        &format!("wait_for('started-2')\n{EDIT_SCRIPT}write(ws / 'edited', 'x')\n"),
    );
    on_ticket(&drift.ws, 2, "write(ws / 'started-2', 'x')\nwait_for('edited')\n");

    let refused = drift.run(&["--strict-inputs", "--parallel", "2"]);
    assert_refused(&refused, "scripts/step.py");
    assert_eq!(task_state(&drift.ws, 1), "completed", "no callback: the exit routes");
    assert_eq!(task_state(&drift.ws, 2), "completed", "no callback: the exit routes");
    assert_eq!(task_state(&drift.ws, 3), "step");
    assert!(!drift.evidence().contains("ticket 3"), "{}", drift.evidence());
}

/// A sequential ticket whose exit transition runs a callback: the program edits
/// the script and exits 0, the callback is the check point, and it is refused.
/// The callback never runs, the ticket stays in `step`, and after `rhei seal`
/// the ticket is resumed and both callbacks run.
// §FS-rhei-seal.8.5 §FS-rhei-run.3.7.6 §FS-rhei-transitions.4.7
#[test]
fn a_refused_callback_leaves_the_exit_unrouted_until_the_change_is_sealed() {
    let dir = unique_temp_dir("strict-callback-log");
    let log = dir.join("callbacks.log");
    let code = format!("open(r'{}', 'a').write('ran\\n')", log.display());
    let callback = python_callback_yaml(&code);
    let shape = MachineShape { on_enter: Some(&callback), ..MachineShape::default() };
    let drift = lay_drift("strict-callback", &[&ticket_chain(2)], &shape);
    on_ticket(&drift.ws, 1, EDIT_SCRIPT);

    let refused = drift.run(&["--strict-inputs"]);
    assert_refused(&refused, "scripts/step.py");
    assert!(!log.exists(), "the refused callback never ran: {:?}", fs::read_to_string(&log).ok());
    assert_eq!(task_state(&drift.ws, 1), "step", "a refused callback leaves the exit unrouted");
    assert_eq!(task_state(&drift.ws, 2), "step");
    assert_eq!(spawned(&drift.ws, "drift.2"), Vec::<String>::new());

    assert_success(&drift.seal());
    assert_success(&drift.run(&["--strict-inputs"]));
    assert_eq!(task_state(&drift.ws, 1), "completed");
    assert_eq!(task_state(&drift.ws, 2), "completed");
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "ran\nran\n",
        "each routed exit ran its callback once"
    );
}
