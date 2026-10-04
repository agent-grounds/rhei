//! A `cli:` callback runs under the `callback_timeout` its machine authors, so
//! one hung callback — a network client stuck on its socket under a sync
//! script — cannot hold a transition, or the plan's writer lock, for longer
//! than the author chose. §FS-rhei-transitions.4.10

use std::fs;
use std::path::Path;

use super::callback_timeout_support::*;
use super::*;

/// A callback that is slow on purpose and then succeeds.
const SLOW: &str = r#"import sys
import time

sys.stdin.read()
time.sleep(3)
"#;

/// A callback that asks for its grace: it traps `SIGTERM`, takes a second to
/// clean up, and records that it did.
#[cfg(unix)]
const TRAPS_SIGTERM: &str = r#"import pathlib
import signal
import sys
import time

sys.stdin.read()
marker = pathlib.Path(__file__).with_suffix('.cleaned')


def stop(signum, frame):
    time.sleep(1)
    marker.write_text('cleaned up')
    sys.exit(143)


signal.signal(signal.SIGTERM, stop)
time.sleep(SLEEP)
"#;

/// `cli:<python> <script>` as a YAML scalar.
fn callback(script: &Path) -> String {
    serde_json::to_string(&format!("cli:{}", fixture_command_line(script)))
        .expect("callback should serialize")
}

/// A machine with a gate, two states to move to from it, and a way back, so one
/// plan can take more than one move. `root` goes at the machine's root and
/// `edges` are the rules out of the gate.
fn machine(root: &str, edges: &str) -> String {
    format!(
        r#"name: callback-bound
version: 1
{root}
states:
  gate:
    initial: true
    description: Waiting for the move
  opening:
    description: Moved here by the bounded edge
  review:
    description: Moved here by the sibling edge
  done:
    final: true
    description: Done
transitions:
{edges}
  - from: opening
    to: gate
    description: Back to the gate
  - from: review
    to: gate
    description: Back to the gate
  - from: gate
    to: done
    description: Finished
  - from: opening
    to: done
    description: Finished
  - from: review
    to: done
    description: Finished
"#
    )
}

/// The ticket's case: `on_enter` hangs on a grandchild, under a 2s bound. The
/// move fails as an enter failure — the write rolled back, no ledger row — the
/// grandchild is stopped with it, and the writer lock is free for the next move.
// §FS-rhei-transitions.4.10 §FS-rhei-transition-cmd.3 §FS-rhei-transition-cmd.5
#[test]
fn a_hung_on_enter_callback_is_stopped_at_its_bound_and_rolled_back() {
    let dir = unique_temp_dir("callback-timeout-enter-fixture");
    let (script, grandchild) = hang_fixture(&dir, "hang.py");
    let case = Case::new(
        "callback-timeout-enter",
        &machine(
            "",
            &format!(
                "  - from: gate\n    to: opening\n    description: Open\n    on_enter: {}\n    callback_timeout: 2s\n",
                callback(&script)
            ),
        ),
    );
    let plan_before = fs::read(&case.plan).expect("read plan");
    let ledger_before = case.ledger();

    let moved = case.transition("opening", &[]);

    moved.assert_failed_with("failed: exceeded callback_timeout 2s; its process tree was stopped");
    assert!(moved.run.stderr.contains("on_enter callback 'cli:"), "{}", moved.output());
    assert_eq!(
        fs::read(&case.plan).expect("read plan"),
        plan_before,
        "an enter failure rolls the state write back"
    );
    assert_eq!(case.ledger(), ledger_before, "an enter failure appends no ledger row");
    grandchild.assert_stopped();

    let again = case.transition("opening", &["--no-callbacks"]);
    assert_success(&again.run);
    assert_eq!(current_state(&case.plan), "opening", "the writer lock was released");
}

/// The same hang on `on_leave` rejects the move before the state write.
// §FS-rhei-transitions.4.10 §FS-rhei-transition-cmd.5
#[test]
fn a_hung_on_leave_callback_rejects_the_transition_at_its_bound() {
    let dir = unique_temp_dir("callback-timeout-leave-fixture");
    let (script, grandchild) = hang_fixture(&dir, "hang.py");
    let case = Case::new(
        "callback-timeout-leave",
        &machine(
            "",
            &format!(
                "  - from: gate\n    to: opening\n    description: Open\n    on_leave: {}\n    callback_timeout: 2s\n",
                callback(&script)
            ),
        ),
    );
    let plan_before = fs::read(&case.plan).expect("read plan");
    let ledger_before = case.ledger();

    let moved = case.transition("opening", &[]);

    moved.assert_failed_with(
        "rejected the transition: exceeded callback_timeout 2s; its process tree was stopped",
    );
    assert!(moved.run.stderr.contains("on_leave callback 'cli:"), "{}", moved.output());
    assert_eq!(fs::read(&case.plan).expect("read plan"), plan_before, "the plan is untouched");
    assert_eq!(case.ledger(), ledger_before, "a rejected move appends no ledger row");
    grandchild.assert_stopped();
}

/// The edge's bound wins over the machine's, and an edge with none of its own
/// takes the machine's.
// §FS-rhei-transitions.4.10
#[test]
fn an_edge_bound_wins_over_the_machine_bound() {
    let dir = unique_temp_dir("callback-timeout-precedence-fixture");
    let slow = callback(&write_fixture_file(&dir, "slow.py", SLOW));
    let case = Case::new(
        "callback-timeout-precedence",
        &machine(
            "callback_timeout: 1s",
            &format!(
                "  - from: gate\n    to: opening\n    description: Its own bound\n    on_enter: {slow}\n    callback_timeout: 30s\n  - from: gate\n    to: review\n    description: The machine's bound\n    on_enter: {slow}\n"
            ),
        ),
    );

    let own = case.transition("opening", &[]);
    assert_success(&own.run);
    assert_eq!(current_state(&case.plan), "opening", "the edge's 30s bound governs");
    assert_success(&case.transition("gate", &[]).run);

    let inherited = case.transition("review", &[]);
    inherited.assert_failed_with("on_enter callback 'cli:");
    assert!(
        inherited.run.stderr.contains("exceeded callback_timeout 1s; its process tree was stopped"),
        "an edge with no bound of its own takes the machine's\n{}",
        inherited.output()
    );
    assert_eq!(current_state(&case.plan), "gate", "the enter failure is rolled back");
}

/// No bound anywhere is today's behaviour: a slow callback is waited for. This
/// is the regression guard, and it passes before the change as well as after.
// §FS-rhei-transitions.4.10
#[test]
fn without_a_bound_a_slow_callback_is_waited_for() {
    let dir = unique_temp_dir("callback-timeout-unbounded-fixture");
    let slow = callback(&write_fixture_file(&dir, "slow.py", SLOW));
    let case = Case::new(
        "callback-timeout-unbounded",
        &machine(
            "",
            &format!(
                "  - from: gate\n    to: opening\n    description: Open\n    on_enter: {slow}\n"
            ),
        ),
    );

    let moved = case.transition("opening", &[]);

    assert_success(&moved.run);
    assert_eq!(current_state(&case.plan), "opening");
}

/// `rhei run` fires a program's exit-code edge whose `on_enter` hangs. The
/// transition fails the way a failing callback fails there: the task stays in
/// its source state and the run's output names the bound.
// §FS-rhei-transitions.4.10 §FS-rhei-programs.7.2
#[test]
fn a_hung_callback_on_a_program_exit_edge_fails_the_run_transition() {
    let dir = unique_temp_dir("callback-timeout-run-fixture");
    let (script, grandchild) = hang_fixture(&dir, "hang.py");
    let program = write_python_agent(&dir, "route.py", "sys.exit(0)\n");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Callback bound under run\n\n## Tasks\n\n### Task 1: Route\n**State:** route\n",
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: callback-bound-run
version: 1
states:
  route:
    initial: true
    description: Run the program
    program:
      command: {program}
    program_timeout: 10s
    attempts: 1
  checked:
    description: Routed here by the exit code
    gating: true
  done:
    final: true
    description: Done
transitions:
  - from: route
    to: checked
    description: The program passed
    exit_code: 0
    on_enter: {on_enter}
    callback_timeout: 2s
  - from: checked
    to: done
    description: Finished
"#,
            program = fixture_command(&program),
            on_enter = callback(&script),
        ),
    );

    let run = rhei_bounded("run", &plan, &machine, &["--no-tui"]);

    assert_eq!(
        current_state(&plan),
        "route",
        "a timed-out on_enter leaves the task in its source state\n{}",
        run.output()
    );
    assert!(
        format!("{}{}", run.run.stdout, run.run.stderr).contains("exceeded callback_timeout 2s"),
        "the run's output should name the bound\n{}",
        run.output()
    );
    grandchild.assert_stopped();
}

/// `rhei validate` refuses a bound that is not a duration, or is zero, at both
/// levels, and warns about a bound on an edge with no callback to bound.
// §FS-rhei-validate.4 §FS-rhei-transitions.4.10
#[test]
fn validate_refuses_a_malformed_or_zero_bound_at_either_level() {
    let edge = |bound: &str| {
        format!(
            "  - from: gate\n    to: opening\n    description: Open\n    on_enter: \"cli:exit 0\"\n    callback_timeout: {bound}\n"
        )
    };
    let plain =
        "  - from: gate\n    to: opening\n    description: Open\n    on_enter: \"cli:exit 0\"\n";
    for bound in ["0s", "soon"] {
        for (level, text) in [
            ("machine", machine(&format!("callback_timeout: {bound}"), plain)),
            ("edge", machine("", &edge(bound))),
        ] {
            let case = Case::new(&format!("callback-timeout-validate-{level}"), &text);
            let validated = rhei_bounded("validate", &case.plan, &case.machine, &[]);
            assert!(
                !validated.run.status.success(),
                "a {level}-level callback_timeout of `{bound}` should be refused\n{}",
                validated.output()
            );
            assert!(
                validated.run.stderr.contains("callback_timeout")
                    && validated.run.stderr.contains(&format!("'{bound}'")),
                "the refusal should name the field and the value as authored\n{}",
                validated.output()
            );
        }
    }

    let case = Case::new(
        "callback-timeout-validate-idle",
        &machine(
            "",
            "  - from: gate\n    to: opening\n    description: Open\n    callback_timeout: 2m\n",
        ),
    );
    let validated = rhei_bounded("validate", &case.plan, &case.machine, &[]);
    assert_success(&validated.run);
    let output = format!("{}{}", validated.run.stdout, validated.run.stderr);
    assert!(
        output.lines().any(|line| line.contains("warning")
            && line.contains("callback_timeout")
            && line.contains("gate -> opening")),
        "a bound with no callback to bound should be warned about, naming the edge\n{}",
        validated.output()
    );
}

/// On Linux and macOS a callback is asked to stop before it is made to: it gets
/// `SIGTERM` and a grace in which its cleanup runs. Windows has no `SIGTERM`,
/// which is the declared difference.
// §FS-rhei-transitions.4.10 §REQ-cross-platform.2
#[cfg(unix)]
#[test]
fn a_callback_that_traps_sigterm_gets_its_grace() {
    let dir = unique_temp_dir("callback-timeout-grace-fixture");
    let script = write_fixture_file(&dir, "traps.py", &sleep_for(TRAPS_SIGTERM));
    let marker = script.with_extension("cleaned");
    let case = Case::new(
        "callback-timeout-grace",
        &machine(
            "",
            &format!(
                "  - from: gate\n    to: opening\n    description: Open\n    on_enter: {}\n    callback_timeout: 1s\n",
                callback(&script)
            ),
        ),
    );

    let moved = case.transition("opening", &[]);

    moved.assert_failed_with("exceeded callback_timeout 1s; its process tree was stopped");
    assert!(
        marker.exists(),
        "the callback's SIGTERM handler should have had its grace to clean up\n{}",
        moved.output()
    );
    assert_eq!(current_state(&case.plan), "gate");
}
