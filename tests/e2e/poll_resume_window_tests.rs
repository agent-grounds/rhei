//! A window the scenario's own steps miss is worth nothing: it is discarded, and the proof
//! comes whole from another. §FS-rhei-run.3.5

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::Command;

use super::*;

const MARGIN: Duration = Duration::from_secs(60);

/// Hold one crossing past the plan's deadline. That window must be discarded there, and another
/// must restart strictly before its own deadline. No number of windows is asserted, because a
/// slow runner may miss another, and a window it misses before the crossing leaves the hold
/// armed for the next. §FS-rhei-run.3.5
fn assert_held_window_is_discarded(crossing: Crossing) {
    let proof = held_poll_scenario(&Hold::at(crossing), |workspace, _, _| workspace.snapshot());
    let held = proof.missed.iter().find(|window| window.crossing == crossing);
    let held = held.unwrap_or_else(|| {
        panic!("the window held at {crossing:?} must be discarded there: {proof:#?}")
    });
    let times = &held.times;
    let timed = match crossing {
        Crossing::FirstLook => times.first_look.is_some(),
        Crossing::Stop => times.first_look.is_some() && times.stop.is_some(),
        Crossing::Restart => {
            times.first_look.is_some()
                && times.stop.is_some()
                && times.capture.is_some()
                && times.relaunch.is_some()
        }
    };
    assert!(timed, "every step the held window reached must be timed: {held:#?}");
    assert!(
        proof.proven.restarted_at < proof.proven.deadline,
        "the proving window must restart strictly before its own deadline: {proof:#?}"
    );
}

/// §FS-rhei-run.3.5
#[test]
fn a_window_held_past_its_deadline_before_the_first_look_is_discarded() {
    assert_held_window_is_discarded(Crossing::FirstLook);
}

/// §FS-rhei-run.3.5
#[test]
fn a_window_held_past_its_deadline_across_the_stop_is_discarded() {
    assert_held_window_is_discarded(Crossing::Stop);
}

/// The shape the ticket reports: the launcher returned in time and the test's clock read came late.
/// §FS-rhei-run.3.5
#[test]
fn a_window_held_past_its_deadline_across_the_restart_is_discarded() {
    assert_held_window_is_discarded(Crossing::Restart);
}

/// Times unique to one window and step, so a message that drops one cannot pass.
fn fake_times(window: u32) -> WindowTimes {
    let step = |step: u32| Some(Duration::from_millis(u64::from(1000 * window + step)));
    WindowTimes { first_look: step(1), stop: step(2), capture: step(3), relaunch: step(4) }
}

fn fake_missed(window: u32) -> MissedWindow {
    MissedWindow { crossing: Crossing::Restart, times: fake_times(window) }
}

fn fake_proven() -> WindowEnd {
    WindowEnd::Proven(ProvenWindow { restarted_at: 1, deadline: 2 })
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("")
}

/// §FS-rhei-run.3.5
#[test]
fn the_windows_stop_at_the_first_that_proves() {
    let opened = Cell::new(0);
    let proof = prove_within(
        MARGIN,
        || Duration::ZERO,
        || {
            opened.set(opened.get() + 1);
            match opened.get() {
                3 => fake_proven(),
                window => WindowEnd::Missed(fake_missed(window)),
            }
        },
    );
    assert_eq!(opened.get(), 3, "the third window proved, and no fourth opens");
    assert_eq!(proof.missed.len(), 2, "the proof keeps what it discarded: {proof:#?}");
}

/// Only a crossing reports a miss. A check that panics in a window is a failure of the regression,
/// in its own words, and the loop does not read it as a reason to try again. §FS-rhei-run.3.5
#[test]
fn a_hard_failure_in_a_window_is_not_a_miss() {
    let opened = Cell::new(0);
    let failed = catch_unwind(AssertUnwindSafe(|| {
        prove_within(
            MARGIN,
            || Duration::ZERO,
            || {
                opened.set(opened.get() + 1);
                match opened.get() {
                    1 => WindowEnd::Missed(fake_missed(1)),
                    _ => panic!("a hard check failed"),
                }
            },
        )
    }));
    assert_eq!(opened.get(), 2, "a miss opens the next window, and a failure opens none");
    let panic = failed.expect_err("a hard failure must fail the scenario");
    assert_eq!(panic_message(panic.as_ref()), "a hard check failed");
}

/// The bound is the margin in time, not a count: six 5 s windows do not outlast a 40 s stall.
/// §FS-rhei-run.3.5 §REQ-cross-platform.8
#[test]
fn the_margin_stops_new_windows_and_not_an_open_one() {
    let opened = Cell::new(0);
    let spent = catch_unwind(AssertUnwindSafe(|| {
        prove_within(
            MARGIN,
            || Duration::from_secs(30) * opened.get(),
            || {
                opened.set(opened.get() + 1);
                WindowEnd::Missed(fake_missed(opened.get()))
            },
        )
    }));
    assert_eq!(opened.get(), 2, "window 2 spends the 60 s margin, so no third opens");
    assert!(spent.is_err(), "running out of windows must fail");

    // Window 1 ends at 30 s and window 2, already open, proves at 90 s. That counts.
    let opened = Cell::new(0);
    let proof = prove_within(
        MARGIN,
        || {
            Duration::from_secs(match opened.get() {
                0 => 0,
                1 => 30,
                _ => 90,
            })
        },
        || {
            opened.set(opened.get() + 1);
            match opened.get() {
                2 => fake_proven(),
                window => WindowEnd::Missed(fake_missed(window)),
            }
        },
    );
    assert_eq!(proof.missed.len(), 1, "an open window runs to its end: {proof:#?}");
}

/// §FS-rhei-run.3.5
#[test]
fn running_out_of_windows_panics_with_every_windows_times() {
    let opened = Cell::new(0);
    let spent = catch_unwind(AssertUnwindSafe(|| {
        prove_within(
            MARGIN,
            || Duration::from_secs(30) * opened.get(),
            || {
                opened.set(opened.get() + 1);
                WindowEnd::Missed(fake_missed(opened.get()))
            },
        )
    }));
    let panic = spent.expect_err("running out of windows must fail");
    let message = panic_message(panic.as_ref());
    for window in 1..=2 {
        let line = fake_missed(window).line();
        assert!(message.contains(&line), "the panic must carry {line:?}, and says: {message}");
    }
}

fn snapshot(attempts: usize, visits: u64, deadline: u64, ledger: &str) -> PollSnapshot {
    PollSnapshot {
        attempts,
        state: "triage".into(),
        deadline: Some(deadline),
        visits: Some(visits),
        run_log: String::new(),
        ledger: ledger.into(),
    }
}

const LAUNCHED_AT: u64 = 1_000;

/// A crossing is noticed at once, rather than by waiting out the first look's patience. Enough
/// time for the lost deadline to have come is whole seconds from the launch's own sample: the
/// deadline is at least the poll interval past it. §FS-rhei-run.3.5
#[test]
fn a_first_look_past_the_deadline_misses_its_window() {
    let lawful_deadline = LAUNCHED_AT + POLL_INTERVAL_SECS;
    let ready = snapshot(1, 2, lawful_deadline, "");
    assert_eq!(first_look(&ready, LAUNCHED_AT, lawful_deadline - 1), FirstLook::Ready);
    assert_eq!(first_look(&ready, LAUNCHED_AT, lawful_deadline), FirstLook::Missed);

    let later = snapshot(2, 3, lawful_deadline + POLL_INTERVAL_SECS, "");
    assert_eq!(first_look(&later, LAUNCHED_AT, lawful_deadline), FirstLook::Missed);
}

/// Synthetic negative control: no early process is launched. A first look cannot call a window
/// missed when no deadline could yet have passed, or when the window took a wrong route.
/// §FS-rhei-run.3.5
#[test]
fn a_first_look_cannot_excuse_an_early_attempt_or_a_wrong_route() {
    let lawful_deadline = LAUNCHED_AT + POLL_INTERVAL_SECS;
    let later = snapshot(2, 3, lawful_deadline + POLL_INTERVAL_SECS, "");
    let early = catch_unwind(AssertUnwindSafe(|| {
        first_look(&later, LAUNCHED_AT, lawful_deadline - 1);
    }));
    let panic = early.expect_err("attempt 2 before any deadline could have come was excused");
    assert!(panic_message(panic.as_ref()).contains("early spawn"));

    let wrong = snapshot(2, 3, lawful_deadline + POLL_INTERVAL_SECS, WRONG_ROUTE);
    let wrong = catch_unwind(AssertUnwindSafe(|| {
        first_look(&wrong, LAUNCHED_AT, lawful_deadline);
    }));
    let panic = wrong.expect_err("a wrong route was called a missed window");
    assert!(panic_message(panic.as_ref()).contains("wrong route"));
}

const WRONG_ROUTE: &str = "triage-tool-reports.1 triage@resolved\n";
/// Synthetic evidence's original deadline: 2020-01-01T00:00:10Z.
const SYNTHETIC_DEADLINE: u64 = 1_577_836_810;

/// Synthetic negative control: no early process is launched. The same oracle as
/// `an_early_retry_is_rejected_even_when_observed_after_the_deadline`, on evidence a missed
/// window would otherwise take with it. §FS-rhei-run.3.5
#[test]
fn a_missed_window_is_discarded_only_if_it_shows_no_early_attempt_and_no_wrong_route() {
    let early = PollWorkspace::new();
    evidence::write_synthetic_attempt(&early, 2, "2020-01-01T00:00:09Z");
    let rejected =
        catch_unwind(AssertUnwindSafe(|| discard_missed_window(&early, SYNTHETIC_DEADLINE)));
    let panic = rejected.expect_err("a missed window took a pre-deadline attempt 2 with it");
    assert!(panic_message(panic.as_ref()).contains("early spawn"));

    let wrong = PollWorkspace::new();
    fs::create_dir_all(wrong.project.join("runtime")).expect("create synthetic runtime");
    fs::write(wrong.project.join("runtime/state-transitions.log"), WRONG_ROUTE)
        .expect("write synthetic ledger");
    let rejected =
        catch_unwind(AssertUnwindSafe(|| discard_missed_window(&wrong, SYNTHETIC_DEADLINE)));
    let panic = rejected.expect_err("a missed window took a wrong route with it");
    assert!(panic_message(panic.as_ref()).contains("wrong route"));

    // The other direction: nothing to hold against the window, so it goes.
    let lawful = PollWorkspace::new();
    evidence::write_synthetic_attempt(&lawful, 2, "2020-01-01T00:00:10Z");
    discard_missed_window(&lawful, SYNTHETIC_DEADLINE);
    let overwritten = PollWorkspace::new();
    evidence::write_synthetic_attempt(&overwritten, 3, "2020-01-01T00:00:09Z");
    discard_missed_window(&overwritten, SYNTHETIC_DEADLINE);
    discard_missed_window(&PollWorkspace::new(), SYNTHETIC_DEADLINE);
}

const LINE_CHILD: &str = "RHEI_552_MISSED_LINE_CHILD";

/// The retry would otherwise bury what the panic used to say, and libtest swallows `eprintln!`
/// on a pass. So a passing child test's output must still carry the line. §FS-rhei-run.3.5
#[test]
fn a_missed_windows_line_survives_a_passing_test() {
    let window = fake_missed(1);
    if std::env::var_os(LINE_CHILD).is_some() {
        report_missed_window(&window);
        return;
    }
    let path = module_path!().split_once("::").expect("a crate-qualified module path").1;
    let name = format!("{path}::a_missed_windows_line_survives_a_passing_test");
    let out = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", &name, "--include-ignored"])
        .env_remove("RUST_TEST_NOCAPTURE")
        .env(LINE_CHILD, "1")
        .output()
        .expect("run this test as a child");
    let (shown_out, shown_err) = (stdout(&out), stderr(&out));
    assert!(out.status.success() && shown_out.contains("1 passed"), "{shown_out}\n{shown_err}");
    assert!(
        shown_out.contains(&window.line()) || shown_err.contains(&window.line()),
        "the job log must keep {:?} on a pass\nstdout:\n{shown_out}\nstderr:\n{shown_err}",
        window.line()
    );
}

/// A margin written shorter beside the named one fails the first look through a stall, whatever
/// the retry does. §REQ-cross-platform.8 §FS-rhei-run.3.5
#[test]
fn the_scenarios_patience_is_the_one_fixture_margin() {
    assert_eq!(format!("{}s", TEST_PATIENCE.as_secs()), super::super::FIXTURE_MARGIN);
}
