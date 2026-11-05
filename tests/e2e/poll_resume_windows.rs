//! What one window of the resumed-poll proof can come to, and the rules over many
//! windows. A window the scenario's own steps miss is worth nothing. §FS-rhei-run.3.5

use std::cell::Cell;
use std::io::Write as _;

use super::*;

/// The three steps whose own time can carry a window past attempt 1's deadline. §FS-rhei-run.3.5
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Crossing {
    FirstLook,
    Stop,
    Restart,
}

/// One window's steps, each timed on its own. A step it never reached stays `None`.
#[derive(Clone, Debug, Default)]
pub(super) struct WindowTimes {
    pub(super) first_look: Option<Duration>,
    pub(super) stop: Option<Duration>,
    pub(super) capture: Option<Duration>,
    pub(super) relaunch: Option<Duration>,
}

#[derive(Clone, Debug)]
pub(super) struct MissedWindow {
    pub(super) crossing: Crossing,
    pub(super) times: WindowTimes,
}

impl MissedWindow {
    /// The one line a missed window leaves behind, on a pass and in the final panic alike.
    /// §FS-rhei-run.3.5
    pub(super) fn line(&self) -> String {
        let times = &self.times;
        format!(
            "poll-resume window missed at {:?}: first look {:?}, stop {:?}, capture {:?}, relaunch {:?}",
            self.crossing, times.first_look, times.stop, times.capture, times.relaunch
        )
    }
}

/// The window the proof came from.
#[derive(Debug)]
pub(super) struct ProvenWindow {
    pub(super) restarted_at: u64,
    pub(super) deadline: u64,
}

pub(super) enum WindowEnd {
    Proven(ProvenWindow),
    Missed(MissedWindow),
}

/// What a passing scenario hands back: the window that proved, and those it discarded.
#[derive(Debug)]
pub(super) struct ScenarioProof {
    pub(super) missed: Vec<MissedWindow>,
    pub(super) proven: ProvenWindow,
}

/// Run one step of a window, and keep what it cost. §FS-rhei-run.3.5
pub(super) fn timed<T>(cost: &mut Option<Duration>, step: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = step();
    *cost = Some(started.elapsed());
    value
}

/// Open windows until one proves. No new window opens once `elapsed` has reached `budget`,
/// an open window runs to its end, and running out panics naming every window's times. A
/// window's own panic is a failure of the regression, and propagates. §FS-rhei-run.3.5
/// §REQ-cross-platform.8
pub(super) fn prove_within(
    budget: Duration,
    mut elapsed: impl FnMut() -> Duration,
    mut window: impl FnMut() -> WindowEnd,
) -> ScenarioProof {
    let mut missed = Vec::new();
    loop {
        match window() {
            WindowEnd::Proven(proven) => return ScenarioProof { missed, proven },
            WindowEnd::Missed(window) => {
                report_missed_window(&window);
                missed.push(window);
            }
        }
        if elapsed() >= budget {
            let lines: Vec<String> = missed.iter().map(MissedWindow::line).collect();
            panic!(
                "no poll-resume window proved within {budget:?}, and every one was missed:\n{}",
                lines.join("\n")
            );
        }
    }
}

/// The one early-spawn comparison: attempt 2 must not start before the deadline it had to wait
/// for. Each caller reads `started` from its own evidence. §FS-rhei-run.3.5
pub(super) fn assert_started_after_deadline(started: u64, deadline: u64) {
    assert!(
        started >= deadline,
        "early spawn: attempt 2's start sample {started} precedes deadline {deadline}"
    );
}

/// The one wrong-route question. The fixture's program never exits 0, so nothing lawful reaches
/// `resolved`. The plan and the ledger are published apart, so either saying so is enough.
/// §FS-rhei-run.3.5
pub(super) fn took_wrong_route(observed: &PollSnapshot) -> bool {
    observed.state == "resolved" || observed.ledger.contains("triage@resolved")
}

fn assert_no_wrong_route(observed: &PollSnapshot) {
    assert!(!took_wrong_route(observed), "wrong route in a missed window: {observed:#?}");
}

/// What one look at the first run's plan found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FirstLook {
    Waiting,
    Ready,
    Missed,
}

/// Attempt 1's deadline as one look knows it: the snapshot's own while that is still attempt 1's,
/// and otherwise the earliest it could be. The run samples it after the launch began, so
/// `launched_at + interval` cannot be later than the real one. §FS-rhei-run.3.5
pub(super) fn attempt_one_deadline(observed: &PollSnapshot, launched_at: u64) -> u64 {
    match observed.deadline {
        Some(deadline) if observed.visits == Some(2) => deadline,
        _ => launched_at + POLL_INTERVAL_SECS,
    }
}

/// Read one look at the first run. A look past the deadline is noticed at once; a look that
/// finds later attempts has lost the deadline, and is called missed only once `launched_at`
/// is old enough for that deadline to have come. Otherwise it is an early spawn, and a wrong
/// route is never missed. §FS-rhei-run.3.5
pub(super) fn first_look(observed: &PollSnapshot, launched_at: u64, now: u64) -> FirstLook {
    assert_no_wrong_route(observed);
    if observed.attempts > 1 {
        // Attempt 2 had started by `now`, so a `now` before the deadline convicts it.
        assert_started_after_deadline(now, attempt_one_deadline(observed, launched_at));
        return FirstLook::Missed;
    }
    match observed.deadline {
        Some(deadline) if observed.attempts == 1 && observed.visits == Some(2) => {
            if deadline > now {
                FirstLook::Ready
            } else {
                FirstLook::Missed
            }
        }
        _ => FirstLook::Waiting,
    }
}

/// Look at the first run until one look decides, and keep the snapshot that decided: a second
/// read could cross the deadline that this one was judged against. §FS-rhei-run.3.5
pub(super) fn look_at_first_run(
    workspace: &PollWorkspace,
    launched_at: u64,
) -> (FirstLook, PollSnapshot) {
    let mut decided = None;
    wait_for("attempt 1 and its persisted future deadline", || {
        let observed = workspace.snapshot();
        let look = first_look(&observed, launched_at, unix_secs());
        if look == FirstLook::Waiting {
            return false;
        }
        decided = Some((look, observed));
        true
    });
    decided.expect("the wait ended on a look that decided")
}

/// Let a missed window that knows its deadline go. A discard excuses no evidence, so it panics
/// on an attempt 2 that started early ("early spawn") or on a wrong route ("wrong route"), and
/// passes when the evidence no longer names attempt 2. The run is stopped first: a window held
/// past its deadline can leave it alive, and attempt 2 starting under the read. §FS-rhei-run.3.5
pub(super) fn discard_missed_window(workspace: &PollWorkspace, deadline: u64) {
    workspace.stop_quietly();
    assert_no_wrong_route(&workspace.snapshot());
    if let Some(started) = evidence::attempt_two_start(workspace) {
        assert_started_after_deadline(started, deadline);
    }
}

/// End a window as missed at `crossing`, once what it leaves behind excuses nothing.
/// §FS-rhei-run.3.5
pub(super) fn missed_at(
    crossing: Crossing,
    workspace: &PollWorkspace,
    deadline: u64,
    times: WindowTimes,
) -> WindowEnd {
    discard_missed_window(workspace, deadline);
    WindowEnd::Missed(MissedWindow { crossing, times })
}

/// Write a missed window's line to the job log, by a write libtest does not capture on a pass:
/// it captures `eprintln!`, and would bury the line. A log that refuses it is no evidence about
/// the run, so it does not fail the window. §FS-rhei-run.3.5
pub(super) fn report_missed_window(window: &MissedWindow) {
    let _ = writeln!(std::io::stderr(), "{}", window.line());
}

/// Holds one crossing, once, until the plan's persisted deadline has passed. The hold follows
/// the deadline and is not a fixed length, so a wider poll interval moves the crossing and
/// cannot hide it. Only the first window to reach the crossing is held. §FS-rhei-run.3.5
pub(super) struct Hold(Cell<Option<Crossing>>);

impl Hold {
    pub(super) fn nowhere() -> Self {
        Self(Cell::new(None))
    }

    pub(super) fn at(crossing: Crossing) -> Self {
        Self(Cell::new(Some(crossing)))
    }

    /// The scenario says when it reaches each crossing, with the clock read still to come.
    pub(super) fn reached(&self, crossing: Crossing, workspace: &PollWorkspace) {
        if self.0.get() != Some(crossing) {
            return;
        }
        self.0.set(None);
        let mut deadline = None;
        // The first launcher can return before attempt 1 has persisted its retry.
        wait_for("a persisted deadline for the hold to outlast", || {
            deadline = workspace.snapshot().deadline;
            deadline.is_some()
        });
        let deadline = deadline.expect("the wait ended on a persisted deadline");
        while unix_secs() <= deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
