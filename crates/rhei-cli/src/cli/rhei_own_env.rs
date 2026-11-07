// Rhei's own variables, as this process inherited them.
//
// The shipped binary reads them from its environment, which is how a nested run
// learns the reservation it is admitted under and a headless child learns it is
// one. This crate's own tests run that same code in the test process, whose
// environment is whatever started `cargo test` — under a `rhei run`, that run's
// — so there the read finds nothing: a test belongs to no rhei run but the ones
// it starts, and one that needs a variable hands it to the code under test
// itself. One reader, so no copy of the read can be the one left inheriting.

/// One of rhei's own `RHEI_*` variables from the environment this process was
/// started with; in this crate's unit tests, never the harness's.
/// §REQ-test-isolation.7
fn inherited_rhei_var_os(key: &str) -> Option<std::ffi::OsString> {
    debug_assert!(key.starts_with("RHEI_"), "{key} is not one of rhei's own variables");
    #[cfg(not(test))]
    {
        std::env::var_os(key)
    }
    #[cfg(test)]
    {
        None
    }
}

/// [`inherited_rhei_var_os`] as text; a value that is not Unicode is no value,
/// as `std::env::var` treats it. §REQ-test-isolation.7
fn inherited_rhei_var(key: &str) -> Option<String> {
    inherited_rhei_var_os(key).and_then(|value| value.into_string().ok())
}
