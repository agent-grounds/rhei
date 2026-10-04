// Every root guard and budget witness of this test binary resolves under one state
// dir of its own, pinned before any test runs: no test moving `XDG_STATE_HOME`
// moves them for a sibling, and no new test has to opt in. §REQ-test-isolation.6

/// One directory per process; each guard and witness creates its own beneath it.
fn state_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("rhei-cli-test-state-{}", std::process::id()))
}

/// Runs at process start, before the harness spawns a single test. It only names
/// the directory: std's runtime is not up yet, so it touches neither the filesystem
/// nor anything that asks for the current thread. §REQ-test-isolation.6
#[ctor::ctor]
fn pin_the_state_dir() {
    rhei_core::root_access::pin_state_dir_for_tests(state_dir());
}

/// The directory outlives every guard, so it goes only when the process does.
#[ctor::dtor]
fn remove_the_state_dir() {
    let _ = std::fs::remove_dir_all(state_dir());
}
