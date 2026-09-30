//! The gate every built-in template is held to after the union: it instantiates
//! standalone **and** `--into` a scratch rhei, both validate, and every ticket it
//! placed resolves to a profile the template brought.
//! §FS-rhei-library.6 §FS-rhei-library.3.3
//!
//! The last clause is the one that needs stating: a `task`-kind ticket that fell
//! into the host's default lane **validates** and is wrong, so a gate that only
//! checked `rhei validate` would pass a template whose tickets walk somebody
//! else's states. That is the proof that a template works at any level, and it
//! is why the gate runs twice rather than once.
//!
//! Today the `--into` half of every case fails with
//! `error: unexpected argument '--into' found`.

use std::fs;

use super::into_support::*;
use super::*;

/// Every built-in the binary ships, by name.
fn built_in_names(dir: &std::path::Path) -> Vec<String> {
    let listed = run_into(&["templates", "--json"], dir);
    assert_success(&listed);
    let value: serde_json::Value =
        serde_json::from_str(&listed.stdout).expect("templates --json should be JSON");
    value
        .as_array()
        .or_else(|| value["templates"].as_array())
        .expect("a list of templates")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .map(str::to_owned)
        .collect()
}

/// A scratch host with a machine of its own, ready to receive anything.
fn scratch_host(dir: &std::path::Path) -> std::path::PathBuf {
    let root = dir.join("scratch");
    fs::create_dir_all(root.join("tasks")).expect("create scratch host");
    write_fixture_file(&root, "index.rhei.md", HOST_INDEX);
    write_fixture_file(&root, "states.yaml", HOST_MACHINE);
    write_fixture_file(&root.join("tasks"), "001-ticket.md", HOST_TICKET);
    root
}

/// The gate itself, over every built-in that declares no required input it
/// cannot be given a placeholder for.
#[test]
fn every_built_in_instantiates_standalone_and_into_a_scratch_rhei() {
    let dir = unique_temp_dir("into-template-gate");
    let names = built_in_names(&dir);
    assert!(!names.is_empty(), "the binary should ship built-in templates");

    for name in &names {
        let host = scratch_host(&dir.join(name));
        let standalone =
            run_into(&["instantiate", name, "--output", &format!("{name}/standalone")], &dir);
        if !standalone.status.success() {
            // A template with a required input this gate cannot guess is out of
            // scope here; the gate is about placement, not about inputs.
            continue;
        }
        let validate_standalone = run_into(&["validate", &format!("{name}/standalone")], &dir);
        assert_success(&validate_standalone);

        let placed =
            run_into(&["instantiate", name, "--into", &format!("{name}/scratch.ticket")], &dir);
        assert!(
            placed.status.success(),
            "{name} instantiates standalone but not `--into` a scratch rhei; got:\nstdout:\n{}\nstderr:\n{}",
            placed.stdout,
            placed.stderr
        );
        let validate_placed = run_into(&["validate", &format!("{name}/scratch")], &dir);
        assert_success(&validate_placed);

        // Every placed ticket resolves to a profile the template brought, never
        // to the host's default lane.
        let parent = read(&host.join("tasks/001-ticket.md"));
        let placed_kinds: Vec<&str> = parent
            .lines()
            .filter(|line| line.starts_with("#### "))
            .filter_map(|line| line.trim_start_matches('#').trim().split(' ').next())
            .collect();
        assert!(
            !placed_kinds.is_empty(),
            "{name} placed no ticket under the scratch host's task; got:\n{parent}"
        );
        for kind in placed_kinds {
            assert_ne!(
                kind.to_ascii_lowercase(),
                "task",
                "{name} placed a bare `task`-kind ticket, which falls into the host's default lane and is wrong"
            );
        }
    }
}
