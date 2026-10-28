//! A poll worker that breaks the plan on every attempt, with its last attempt
//! ending in a later second than the deadline it was spawned for: the timing
//! agent-grounds/rhei#517's macOS run hit by chance, forced here so the sleep
//! the run takes for that spent bound is announced on every run rather than on
//! some. §FS-rhei-run.3.7.4 §FS-rhei-run.5.1

use std::fs;

use super::python_fixture::fixture_command_with_args;
use super::worker_edit_revert_race_tests::{poll_machine, runs, task_text, workspace};
use super::worker_edit_revert_support::{portable, state_in};
use super::*;

/// The race test's poll culprit, except that the third and last attempt ends
/// 20 ms into the whole second after the one it started in, and records both.
/// It starts no earlier than the second it fell due in, so it exits later than
/// that deadline, and the deadline the spent charge writes is one the run has
/// not announced yet.
const LATE_POLL_CULPRIT: &str = r#"here = pathlib.Path(__file__).parent
runs = here / 'poll-runs.txt'
attempt = len(runs.read_text().splitlines()) + 1 if runs.exists() else 1
append(here / 'ws' / 'tasks' / '01-t.md', '\n#### Visit (poll)\n\nnote\n')
append(runs, 'ran\n')
if attempt == 3:
    started = time.time()
    time.sleep(int(started) + 1.02 - started)
    write(here / 'last-attempt.txt', str(int(started)) + ' ' + str(int(time.time())) + '\n')
sys.exit(75)
"#;

/// Once the reverted attempts have spent `poll.max_attempts`, no run sleeps for
/// the deadline the last charge left: the attempt it would wait for never
/// spawns. The worker pool reloads the plan before its end-of-pass scan, so it
/// meets that deadline in the very pass that spent the bound, whatever the
/// machine's speed. The sequential run meets it only when its next pass starts
/// inside the same second, so it is left to the race test.
// §FS-rhei-run.3.7.4 §FS-rhei-run.5.1 §REQ-bounded-neural-work.1
#[test]
fn no_run_waits_for_a_spent_poll_bound_when_the_last_attempt_crosses_a_second() {
    let dir = unique_temp_dir("worker-edit-late-poll-culprit");
    let script = write_python_agent(&dir, "poll.py", LATE_POLL_CULPRIT);
    let task = "### Task 1: One\n**State:** waiting\n\nBody.\n";
    let machine = poll_machine(&fixture_command_with_args(&script, &[]));
    let (ws, machine) = workspace(&dir, &[("01-t.md", task)], &machine);

    let mut output = String::new();
    let mut stdouts = Vec::new();
    for _ in 0..4 {
        let args = ["--no-tui", "--no-callbacks", "--parallel", "2"];
        let ran = run_cli("run", &ws, &machine, &args);
        output.push_str(&portable(&format!("{}{}\n---\n", ran.stdout, ran.stderr)));
        stdouts.push(ran.stdout);
    }

    // The runs got where the report got, or a quiet run proves nothing.
    assert_eq!(runs(&dir, "poll-runs.txt"), 3, "spawns never exceed max_attempts\n{output}");
    assert!(output.contains("attempt 3 of 3 (poll.max_attempts) spent"), "{output}");
    assert!(
        output.contains("halting Task ws.1 in state 'waiting': 3 attempts spent"),
        "the halt names the culprit\n{output}"
    );
    assert_eq!(state_in(&task_text(&ws, "01-t.md")), "waiting", "no edge fires\n{output}");
    let timing = fs::read_to_string(dir.join("last-attempt.txt")).expect("last attempt timing");
    let seconds: Vec<u64> =
        timing.split_whitespace().map(|second| second.parse().expect("a second")).collect();
    assert!(seconds[1] > seconds[0], "the last attempt crossed a second: {timing}\n{output}");

    // The sleep and the spawn are both told on stdout, in the order they happen.
    let mut after_last =
        stdouts.iter().filter_map(|out| out.find("attempt 3 of 3 (").map(|at| &out[at..]));
    assert!(
        after_last.all(|rest| !rest.contains("sleeping")),
        "no run waits once the bound is spent\n{output}"
    );
}
