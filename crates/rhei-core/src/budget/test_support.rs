//! The world the budget unit suites share: a project root, a witness
//! directory of its own, and a clock the test sets.
//!
//! Every one of them needs all three, and the two environment variables
//! involved are process-global, so a single lock serializes the suites rather
//! than letting one test's clock decide another's day.

use super::*;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// One witness home for the whole process, because the witness base is resolved
/// once per process: a second directory would be set in the environment and
/// never read. The cases stay apart by account uuid instead, which is what keys
/// a witness anyway. §FS-rhei-budgets.5.3
fn witness_home() -> &'static Path {
    static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
    HOME.get_or_init(|| tempfile::tempdir().expect("witness home")).path()
}

/// A project root, its witness directory, and the environment that points one
/// at the other. Dropping it restores whatever the process had before.
pub(super) struct Case {
    _guard: MutexGuard<'static, ()>,
    project: tempfile::TempDir,
    previous_state_home: Option<std::ffi::OsString>,
    previous_clock: Option<std::ffi::OsString>,
    pub account: Account,
    /// The ticket source every binding in this case points at.
    pub source: PathBuf,
}

impl Case {
    pub fn new() -> Self {
        Self::at("2026-09-24T12:00:00Z")
    }

    pub fn at(now: &str) -> Self {
        let guard = env_lock().lock().unwrap_or_else(|poison| poison.into_inner());
        let project = tempfile::tempdir().expect("project root");
        let previous_state_home = std::env::var_os("XDG_STATE_HOME");
        let previous_clock = std::env::var_os(CLOCK_ENV);
        std::env::set_var("XDG_STATE_HOME", witness_home());
        std::env::set_var(CLOCK_ENV, now);
        let source = project.path().join("plan.rhei.md");
        std::fs::write(&source, "# Rhei: Case\n").expect("write ticket source");
        let (account, _) = Account::establish(project.path(), &audit()).expect("establish");
        Self { _guard: guard, project, previous_state_home, previous_clock, account, source }
    }

    pub fn root(&self) -> &Path {
        self.project.path()
    }

    /// Move the case's clock, the way a later run on the same account would
    /// find it. §FS-rhei-budgets.3.3.1
    pub fn set_clock(&self, now: &str) {
        std::env::set_var(CLOCK_ENV, now);
    }

    /// A writable transaction on this account, with the ticket bound.
    pub fn open(&self) -> Journal {
        let mut journal = self.account.open(true).expect("open account");
        let ticket = self.ticket();
        if !journal.identity_installed(&ticket) {
            journal.bind_ticket(&ticket, "plan.1", &self.source, &audit()).expect("bind");
        }
        journal
    }

    pub fn ticket(&self) -> String {
        // One ticket per case is enough for every arithmetic question; the
        // ones about two tickets mint a second explicitly.
        self.account.ticket_identity("11111111-1111-4111-8111-111111111111")
    }

    pub fn journal_path(&self) -> PathBuf {
        self.account.directory().join("journal.jsonl")
    }

    pub fn witness_path(&self) -> PathBuf {
        witness_home().join("rhei/budget-authority").join(self.account.uuid()).join("history.jsonl")
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        restore("XDG_STATE_HOME", self.previous_state_home.take());
        restore(CLOCK_ENV, self.previous_clock.take());
    }
}

fn restore(key: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}

pub(super) fn audit() -> Audit {
    Audit {
        actor: "tester".into(),
        written_at: "2026-09-24T12:00:00Z".into(),
        reason: "unit test".into(),
        argv: vec!["rhei".into(), "run".into()],
    }
}

/// The bounds a case runs under unless it says otherwise: generous enough that
/// only the bound a test is about ever fires.
pub(super) fn bounds(transition_limit: u64, invocations_per_day: u64) -> EffectiveBounds {
    EffectiveBounds { transition_limit, invocations_per_day }
}

pub(super) fn arm(attempt: &str) -> [Arm<'_>; 1] {
    [Arm { attempt_identity: attempt, descendant_envelope: 0 }]
}

pub(super) fn request<'a>(
    case: &'a Case,
    ticket: &'a str,
    arms: &'a [Arm<'a>],
    travel: bool,
) -> AdmissionRequest<'a> {
    AdmissionRequest {
        ticket_identity: ticket,
        display_id: "plan.1",
        project_label: "case",
        execution_root: case.root().to_str().expect("utf-8 root"),
        arms,
        parent_reservation: None,
        travel,
    }
}
