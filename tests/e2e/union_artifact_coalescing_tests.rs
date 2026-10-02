//! R1-01: coalesced writers preserve pairs internal to either side without
//! excusing a new cross-boundary pair. §FS-rhei-library.7.2.4

use super::into_support::{read, run_into};
use super::union_artifact_paths_support::*;
use super::*;

const NOTE: &str = "runtime/notes/plan.md";
const WRITERS_HELP: &str = "help: give the two states distinct artifact paths, or keep one of \
                            them as the path's writer and have the other list it under `inputs:`.";

fn assert_internal_pair(machine: &std::path::Path) {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&read(machine)).expect("laid machine parses");
    let states = yaml["states"].as_mapping().expect("states mapping");
    let writers: Vec<&str> = states
        .iter()
        .filter(|(_, state)| state["outputs"][0]["path"].as_str() == Some(NOTE))
        .map(|(name, _)| name.as_str().expect("state name"))
        .collect();
    assert_eq!(writers.len(), 2, "the shared state coalesces into one writer");
    assert!(writers.contains(&"pending") && writers.contains(&"polish"), "writers: {writers:?}");
}

/// The placed pair and the reverse distribution both remain internal when
/// `pending` coalesces with an identical writer. §FS-rhei-library.7.2.4
#[test]
fn placement_preserves_internal_pairs_with_a_coalesced_writer_on_either_side() {
    let single: &[Spec] = &[("pending", &[Writes(NOTE)])];
    let pair: &[Spec] = &[("pending", &[Writes(NOTE)]), ("polish", &[Writes(NOTE)])];
    for (target, incoming) in [(single, pair), (pair, single)] {
        let (dir, root) = target_with("artifact-coalesced-into", target);
        placed(&dir, "drafts", incoming);

        let result = run_into(&["instantiate", "drafts", "--into", "release"], &dir);
        assert_success(&result);
        assert!(!result.stderr.contains(NOTE), "an internal pair is silent:\n{}", result.stderr);
        assert_internal_pair(&root.join("states.yaml"));
    }
}

/// Composition must preserve the same pair with either include order: one
/// writer coalesces and the pair belongs to one entry. §FS-rhei-library.7.2.4
#[test]
fn includes_preserve_internal_pairs_with_a_coalesced_writer_in_either_order() {
    let single: &[Spec] = &[("pending", &[Writes(NOTE)])];
    let pair: &[Spec] = &[("pending", &[Writes(NOTE)]), ("polish", &[Writes(NOTE)])];
    for (first, second) in [(single, pair), (pair, single)] {
        let dir = unique_temp_dir("artifact-coalesced-includes");
        part(&dir, "first", first);
        part(&dir, "second", second);
        ticket_host(&dir, &[], &["pending", "polish"], &["../first", "../second"]);

        let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
        assert_success(&result);
        assert!(!result.stderr.contains(NOTE), "an internal pair is silent:\n{}", result.stderr);
        assert_internal_pair(&dir.join("out/states.yaml"));
    }
}

/// Coalescing `pending` does not excuse the new `draft`/`polish` pair,
/// whose writers belong exclusively to opposite sides. §FS-rhei-library.7.2.1
#[test]
fn placement_refuses_exclusive_writers_even_beside_a_coalesced_writer() {
    let (dir, root) = target_with(
        "artifact-coalesced-into-collision",
        &[("pending", &[Writes(NOTE)]), ("draft", &[Writes(NOTE)])],
    );
    placed(&dir, "drafts", &[("pending", &[Writes(NOTE)]), ("polish", &[Writes(NOTE)])]);
    let before = snapshot(&root);

    let result = run_into(&["instantiate", "drafts", "--into", "release"], &dir);
    assert!(!result.status.success(), "exclusive writers introduce a new collision");
    assert_stderr_contains(
        &result,
        &format!(
            "states 'draft' (in the target) and 'polish' (in template 'drafts') both declare \
             the rhei-scoped artifact path '{NOTE}' in `outputs:`"
        ),
    );
    assert_stderr_contains(&result, WRITERS_HELP);
    assert_eq!(before, snapshot(&root), "refusal precedes writes");
}

/// The next include still collides with an exclusive accumulated writer
/// even when both entries also bring `pending`. §FS-rhei-library.7.2.1
#[test]
fn includes_refuse_exclusive_writers_even_beside_a_coalesced_writer() {
    let dir = unique_temp_dir("artifact-coalesced-includes-collision");
    part(&dir, "first", &[("pending", &[Writes(NOTE)]), ("draft", &[Writes(NOTE)])]);
    part(&dir, "second", &[("pending", &[Writes(NOTE)]), ("polish", &[Writes(NOTE)])]);
    ticket_host(&dir, &[], &["pending", "draft", "polish"], &["../first", "../second"]);

    let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
    assert!(!result.status.success(), "exclusive writers introduce a new collision");
    assert_stderr_contains(
        &result,
        &format!(
            "`includes:` entry '../second' of template 'ticket-host': states 'draft' \
             (in 'ticket-host') and 'polish' (in '../second') both declare the rhei-scoped \
             artifact path '{NOTE}' in `outputs:`"
        ),
    );
    assert_stderr_contains(&result, WRITERS_HELP);
    assert!(!dir.join("out").exists(), "refusal leaves no output directory");
}

/// A shared name with different instructions does not grant membership on
/// both sides; ordinary definition equality still governs. §FS-rhei-library.3.1
#[test]
fn placement_does_not_treat_conflicting_same_named_writers_as_coalesced() {
    let single: &[Spec] = &[("pending", &[Writes(NOTE)])];
    let pair: &[Spec] = &[("pending", &[Writes(NOTE)]), ("polish", &[Writes(NOTE)])];
    for (target, incoming) in [(single, pair), (pair, single)] {
        let (dir, root) = target_with("artifact-conflicting-writer", target);
        let template = placed(&dir, "drafts", incoming);
        let machine = template.join("states.yaml");
        std::fs::write(&machine, read(&machine).replace("Do the pending step.", "Different work."))
            .expect("change pending's definition");
        let before = snapshot(&root);

        let result = run_into(&["instantiate", "drafts", "--into", "release"], &dir);
        assert!(!result.status.success(), "a conflicting name cannot coalesce");
        assert_stderr_contains(
            &result,
            &format!("rhei-scoped artifact path '{NOTE}' in `outputs:`"),
        );
        assert_stderr_contains(&result, WRITERS_HELP);
        assert_eq!(before, snapshot(&root), "refusal precedes writes");
    }
}
