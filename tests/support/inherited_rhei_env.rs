//! One test of this very binary, run again with a `RHEI_*` in the environment
//! that starts it — the way a `rhei run` leaves one for the suite its agent's
//! commit hook runs. §REQ-test-isolation.7
//!
//! Shared by the CLI's two harnesses with `#[path] mod`, like `git_env.rs`.
//! The library's tests keep their own copy in `tests_inherited_rhei_env.rs`.
#![allow(dead_code)]

use std::process::{Command, Output};

/// What a `rhei run` older than agent-grounds/rhei#361 exports to every agent
/// it starts: a reservation, without the account it was minted for, naming
/// nothing any journal a test makes holds. §FS-rhei-budgets.7.1
pub const UNKNOWN_PARENT_RESERVATION: (&str, &str) =
    ("RHEI_BUDGET_PARENT_RESERVATION", "reservation:11b12532-e29c-46d8-a34a-d8e610917167");

/// Run `test` alone in a fresh process of this test binary, with every `RHEI_*`
/// this process inherited taken off and `inherited` put in their place.
///
/// A child rather than `std::env::set_var`: this process's environment is
/// shared by every test running beside this one, and setting it is unsafe
/// besides. The child's environment is the one its harness inherits, which is
/// exactly the seam §REQ-test-isolation.7 governs.
pub fn rerun_this_binary(test: &str, inherited: &[(&str, &str)]) -> Output {
    let exe = std::env::current_exe().expect("the running test binary has a path");
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", test, "--test-threads=1"]);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"RHEI_") {
            cmd.env_remove(key);
        }
    }
    for (key, value) in inherited {
        cmd.env(key, value);
    }
    cmd.output().expect("the test binary runs again")
}

/// The child ran exactly the one test and it passed.
fn passed_alone(output: &Output) -> bool {
    output.status.success()
        && String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed")
}

/// `test` reaches the verdict under `inherited` that it reaches with no
/// `RHEI_*` at all. The clean run is the control: a test that does not pass
/// clean says nothing about the environment. §REQ-test-isolation.7
pub fn assert_verdict_ignores_inherited_rhei_env(test: &str, inherited: &[(&str, &str)]) {
    let clean = rerun_this_binary(test, &[]);
    assert!(
        passed_alone(&clean),
        "{test} must pass alone with no RHEI_* set before its verdict under an inherited one \
         means anything; the filter may name a renamed test.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&clean.stdout),
        String::from_utf8_lossy(&clean.stderr)
    );
    let under = rerun_this_binary(test, inherited);
    let names: Vec<&str> = inherited.iter().map(|(key, _)| *key).collect();
    assert!(
        passed_alone(&under),
        "{test} passes with no RHEI_* set and fails when the environment that starts the suite \
         carries {names:?}. Which rhei run a test belongs to is chosen, never inherited \
         (§REQ-test-isolation.7): every RHEI_* the harness was handed has to stay out of what \
         the test runs.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&under.stdout),
        String::from_utf8_lossy(&under.stderr)
    );
}
