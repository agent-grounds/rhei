//! A shared root guard degrades where the account cannot hold its lock, and a
//! guard that cannot be taken names what it measured rather than an `os error`
//! that reads like a defect in the plan.
//! §FS-rhei-recover.4.1 §FS-rhei-errors.1.5

// Withdrawing write permission from a directory and putting it back afterwards
// is a Unix facility; the reported case is a mode, not a filesystem.
// §FS-rhei-recover.4.1
#![cfg(unix)]

use super::operator_force_support::{
    recovery_confirmation, recovery_fixture, run_recover_in_terminal,
};
use super::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const PLAN: &str = r#"# Rhei: Unwritable Account State

## Tasks

### Task 1: Only step
**State:** draft
"#;

const MARKER_PATH: &str = ".rhei/forced-recovery.json";

/// The smallest marker the interlock reports a hop from: it reads `hop`, and
/// nothing else on a read path parses the rest. §FS-rhei-recover.4
const MARKER: &str = r#"{"version":1,"hop":{"task_id":"plan.1","from":"draft","to":"pending"}}"#;

/// Withdraw write permission from an account state directory, and refuse to
/// conclude anything unless it bit: `root` ignores the mode, and a case that can
/// still create the lock proves nothing about this ticket.
fn seal(state: &Path) {
    assert!(
        !state.join("rhei").exists(),
        "{} must not hold a guard directory yet, or creating the lock could still succeed",
        state.display()
    );
    fs::set_permissions(state, fs::Permissions::from_mode(0o555))
        .expect("seal the account state directory");
    let probe = state.join("probe");
    if fs::write(&probe, b"probe").is_ok() {
        let _ = fs::remove_file(&probe);
        unseal(state);
        panic!(
            "{} is still writable at mode 0555, so this test cannot stage the ticket's case; \
             do not run the suite as root",
            state.display()
        );
    }
}

/// Put the mode back before any assertion runs, so a failing test still leaves a
/// removable tree behind.
fn unseal(state: &Path) {
    fs::set_permissions(state, fs::Permissions::from_mode(0o755))
        .expect("unseal the account state directory");
}

fn canonical(root: &Path) -> PathBuf {
    rhei_core::platform::canonical_path(root).expect("canonical execution root")
}

/// The lock the guard would take: SHA-256 of the canonical root under the
/// account's state directory, which is what makes it nameable from outside the
/// process. §FS-rhei-recover.4
fn guard_lock(root: &Path, state: &Path) -> PathBuf {
    let key = format!("{:x}", Sha256::digest(canonical(root).as_os_str().as_encoded_bytes()));
    state.join("rhei/root-guards").join(format!("{key}.lock"))
}

/// Every subject a guard failure has to name, so a caller can tell a tool-side
/// write from a defect in their own plan. §FS-rhei-errors.1.5
fn assert_names_the_guard(text: &str, root: &Path, state: &Path, whose: &str) {
    for subject in [
        guard_lock(root, state).display().to_string(),
        canonical(root).display().to_string(),
        "Permission denied".to_owned(),
        "XDG_STATE_HOME".to_owned(),
    ] {
        assert!(text.contains(&subject), "{whose} should name {subject}, and says:\n{text}");
    }
}

/// An execution root and its plan, beside two account state directories: one
/// left writable, so a control run shows the plan itself validates, and one this
/// test seals against writes. §FS-rhei-recover.4.1
struct Fixture {
    // Owns the tree; dropped last, after `unseal` has made it removable.
    _dir: TestDir,
    root: PathBuf,
    plan: PathBuf,
    machine: PathBuf,
    sealed_home: PathBuf,
    writable_home: PathBuf,
}

impl Fixture {
    fn new(prefix: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let root = dir.join("root");
        fs::create_dir_all(&root).expect("execution root");
        let plan = write_fixture_file(&root, "plan.rhei.md", PLAN);
        let machine = write_fixture_file(&root, "states.yaml", STATE_MACHINE);
        // Both homes sit outside the execution root, so sealing one says
        // nothing about the project's own writability.
        let sealed_home = dir.join("sealed-home");
        let writable_home = dir.join("writable-home");
        for home in [&sealed_home, &writable_home] {
            fs::create_dir_all(home.join("state")).expect("account state directory");
        }
        Self { _dir: dir, root, plan, machine, sealed_home, writable_home }
    }

    fn state(&self) -> PathBuf {
        self.sealed_home.join("state")
    }

    fn publish_marker(&self) {
        fs::create_dir_all(self.root.join(".rhei")).expect("marker directory");
        fs::write(self.root.join(MARKER_PATH), MARKER).expect("recovery marker");
    }

    fn validate(&self, home: &Path) -> CliRun {
        let output = rhei_command(home)
            .arg("--state-machine")
            .arg(&self.machine)
            .arg("validate")
            .arg(&self.plan)
            .output()
            .expect("validate command");
        CliRun::from(&output)
    }
}

/// `validate` reads a plan; an account that cannot hold the lock costs the
/// exclusion and says so on stderr, not the validation.
/// §FS-rhei-recover.4.1 §FS-rhei-errors.1.5
#[test]
fn validate_survives_an_account_that_cannot_hold_the_root_guard_lock() {
    let fixture = Fixture::new("unwritable-state-validate");
    let control = fixture.validate(&fixture.writable_home);
    seal(&fixture.state());
    let degraded = fixture.validate(&fixture.sealed_home);
    unseal(&fixture.state());

    // The control is what makes the state directory the only variable.
    assert_success(&control);
    assert!(control.stdout.contains("Validation succeeded"), "control:\n{}", control.stdout);

    assert!(
        degraded.status.success(),
        "a readable, valid plan must validate where only the account state directory is \
         unwritable\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        degraded.status,
        degraded.stdout,
        degraded.stderr
    );
    assert!(degraded.stdout.contains("Validation succeeded"), "stdout:\n{}", degraded.stdout);
    assert_names_the_guard(
        &degraded.stderr,
        &fixture.root,
        &fixture.state(),
        "the degradation warning on stderr",
    );
    // §FS-rhei-validate.6 pins this command's stdout; the warning is not on it.
    for absent in ["root-guards", "root guard", "XDG_STATE_HOME"] {
        assert!(
            !degraded.stdout.contains(absent),
            "stdout must not mention {absent}:\n{}",
            degraded.stdout
        );
    }
}

/// The safety property the lock stood in for does not depend on the lock: an
/// established marker still refuses on the machine where no lock can exist.
/// §FS-rhei-recover.4.1
#[test]
fn a_marked_root_is_still_refused_where_the_lock_cannot_be_created() {
    let fixture = Fixture::new("unwritable-state-marker");
    fixture.publish_marker();
    seal(&fixture.state());
    let refused = fixture.validate(&fixture.sealed_home);
    unseal(&fixture.state());

    assert!(
        !refused.status.success(),
        "a marked root must refuse\nstdout:\n{}\nstderr:\n{}",
        refused.stdout,
        refused.stderr
    );
    assert!(
        refused.stderr.contains("forced recovery pending: plan.1 draft -> pending"),
        "stderr:\n{}",
        refused.stderr
    );
    let recovery =
        format!("rhei recover {}", shell_quote(&canonical(&fixture.root).display().to_string()));
    assert_eq!(
        refused.stderr.matches(&recovery).count(),
        1,
        "exactly one recovery command\nstderr:\n{}",
        refused.stderr
    );
}

/// Exclusive acquisition never degrades — but the refusal names its subject.
/// §FS-rhei-recover.4.1 §FS-rhei-errors.1.5
#[test]
fn recover_refuses_and_names_the_lock_it_could_not_create() {
    let fixture = recovery_fixture("unwritable-state-recover");
    let root = fixture.force.dir.to_path_buf();
    // `run_recover_in_terminal` keeps the account under the root it recovers;
    // only this directory is sealed, so the marker and the plan stay readable
    // and the run lock inside the root can still be taken.
    let state = root.join("home/state");
    fs::create_dir_all(&state).expect("account state directory");
    seal(&state);
    let refused = run_recover_in_terminal(&root, &recovery_confirmation("rollback"));
    unseal(&state);

    let transcript = refused.transcript.replace('\r', "");
    assert!(
        !refused.status.success(),
        "recover must not resolve a marker without exclusion\ntranscript:\n{transcript}"
    );
    assert_names_the_guard(&transcript, &root, &state, "the recover refusal");
    assert!(
        fixture.force.dir.join(MARKER_PATH).exists(),
        "a refused recovery leaves the marker in place"
    );
}
