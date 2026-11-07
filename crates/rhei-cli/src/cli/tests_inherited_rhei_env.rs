// The library route of agent-grounds/rhei#635. A plan these tests run executes
// in this process, so a scrub on a spawned command cannot reach it: whatever
// `RHEI_*` the harness inherited is read where the code under test reads it.
// Shaped like `tests/support/inherited_rhei_env.rs`, which the two binary
// harnesses share; the library cannot take a module from outside its crate.

// §REQ-test-isolation.7

/// What a `rhei run` older than agent-grounds/rhei#361 exports to every agent it
/// starts: a reservation, without its account, that no journal a test makes holds.
const UNKNOWN_PARENT_RESERVATION: (&str, &str) =
    ("RHEI_BUDGET_PARENT_RESERVATION", "reservation:11b12532-e29c-46d8-a34a-d8e610917167");

/// Run `test` alone in a fresh process of this test binary, every inherited
/// `RHEI_*` taken off and `inherited` put in its place. A child, never
/// `std::env::set_var`: this process's environment belongs to every test
/// beside this one. §REQ-test-isolation.7
fn rerun_this_binary(test: &str, inherited: &[(&str, &str)]) -> std::process::Output {
    let exe = std::env::current_exe().expect("the running test binary has a path");
    let mut cmd = std::process::Command::new(exe);
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

fn passed_alone(output: &std::process::Output) -> bool {
    output.status.success()
        && String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed")
}

/// Under a parent reservation the code cannot resolve, a plan run that reads it
/// from the harness is refused with `nested admission cannot use ancestor`.
#[test]
#[ignore = "red until #635: the in-process plan run reads RHEI_BUDGET_PARENT_RESERVATION from the harness; implement removes this"]
fn an_in_process_plan_run_reaches_its_clean_verdict_under_an_inherited_parent_reservation() {
    let test = "tests::missing_outputs_reschedule_single_invocation_spawns_once_and_keeps_state";
    let clean = rerun_this_binary(test, &[]);
    assert!(
        passed_alone(&clean),
        "{test} must pass alone with no RHEI_* set before its verdict under an inherited one \
         means anything; the filter may name a renamed test.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&clean.stdout),
        String::from_utf8_lossy(&clean.stderr)
    );
    let under = rerun_this_binary(test, &[UNKNOWN_PARENT_RESERVATION]);
    assert!(
        passed_alone(&under),
        "{test} passes with no RHEI_* set and fails when the environment that starts the suite \
         carries {}. Which rhei run a test belongs to is chosen, never inherited \
         (§REQ-test-isolation.7): code a test runs in-process reads no RHEI_* from the \
         harness.\nstdout:\n{}\nstderr:\n{}",
        UNKNOWN_PARENT_RESERVATION.0,
        String::from_utf8_lossy(&under.stdout),
        String::from_utf8_lossy(&under.stderr)
    );
}
