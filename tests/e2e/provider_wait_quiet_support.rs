//! A continuous `rhei run` watched while every ticket it could work is held by a
//! provider wait: what it prints after it says it is sleeping (agent-grounds/rhei#681).
//!
//! The run is in text mode, as an operator or a launcher capturing its stdout
//! sees it, and the watch counts the `Pass N:` headers that follow the sleep
//! line. §FS-rhei-run.5.1 §FS-rhei-run.3.3

use super::provider_limit_support::*;
use super::*;
use std::thread;
use std::time::Duration;

/// The line the run prints once per deadline when it goes to sleep.
pub(super) const SLEEP_LINE: &str = "No ready tasks; sleeping";

/// How long the watch lasts once the sleep line is out. The sleep is cut into
/// 500 ms slices for Ctrl+C, so a run that opens a pass per slice prints about
/// six headers in it; a quiet one prints none.
pub(super) const QUIET_WINDOW: Duration = Duration::from_secs(3);

/// The most `Pass N:` headers the window may hold: an order of magnitude below
/// the six a pass per slice prints, so a slow runner cannot flake it.
pub(super) const QUIET_BOUND: usize = 1;

/// A session-limit refusal whose reset is two hours out, in UTC, so no window
/// of the test can reach it, whatever the time of day. §FS-rhei-agents.2.3
pub(super) fn far_limit_signal() -> String {
    let reset = time::OffsetDateTime::now_utc() + time::Duration::hours(2);
    let (hour, meridiem) = match reset.hour() {
        0 => (12, "am"),
        hour @ 1..=11 => (hour, "am"),
        12 => (12, "pm"),
        hour => (hour - 12, "pm"),
    };
    format!("You've hit your session limit · resets {hour}:{:02}{meridiem} (UTC)", reset.minute())
}

/// A continuous text-mode run of `fixture`'s plan, its stdout and stderr kept
/// as `<name>.stdout` and `<name>.stderr` in the test's directory.
pub(super) fn start_text_run(
    fixture: &ProviderFixture,
    name: &str,
    extra: &[&str],
) -> RunningChild {
    let mut command = rhei_command(fixture.root.join(".home"));
    command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
    command.args(["--no-tui", "--no-dashboard"]).args(extra);
    RunningChild::spawn(&mut command, &fixture.dir, name)
}

/// The `Pass N: k ready, …` headers in `text`.
pub(super) fn pass_headers(text: &str) -> usize {
    text.lines().filter(|line| line.starts_with("Pass ") && line.contains(" ready, ")).count()
}

/// What the run printed in the window after its sleep line.
pub(super) struct Watched {
    /// Everything on stdout, for messages.
    pub stdout: String,
    /// `Pass N:` headers after the first sleep line.
    pub headers: usize,
    /// Sleep lines in the whole of stdout.
    pub sleeps: usize,
    /// How many more times the stand-in agent started during the window.
    pub spawned: usize,
}

impl Watched {
    /// What followed the first sleep line.
    pub fn after_sleep(&self) -> &str {
        self.stdout.split_once(SLEEP_LINE).map(|(_, rest)| rest).unwrap_or_default()
    }

    /// The run printed one sleep line and then no pass and no spawn for as long
    /// as the window lasted. The header count is checked first: it is what the
    /// ticket reports. §FS-rhei-run.5.1
    pub fn assert_quiet(&self, what: &str) {
        assert!(
            self.headers <= QUIET_BOUND,
            "{what}: the run opened {} pass(es) in the {QUIET_WINDOW:?} after it said it was \
             sleeping, at most {QUIET_BOUND} expected, while every ticket was held by a provider \
             wait; after the sleep line it printed:\n{}",
            self.headers,
            self.after_sleep()
        );
        assert_eq!(self.spawned, 0, "{what}: nothing spawns during the wait:\n{}", self.stdout);
        assert_eq!(self.sleeps, 1, "{what}: one sleep line per deadline:\n{}", self.stdout);
    }
}

/// Wait for `run` to say it is sleeping, then watch it for [`QUIET_WINDOW`].
/// `starts` counts the stand-in agent's starts. The run must still be live at
/// the end of the window: a run that halted is not the one the ticket is about.
pub(super) fn watch_the_sleep(
    fixture: &ProviderFixture,
    run: &mut RunningChild,
    name: &str,
    starts: impl Fn() -> usize,
) -> Watched {
    let stdout = fixture.dir.join(format!("{name}.stdout"));
    wait_for("the run to say it is sleeping", run, || {
        stdout_from_file(&stdout).contains(SLEEP_LINE)
    });
    let started = starts();
    thread::sleep(QUIET_WINDOW);
    if let Some(status) = run.child().try_wait().expect("inspect run status") {
        panic!("the run exited {status} during the wait instead of sleeping; {}", run.output());
    }
    let printed = stdout_from_file(&stdout);
    let after = printed.split_once(SLEEP_LINE).map(|(_, rest)| rest).unwrap_or_default();
    Watched {
        headers: pass_headers(after),
        sleeps: printed.matches(SLEEP_LINE).count(),
        spawned: starts() - started,
        stdout: printed,
    }
}

/// The lines in `path`, or none when it does not exist yet.
pub(super) fn line_count(path: &Path) -> usize {
    fs::read_to_string(path).unwrap_or_default().lines().count()
}

/// Interrupt `run` the way a foreground Ctrl+C does and wait for it to exit.
/// §FS-rhei-run.3.2
#[cfg(unix)]
pub(super) fn interrupt(run: &mut RunningChild) -> std::process::ExitStatus {
    let pid = nix::unistd::Pid::from_raw(run.child().id() as i32);
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGINT).expect("send SIGINT to the run");
    let deadline = std::time::Instant::now() + PROVIDER_WAIT_PATIENCE;
    loop {
        if let Some(status) = run.child().try_wait().expect("inspect run status") {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run outlived its interrupt; {}",
            run.output()
        );
        thread::sleep(Duration::from_millis(25));
    }
}
