//! What one window of the resumed-poll proof can come to, and the rules over many
//! windows. A window the scenario's own steps miss is worth nothing. §FS-rhei-run.3.5
//!
//! Until #552's fix lands, `prove_within`, `first_look`, `discard_missed_window` and
//! `report_missed_window` hold today's behaviour: one window, no miss, no oracle on a
//! discard, and a line libtest swallows. The tests in `poll_resume_window_tests.rs`
//! fail against them.

use std::cell::Cell;

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

/// Open windows until one proves. No new window opens once `elapsed` has reached `budget`,
/// an open window runs to its end, and running out panics naming every window's times.
/// Today it opens one, and a miss is a panic. §FS-rhei-run.3.5
pub(super) fn prove_within(
    budget: Duration,
    mut elapsed: impl FnMut() -> Duration,
    mut window: impl FnMut() -> WindowEnd,
) -> ScenarioProof {
    let _ = (budget, &mut elapsed);
    match window() {
        WindowEnd::Proven(proven) => ScenarioProof { missed: Vec::new(), proven },
        WindowEnd::Missed(missed) => panic!("the only window was missed: {}", missed.line()),
    }
}

/// What one look at the first run's plan found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FirstLook {
    Waiting,
    Ready,
    Missed,
}

/// Read one look at the first run. A look past the deadline is noticed at once; a look that
/// finds later attempts has lost the deadline, and is called missed only once `launched_at`
/// is old enough for that deadline to have come. Otherwise it is an early spawn, and a wrong
/// route is never missed. Today only a future deadline ends the wait. §FS-rhei-run.3.5
pub(super) fn first_look(observed: &PollSnapshot, launched_at: u64, now: u64) -> FirstLook {
    let _ = launched_at;
    if observed.attempts == 1
        && observed.visits == Some(2)
        && observed.deadline.is_some_and(|deadline| deadline > now)
    {
        FirstLook::Ready
    } else {
        FirstLook::Waiting
    }
}

/// Let a missed window that knows its deadline go. A discard excuses no evidence, so it panics
/// on an attempt 2 that started early ("early spawn") or on a wrong route ("wrong route"), and
/// passes when the evidence no longer names attempt 2. Today it checks nothing. §FS-rhei-run.3.5
pub(super) fn discard_missed_window(workspace: &PollWorkspace, deadline: u64) {
    let _ = (workspace, deadline);
}

/// Write a missed window's line to the job log, by a write libtest does not capture on a pass.
/// Today it uses `eprintln!`, which libtest does capture. §FS-rhei-run.3.5
pub(super) fn report_missed_window(window: &MissedWindow) {
    eprintln!("{}", window.line());
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
