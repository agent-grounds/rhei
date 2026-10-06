//! `rhei remove` — taking back a ticket nothing has acted on, and retiring its
//! id so no later create reissues it.
//! §FS-rhei-remove
//!
//! Every test here is committed red and `#[ignore]`d, so the spec commit passes
//! the full-suite pre-commit gate; `cargo test -- --ignored remove_tests` shows
//! them failing. The implementation removes every `#[ignore]` in this file.

use std::fs;

use super::new_tests::{
    assert_failure, empty_project, flattened_output, new_run, project_with_rhei,
};
use super::*;

/// Everything under `dir` that a removal must leave byte-identical when it
/// refuses: every file's path and contents, `.home` excluded.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir).expect("readable dir").flatten().collect();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".home") {
                continue;
            }
            let relative = path.strip_prefix(root).expect("under root").to_path_buf();
            if path.is_dir() {
                out.push((relative, Vec::new()));
                walk(root, &path, out);
            } else {
                out.push((relative, fs::read(&path).expect("readable file")));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

// ---------------------------------------------------------------------------
// The ticket's own example — §FS-rhei-remove, §FS-rhei-remove.5.2, §FS-rhei-new.4
// ---------------------------------------------------------------------------

/// The case that prompted the command: a ticket filed by mistake is taken back
/// in one command, its section and its empty residue go, its sibling stays,
/// and the next create skips its number rather than reissuing it.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn removes_a_mistaken_ticket_and_never_reissues_its_id() {
    let dir = project_with_rhei("remove-mistaken");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(
        &["new", "Mistaken report", "--under", "auth", "--provides", "findings"],
        &dir,
    ));
    // Empty, ticket-owned residue: the export home of the mistaken ticket, and
    // its sibling's, which must survive. §FS-rhei-remove.4.2
    fs::create_dir_all(dir.join("runtime/exports/auth.2")).expect("residue dir");
    fs::create_dir_all(dir.join("runtime/exports/auth.1")).expect("sibling dir");

    let removed = new_run(&["remove", "auth.2"], &dir);
    assert_success(&removed);
    assert!(
        removed.stdout.contains("removed auth.2; id retired"),
        "§FS-rhei-remove.8 output; got:\n{}",
        removed.stdout
    );

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("plan");
    assert!(!plan.contains("Mistaken report"), "the section is gone:\n{plan}");
    assert!(!plan.contains("findings"), "its fields went with it:\n{plan}");
    assert!(plan.contains("### Task 1: First"), "the sibling stays:\n{plan}");
    assert!(!dir.join("runtime/exports/auth.2").exists(), "empty residue is cleaned");
    assert!(dir.join("runtime/exports/auth.1").is_dir(), "a sibling's residue is not");

    // §FS-rhei-remove.5.1: the retirement is recorded in the project manifest.
    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("manifest");
    assert!(manifest.contains("retiredTickets"), "manifest:\n{manifest}");
    assert!(manifest.contains("auth.2"), "manifest:\n{manifest}");

    assert_success(&new_run(&["validate"], &dir));
    let next = new_run(&["new", "Check another finding", "--under", "auth"], &dir);
    assert_success(&next);
    assert!(
        next.stdout.contains("Created ticket auth.3"),
        "§FS-rhei-new.4: a retired id is never reissued; got:\n{}",
        next.stdout
    );
}

/// §FS-rhei-remove.1.2, §FS-rhei-remove.5.1: the same, in a Directory Workspace
/// rhei and in the basin, whose retirement is the project's too.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn removes_and_retires_in_workspace_and_basin_rheis() {
    let dir = empty_project("remove-layouts");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    for (under, removed, next) in
        [("billing", "billing.1", "billing.2"), ("basin", "basin.1", "basin.2")]
    {
        assert_success(&new_run(&["new", "Mistake", "--under", under], &dir));
        let result = new_run(&["remove", removed], &dir);
        assert_success(&result);
        assert!(
            result.stdout.contains(&format!("removed {removed}; id retired")),
            "got:\n{}",
            result.stdout
        );
        let created = new_run(&["new", "Real one", "--under", under], &dir);
        assert_success(&created);
        assert!(
            created.stdout.contains(&format!("Created ticket {next}")),
            "a retired {removed} is never reissued; got:\n{}",
            created.stdout
        );
    }
    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("manifest");
    assert!(
        manifest.contains("billing.1") && manifest.contains("basin.1"),
        "manifest:\n{manifest}"
    );
}

/// §FS-rhei-remove.5.2, §FS-rhei-new.4: `--id` cannot bring a retired id back.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn refuses_to_recreate_a_retired_id_explicitly() {
    let dir = project_with_rhei("remove-resurrect");
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    assert_success(&new_run(&["remove", "auth.1"], &dir));

    let result = new_run(&["new", "Again", "--under", "auth", "--id", "1"], &dir);
    assert_failure(&result, "retired");
}

/// §FS-rhei-remove.5.1, §FS-rhei-reset.2: a reset returns live tickets to
/// their authored state and leaves the retirement record alone, so the id is
/// still not reissued afterwards.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn retirement_survives_a_reset() {
    let dir = project_with_rhei("remove-reset");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    assert_success(&new_run(&["remove", "auth.2"], &dir));

    assert_success(&new_run(&["reset", ".", "--yes"], &dir));

    let created = new_run(&["new", "Next", "--under", "auth"], &dir);
    assert_success(&created);
    assert!(created.stdout.contains("Created ticket auth.3"), "got:\n{}", created.stdout);
}

/// §FS-rhei-remove.7: a dry run checks and previews, and writes nothing.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn a_dry_run_previews_and_changes_nothing() {
    let dir = project_with_rhei("remove-dry-run");
    assert_success(&new_run(&["new", "Mistake", "--under", "auth"], &dir));
    let before = snapshot(&dir);

    let result = new_run(&["remove", "auth.1", "--dry-run"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("auth.1"),
        "the preview names the ticket; got:\n{}",
        result.stdout
    );
    assert_eq!(snapshot(&dir), before, "a dry run changes nothing");
}

// ---------------------------------------------------------------------------
// Refusals — §FS-rhei-remove.3
// ---------------------------------------------------------------------------

/// §FS-rhei-remove.3.1: a ticket another ticket's `**Prior:**` names is
/// refused, naming the dependent and the field, and nothing changes.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn refuses_a_ticket_another_names_in_prior() {
    let dir = project_with_rhei("remove-prior");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Second", "--under", "auth", "--prior", "auth.1"], &dir));
    let before = snapshot(&dir);

    let result = new_run(&["remove", "auth.1"], &dir);
    assert!(!result.status.success(), "a dependent must refuse removal\n{}", result.stdout);
    let said = flattened_output(&result);
    assert!(
        said.contains("auth.1 cannot be removed: auth.2 names it in **Prior:**"),
        "got:\n{said}"
    );
    assert_eq!(snapshot(&dir), before, "a refusal changes nothing");
}

/// §FS-rhei-remove.2, §FS-rhei-remove.3.4: a terminal ticket with a recorded
/// transition and a result has history, and history is kept.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn refuses_a_ticket_that_has_been_acted_on() {
    let dir = project_with_rhei("remove-acted-on");
    assert_success(&new_run(&["new", "Done already", "--under", "auth"], &dir));
    assert_success(&new_run(
        &["transition", "auth.1", "--from", "pending", "--to", "completed", "--result", "done"],
        &dir,
    ));
    let before = snapshot(&dir);

    let result = new_run(&["remove", "auth.1"], &dir);
    assert!(!result.status.success(), "history must refuse removal\n{}", result.stdout);
    let said = flattened_output(&result);
    assert!(said.contains("auth.1 cannot be removed"), "got:\n{said}");
    assert!(said.contains("runtime/results/auth.1.md"), "names the evidence; got:\n{said}");
    assert_eq!(snapshot(&dir), before, "a refusal changes nothing");
}

/// §FS-rhei-remove.2: a claimed ticket has been acted on even though it has
/// not moved — its `**Assignee:**` is the evidence.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn refuses_a_claimed_ticket() {
    let dir = project_with_rhei("remove-claimed");
    assert_success(&new_run(&["new", "Claimed", "--under", "auth"], &dir));
    assert_success(&new_run(&["next"], &dir));
    assert!(fs::read_to_string(dir.join("auth.rhei.md")).expect("plan").contains("**Assignee:**"));
    let before = snapshot(&dir);

    let result = new_run(&["remove", "auth.1"], &dir);
    assert!(!result.status.success(), "a claim must refuse removal\n{}", result.stdout);
    let said = flattened_output(&result);
    assert!(said.contains("auth.1 cannot be removed"), "got:\n{said}");
    assert!(said.contains("Assignee"), "names the evidence; got:\n{said}");
    assert_eq!(snapshot(&dir), before, "a refusal changes nothing");
}

/// §FS-rhei-remove.3.2: a ticket with children is refused, naming them.
#[test]
#[ignore = "red until `rhei remove` exists (§FS-rhei-remove); implement removes this"]
fn refuses_a_ticket_with_children() {
    let dir = project_with_rhei("remove-children");
    assert_success(&new_run(&["new", "Parent", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Child", "--under", "auth.1"], &dir));
    let before = snapshot(&dir);

    let result = new_run(&["remove", "auth.1"], &dir);
    assert!(!result.status.success(), "a parent must refuse removal\n{}", result.stdout);
    let said = flattened_output(&result);
    assert!(said.contains("auth.1 cannot be removed"), "got:\n{said}");
    assert!(said.contains("auth.1.1"), "names the child; got:\n{said}");
    assert_eq!(snapshot(&dir), before, "a refusal changes nothing");
}
