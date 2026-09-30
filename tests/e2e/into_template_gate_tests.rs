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
//! Both clauses are held to what §FS-rhei-library asserts and no wider.
//!
//! - A template may ship **states and no tickets** — §FS-rhei-library.4 blesses
//!   it in as many words, and `fix` is one — so a template that placed nothing
//!   has no routing to check and still owes the rest of the gate.
//! - A template that declares no `by_type` at all is the spec's "means the
//!   host's default lane" case (§FS-rhei-library.3.3). `code-review` ships one
//!   deliberately, because its ticket is the `Task coordinate` every caller
//!   holds. Such a template is not wrong, it is *unrouted*, and the host owes it
//!   the spanning profile §FS-rhei-library.3.5 says a host writes — so the gate
//!   writes one rather than skipping the template.
//! - A machine in the pre-`profiles` form is the one shape no host can take in:
//!   `node_policy` is what §FS-rhei-library.3.3 projects onto a host, and a
//!   state's `initial: true` marker is illegal beside `profiles`. That is a fact
//!   about the template's own grammar, so it owes the standalone half alone.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::into_support::*;
use super::*;

/// Every built-in the binary ships, by name.
fn built_in_names(dir: &Path) -> Vec<String> {
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

/// A scratch host with a machine of its own, whose default lane spans `also` —
/// the states an unrouted template's tickets walk. With nothing extra this is
/// the plain host every other `--into` test uses. §FS-rhei-library.3.5
fn scratch_host(dir: &Path, also: &BTreeSet<String>) -> PathBuf {
    let root = dir.join("scratch");
    fs::create_dir_all(root.join("tasks")).expect("create scratch host");
    let extra: Vec<&str> = also
        .iter()
        .map(String::as_str)
        .filter(|state| !["pending", "completed", "cancelled"].contains(state))
        .collect();
    let spanning = if extra.is_empty() {
        HOST_MACHINE.to_owned()
    } else {
        HOST_MACHINE.replace(
            "allowed: [pending, completed, cancelled]",
            &format!("allowed: [pending, completed, cancelled, {}]", extra.join(", ")),
        )
    };
    write_fixture_file(&root, "index.rhei.md", HOST_INDEX);
    write_fixture_file(&root, "states.yaml", &spanning);
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
        let standalone =
            run_into(&["instantiate", name, "--output", &format!("{name}/standalone")], &dir);
        if !standalone.status.success() {
            // A template with a required input this gate cannot guess is out of
            // scope here; the gate is about placement, not about inputs.
            continue;
        }
        let validate_standalone = run_into(&["validate", &format!("{name}/standalone")], &dir);
        assert_success(&validate_standalone);

        let machine = dir.join(format!("{name}/standalone/states.yaml"));
        let routes = declared_routes(&machine);
        let brought = brought_profiles(&machine);
        if brought.is_empty() {
            continue;
        }
        // Unrouted tickets mean the host's default lane, so the host spans the
        // states they walk. A template that routes needs nothing of the host.
        let spanned = if routes.is_empty() { declared_states(&machine) } else { BTreeSet::new() };
        let host = scratch_host(&dir.join(name), &spanned);

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
        // to the host's default lane — for a template that routes. One that does
        // not placed its tickets into the spanning lane above, on purpose.
        let parent = read(&host.join("tasks/001-ticket.md"));
        let placed_kinds: Vec<String> = parent
            .lines()
            .filter(|line| line.starts_with("#### "))
            .filter_map(|line| line.trim_start_matches('#').trim().split(' ').next())
            .map(str::to_ascii_lowercase)
            .collect();
        if placed_kinds.is_empty() || routes.is_empty() {
            continue;
        }
        for kind in placed_kinds {
            let profile = routes.get(&kind).unwrap_or_else(|| {
                panic!(
                    "{name} placed a '{kind}'-kind ticket its own `node_policy.by_type` does not \
                     route, so it falls into the host's default lane and is wrong; routes: {routes:?}"
                )
            });
            assert!(
                brought.contains(profile),
                "{name} routes '{kind}' to profile '{profile}', which the template does not bring"
            );
        }
    }
}

/// A template's own `node_policy.by_type`: which kind it routes to which
/// profile. Empty where it declares none, which is the spec's "means the host's
/// default lane". §FS-rhei-library.3.3
fn declared_routes(machine: &Path) -> BTreeMap<String, String> {
    machine_value(machine)
        .get("node_policy")
        .and_then(|policy| policy.get("by_type"))
        .and_then(serde_yaml::Value::as_mapping)
        .map(|by_type| {
            by_type
                .iter()
                .filter_map(|(kind, profile)| {
                    Some((kind.as_str()?.to_ascii_lowercase(), profile.as_str()?.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Every profile the template brings.
fn brought_profiles(machine: &Path) -> BTreeSet<String> {
    named_keys(machine, "profiles")
}

/// Every state the template brings.
fn declared_states(machine: &Path) -> BTreeSet<String> {
    named_keys(machine, "states")
}

fn named_keys(machine: &Path, block: &str) -> BTreeSet<String> {
    machine_value(machine)
        .get(block)
        .and_then(serde_yaml::Value::as_mapping)
        .map(|entries| {
            entries.iter().filter_map(|(name, _)| name.as_str().map(str::to_owned)).collect()
        })
        .unwrap_or_default()
}

fn machine_value(machine: &Path) -> serde_yaml::Value {
    let raw =
        fs::read_to_string(machine).unwrap_or_else(|e| panic!("read {}: {e}", machine.display()));
    serde_yaml::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", machine.display()))
}
