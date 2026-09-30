//! `changeset-review` after the union: same name, same inputs, same ticket ids,
//! same artifact paths — and a machine whose states are the names their authors
//! wrote rather than `m6_review__split`.
//! §FS-rhei-library.9 §FS-rhei-library.14
//!
//! This is the one test that says the **non-breaking promise held**. Rule 1
//! refuses two `profiles.primary`, so `code-review` and `fix` are re-authored
//! with a profile and a node kind each, and `changeset-review` becomes the host
//! that writes the edge between them and the profile that spans them. What a
//! user could notice is only what a *new* instantiation's machine looks like;
//! everything a caller passes or reads is unchanged, and this pins that.
//!
//! The golden files under `fixtures/changeset-review-golden/` were captured
//! while the compiler was still live. The three surface tests here therefore
//! **pass today, by construction** — that is what a regression guard is, and it
//! is why they are not to be deleted for passing. The driven test below is the
//! half that cannot pass today.

use std::fs;
use std::path::PathBuf;

use super::into_support::run_into;
use super::*;

fn golden(name: &str) -> String {
    let path: PathBuf = repo_root().join("tests/e2e/fixtures/changeset-review-golden").join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `--list-inputs` byte for byte. An input renamed, reordered, retyped or given
/// a different default is a break for every caller that scripts the template,
/// and re-authoring the internals must not be one.
/// §FS-rhei-library.11.4
#[test]
fn changeset_review_list_inputs_is_byte_identical() {
    let dir = unique_temp_dir("changeset-golden-inputs");

    let result = run_into(&["instantiate", "changeset-review", "--list-inputs"], &dir);
    assert_success(&result);
    assert_eq!(
        result.stdout,
        golden("list-inputs.txt"),
        "changeset-review's input surface must not move when its internals are re-authored"
    );
}

/// The ticket ids an instantiation writes. A ticket id is what a `**Prior:**`, a
/// `rhei next`, an `--task` and every artifact path's `{task_id}` resolve
/// through, so it is the part of the output callers actually hold.
#[test]
fn changeset_review_writes_the_same_ticket_ids() {
    let dir = unique_temp_dir("changeset-golden-ids");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let mut ids: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir.join("out/tasks")).expect("the workspace has task files") {
        let text = fs::read_to_string(entry.expect("dir entry").path()).expect("read task file");
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("### ") {
                if let Some((head, _)) = rest.split_once(':') {
                    ids.push(head.to_owned());
                }
            }
        }
    }
    ids.sort();
    let mut expected: Vec<String> = golden("ticket-ids.txt").lines().map(str::to_owned).collect();
    expected.sort();
    assert_eq!(ids, expected, "the ticket ids callers hold must not move");
}

/// Every artifact path the machine declares. `--into` leaves paths alone, so
/// re-authoring must too: a path is the contract between two states and, in this
/// template, between the review's `decide` and the fix's entry.
/// §FS-rhei-library.12
#[test]
fn changeset_review_declares_the_same_artifact_paths() {
    let dir = unique_temp_dir("changeset-golden-paths");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let machine = fs::read_to_string(dir.join("out/states.yaml")).expect("read machine");
    let mut paths: Vec<String> = machine
        .lines()
        .filter_map(|line| line.trim().strip_prefix("path: "))
        .map(str::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    let mut expected: Vec<String> =
        golden("artifact-paths.txt").lines().map(str::to_owned).collect();
    expected.sort();
    assert_eq!(paths, expected, "the artifact paths the states hand each other must not move");
}

/// The machine reads like a machine somebody wrote: `split` and `final-fix`
/// rather than `m6_review__split` and `m3_fix__final-fix`. This is the one thing
/// a user could notice, and the reason the change is worth making.
/// §FS-rhei-library.9
#[test]
fn changeset_review_states_keep_their_authors_names() {
    let dir = unique_temp_dir("changeset-names");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let machine = fs::read_to_string(dir.join("out/states.yaml")).expect("read machine");
    assert!(
        !machine.contains("m6_review__") && !machine.contains("m3_fix__"),
        "no alias-encoded state name survives the union; got:\n{machine}"
    );
    for state in ["  split:", "  review:", "  human-review:", "  final-fix:"] {
        assert!(machine.contains(state), "the union should carry {state}; got:\n{machine}");
    }
    // A prompt can therefore name a state, instead of being told to look one up.
    let coordinate = fs::read_to_string(dir.join("out/tasks/001-coordinate.md"))
        .expect("read the coordinate ticket");
    assert!(
        !coordinate.contains("compiled state names"),
        "the coordinate ticket no longer has to send its agent to states.yaml; got:\n{coordinate}"
    );
}

/// The chain the wrapper writes actually runs: a driven ticket reaches the
/// review's gate and then the fix's entry. `code-review`'s `human-review` is
/// `final: true` with no outgoing edge today, so this cannot pass until its
/// author makes it a non-final gate — the union never un-finalizes a terminal.
/// §FS-rhei-library.11.1 §FS-rhei-library.11.5
#[test]
fn a_driven_changeset_review_reaches_human_review_and_then_final_fix() {
    let dir = unique_temp_dir("changeset-driven");

    let result = run_into(
        &["instantiate", "changeset-review", "change_ref=GOLDEN", "--output", "out"],
        &dir,
    );
    assert_success(&result);

    let machine = fs::read_to_string(dir.join("out/states.yaml")).expect("read machine");
    // The gate has a way out, which is what makes the chain a chain.
    let human_review =
        machine.split("  human-review:").nth(1).expect("the machine declares human-review");
    let body = human_review.split("\n  ").next().unwrap_or(human_review);
    assert!(
        !body.contains("final: true"),
        "a gate a chain is continued from cannot be terminal; got:\n{body}"
    );

    // Two edges out of the gate: approve with no fix, and continue into the fix.
    assert!(
        machine.contains("from: human-review"),
        "human-review needs outgoing edges; got:\n{machine}"
    );
    for target in ["to: completed", "to: final-fix"] {
        assert!(machine.contains(target), "the gate should reach {target}; got:\n{machine}");
    }

    // And the move the runtime actually allows.
    let moved = run_into(
        &[
            "transition",
            "out",
            "--task",
            "coordinate",
            "--from",
            "human-review",
            "--to",
            "final-fix",
            "--no-callbacks",
        ],
        &dir,
    );
    assert_success(&moved);
}
