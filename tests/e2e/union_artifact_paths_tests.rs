//! What a union refuses and what it leaves alone about one rhei-scoped
//! artifact path, cell by cell. §FS-rhei-library.7.2
//!
//! A *writer* lists the path under `outputs:`, a *reader* only under
//! `inputs:`. Two writers on opposite sides of a union are the one refusal
//! (§FS-rhei-library.7.2.1); a writer handing its file to readers is silent
//! wherever the two sit (§FS-rhei-library.7.2.2); and a pair inside one side's
//! own machine is never the union's to diagnose (§FS-rhei-library.7.2.4).
//! Readers with no writer are `union_shared_input_warning_tests`.
//!
//! Before the change one check pooled `inputs:` and `outputs:` from both sides
//! into one set of claimants, so every hand-off and every pair inside one side
//! was refused, with help that sent the author to `{task_id}`.

use super::into_support::{read, run_into};
use super::union_artifact_paths_support::*;
use super::*;

/// The path the `--into` cells declare.
const NOTE: &str = "runtime/notes/plan.md";

/// The pull request body `describe` hands to `ship`.
const BODY: &str = "runtime/supervision/pr-description.md";

/// The refusal's help, which names a remedy that works for two writers.
/// §FS-rhei-library.7.2.1
const WRITERS_HELP: &str = "help: give the two states distinct artifact paths, or keep one of \
                            them as the path's writer and have the other list it under `inputs:`.";

/// The `path` of a state's first declared artifact in `list` of a laid machine.
fn declared_path(machine: &std::path::Path, state: &str, list: &str) -> String {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&read(machine)).expect("the laid machine parses");
    yaml["states"][state][list][0]["path"].as_str().unwrap_or_default().to_owned()
}

/// Q1: two writers inside the placed template were not introduced by placing
/// it, so `--into` says nothing about them. §FS-rhei-library.7.2.4
#[test]
fn two_writers_inside_the_placed_template_are_silent() {
    let (dir, _root) = target_with("artifact-q1-placed", &[("pending", &[])]);
    placed(&dir, "drafts", &[("draft", &[Writes(NOTE)]), ("polish", &[Writes(NOTE)])]);

    let result = run_into(&["instantiate", "drafts", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(NOTE), "a pair inside one side is silent:\n{}", result.stderr);
}

/// Q1 on the other side: the target's own pair of writers is not the
/// placement's to diagnose either. §FS-rhei-library.7.2.4
#[test]
fn two_writers_inside_the_target_are_silent() {
    let (dir, _root) = target_with(
        "artifact-q1-target",
        &[("pending", &[Writes(NOTE)]), ("tidy", &[Writes(NOTE)])],
    );
    placed(&dir, "plain", &[("review", &[])]);

    let result = run_into(&["instantiate", "plain", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(NOTE), "a pair inside one side is silent:\n{}", result.stderr);
}

/// Q2 under `includes:`, the including template against its entry: refused at
/// that entry, naming both states and the side each is on, with the help that
/// works, and nothing left behind. §FS-rhei-library.7.2.1
#[test]
fn an_includes_entry_writing_the_hosts_path_is_refused_naming_both_sides() {
    let dir = unique_temp_dir("artifact-q2-host-entry");
    ticket_host(&dir, &[Writes(BODY)], &["rewriting"], &["../rewrite-body"]);
    part(&dir, "rewrite-body", &[("rewriting", &[Writes(BODY)])]);

    let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
    assert!(!result.status.success(), "two writers across a union must be refused");
    assert_stderr_contains(
        &result,
        &format!(
            "`includes:` entry '../rewrite-body' of template 'ticket-host': states 'supervising' \
             (in 'ticket-host') and 'rewriting' (in '../rewrite-body') both declare the \
             rhei-scoped artifact path '{BODY}' in `outputs:`"
        ),
    );
    assert_stderr_contains(&result, WRITERS_HELP);
    assert!(!dir.join("out").exists(), "a refused instantiation leaves no output directory");
}

/// Q2 under `includes:`, an entry against a later entry: the accumulated
/// machine is one side and the later entry the other, so the refusal lands at
/// the later entry. §FS-rhei-library.7.2.1
#[test]
fn a_later_includes_entry_writing_an_earlier_entrys_path_is_refused_at_the_later_entry() {
    let dir = unique_temp_dir("artifact-q2-entry-entry");
    ticket_host(&dir, &[], &["describing", "rewriting"], &["../describe", "../rewrite-body"]);
    part(&dir, "describe", &[("describing", &[Writes(BODY)])]);
    part(&dir, "rewrite-body", &[("rewriting", &[Writes(BODY)])]);

    let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
    assert!(!result.status.success(), "two writers across a union must be refused");
    assert_stderr_contains(
        &result,
        &format!(
            "`includes:` entry '../rewrite-body' of template 'ticket-host': states 'describing' \
             (in 'ticket-host') and 'rewriting' (in '../rewrite-body') both declare the \
             rhei-scoped artifact path '{BODY}' in `outputs:`"
        ),
    );
    assert_stderr_contains(&result, WRITERS_HELP);
    assert!(!dir.join("out").exists(), "a refused instantiation leaves no output directory");
}

/// Q3, ported from validation's `reproduce-handoff-refusal.sh`: one template
/// whose `produce` writes a note its `consume` reads joins a rhei under a task.
/// §FS-rhei-library.7.2.2
#[test]
fn a_hand_off_inside_the_placed_template_is_silent() {
    let (dir, _root) = target_with("artifact-q3-placed", &[("pending", &[])]);
    let note = "runtime/note.md";
    placed(&dir, "handoff", &[("produce", &[Writes(note)]), ("consume", &[Reads(note)])]);

    let result = run_into(&["instantiate", "handoff", "--into", "release.ticket"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(note), "a hand-off is silent:\n{}", result.stderr);
    assert_success(&run_into(&["validate", "release"], &dir));
}

/// Q3 inside the target: its own hand-off does not stop a placement that never
/// touches the path. §FS-rhei-library.7.2.2
#[test]
fn a_hand_off_inside_the_target_is_silent() {
    let (dir, _root) = target_with(
        "artifact-q3-target",
        &[("pending", &[Writes(NOTE)]), ("check", &[Reads(NOTE)])],
    );
    placed(&dir, "plain", &[("review", &[])]);

    let result = run_into(&["instantiate", "plain", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(NOTE), "a hand-off is silent:\n{}", result.stderr);
}

/// Q4 under `--into`: the target writes, the placed template reads, and both
/// keep the literal path — no artifact path is rewritten. §FS-rhei-library.7.2.2
#[test]
fn a_hand_off_from_the_target_to_the_placed_template_composes_with_literal_paths() {
    let (dir, root) = target_with("artifact-q4-into", &[("pending", &[Writes(NOTE)])]);
    placed(&dir, "reader", &[("review", &[Reads(NOTE)])]);

    let result = run_into(&["instantiate", "reader", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(NOTE), "a hand-off is silent:\n{}", result.stderr);
    let machine = root.join("states.yaml");
    assert_eq!(declared_path(&machine, "pending", "outputs"), NOTE, "the writer keeps the path");
    assert_eq!(declared_path(&machine, "review", "inputs"), NOTE, "the reader keeps the path");
    assert_success(&run_into(&["validate", "release"], &dir));
}

/// Q4 under `includes:`, ported from validation's
/// `reproduce-includes-handoff.sh`: `describe` and `ship` taken out as
/// templates of their own share the body the way §FS-rhei-library.3.4 says
/// parts do, by agreeing on an input default. §FS-rhei-library.7.2.2
#[test]
fn a_hand_off_between_included_templates_composes_through_agreeing_defaults() {
    let dir = unique_temp_dir("artifact-q4-includes");
    let inputs = format!(
        "inputs:\n  - name: body_path\n    description: Where the pull request body is handed \
         over\n    type: string\n    default: {BODY}\n"
    );
    for (name, state, decl) in [
        ("describe", "describing", Writes("{{body_path}}")),
        ("ship", "shipping", Reads("{{body_path}}")),
    ] {
        Template { name, states: &[(state, &[decl])], inputs: &inputs, ..Template::default() }
            .write(&dir);
    }
    ticket_host(&dir, &[], &["describing", "shipping"], &["../describe", "../ship"]);

    let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Validation succeeded"), "stdout:\n{}", result.stdout);
    assert!(!result.stderr.contains(BODY), "a hand-off is silent:\n{}", result.stderr);
    let machine = dir.join("out/states.yaml");
    assert_eq!(declared_path(&machine, "describing", "outputs"), BODY, "the writer keeps it");
    assert_eq!(declared_path(&machine, "shipping", "inputs"), BODY, "the reader keeps it");
}

/// `{task_id_local}` is as per-task as `{task_id}`: two writers of a path
/// carrying it are one file per ticket, not a collision. §FS-rhei-library.7.2
#[test]
fn writers_of_a_task_id_local_path_across_a_union_are_not_refused() {
    let per_ticket = "runtime/notes/{task_id_local}.md";
    let (dir, _root) = target_with("artifact-task-id-local", &[("pending", &[Writes(per_ticket)])]);
    placed(&dir, "per-ticket", &[("review", &[Writes(per_ticket)])]);

    let result = run_into(&["instantiate", "per-ticket", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains("rhei-scoped"), "stderr:\n{}", result.stderr);
}

/// A state listing the path in both lists counts once, as a writer, so a
/// reader in the target makes it a hand-off. §FS-rhei-library.7.2
#[test]
fn a_state_listing_the_path_in_both_lists_is_one_writer_and_hands_off() {
    let (dir, _root) = target_with("artifact-both-lists", &[("pending", &[Reads(NOTE)])]);
    placed(&dir, "rewrite", &[("review", &[Reads(NOTE), Writes(NOTE)])]);

    let result = run_into(&["instantiate", "rewrite", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(NOTE), "a hand-off is silent:\n{}", result.stderr);
}

/// The converse: a placed state that reads the target's file and also lists it
/// under `outputs:` is a second writer, and is refused. §FS-rhei-library.7.2.1
#[test]
fn a_reader_that_also_writes_the_targets_path_is_refused() {
    let (dir, root) = target_with("artifact-reader-writer", &[("pending", &[Writes(NOTE)])]);
    placed(&dir, "rewrite", &[("review", &[Reads(NOTE), Writes(NOTE)])]);
    prepare_placement_sidecars(&root, &["tasks/002-work.md.lock"]);
    let before = snapshot(&root);

    let result = run_into(&["instantiate", "rewrite", "--into", "release"], &dir);
    assert!(!result.status.success(), "a second writer across a union must be refused");
    assert_stderr_contains(
        &result,
        &format!(
            "states 'pending' (in the target) and 'review' (in template 'rewrite') both declare \
             the rhei-scoped artifact path '{NOTE}' in `outputs:`"
        ),
    );
    assert_eq!(before, snapshot(&root), "nothing is written until the union validates");
}

/// Guard: an identical same-named writer brought by both sides is one state
/// once the union coalesces it, not two writers, so it composes silently.
/// §FS-rhei-library.7.2
#[test]
fn an_identical_same_named_writer_on_both_sides_merges_silently() {
    let notes: Spec = ("notes", &[Writes(NOTE)]);
    let (dir, root) = target_with("artifact-same-writer", &[("pending", &[]), notes]);
    placed(&dir, "noted", &[("review", &[]), notes]);

    let result = run_into(&["instantiate", "noted", "--into", "release"], &dir);
    assert_success(&result);
    assert!(!result.stderr.contains(NOTE), "one coalesced writer is silent:\n{}", result.stderr);
    assert_eq!(declared_path(&root.join("states.yaml"), "notes", "outputs"), NOTE);
}
