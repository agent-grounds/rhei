// agent-grounds/rhei#751: a call that replaces a task's earlier record says so,
// and every pointer a prompt gives to the raw store carries the rule that tells
// a task's live entry from the one it replaced.
//
// Its own part beside `note_command_tests.rs`: those pin what the verb writes,
// these pin what a reader is told about a store that keeps every record.

// §FS-rhei-note.5.1 §FS-rhei-note.1 §FS-rhei-memory.4.5 §FS-rhei-memory.3.4

use std::fs;

use super::note_store_support::*;
use super::*;

/// The fold's step 2, as every surface that names the raw store states it.
const FOLD_RULE: &str = "a task's last record there is its live one";

/// §FS-rhei-note.5.1: a first record prints today's line; a second one by the
/// same task names what it replaced, though the store keeps both records.
#[test]
fn a_second_record_by_one_task_says_it_replaced_the_first() {
    let (_dir, root) = note_fixture("note-replace-fact", None);

    let first = run_note(&root, &root, Some("auth.2"), &["The first thing I found."]);
    assert_success(&first);
    assert_eq!(
        first.stdout,
        "noted as auth.2 \u{2014} 1 line, composed into later prompts from now on\n"
    );

    let second = run_note(&root, &root, Some("auth.2"), &["What it actually was."]);
    assert_success(&second);
    assert_eq!(
        second.stdout,
        "noted as auth.2 \u{2014} 1 line, replacing its earlier entry; composed into later \
         prompts from now on\n"
    );
}

/// §FS-rhei-note.5.1: a fact recorded after the same task's own strike lifts
/// the strike, and the struck entry is back in composition (§FS-rhei-memory.4.2).
#[test]
fn a_record_after_the_tasks_own_strike_says_the_struck_entry_is_back() {
    let (_dir, root) = note_fixture("note-replace-strike", None);
    assert_success(&run_note(&root, &root, Some("auth.2"), &[TRAP]));
    assert_success(&run_note(&root, &root, Some("reporting.1"), &["--strike", "auth.2"]));

    let fact = run_note(&root, &root, Some("reporting.1"), &["The rollup reads the session."]);
    assert_success(&fact);
    assert_eq!(
        fact.stdout,
        "noted as reporting.1 \u{2014} 1 line, replacing its strike of auth.2, whose entry is \
         back in composition; composed into later prompts from now on\n"
    );
}

/// §FS-rhei-note.5.1: the clause is appended to a strike's line too, and a
/// strike lifted while another still holds the entry says nothing came back.
#[test]
fn a_restate_or_strike_that_replaces_the_slot_says_what_it_replaced() {
    let (_dir, root) = note_fixture("note-replace-forms", None);
    assert_success(&run_note(&root, &root, Some("auth.2"), &[TRAP]));
    assert_success(&run_note(&root, &root, Some("reporting.1"), &["A fact of its own."]));

    let strike = run_note(&root, &root, Some("reporting.1"), &["--strike", "auth.2"]);
    assert_success(&strike);
    assert_eq!(
        strike.stdout,
        "struck auth.2 \u{2014} out of composition; its bytes stay in runtime/notes.md; \
         replacing this task's earlier entry\n"
    );

    // A second strike of auth.2 holds it out, so lifting the first brings nothing back.
    assert_success(&run_note(&root, &root, Some("billing.1"), &["--strike", "auth.2"]));
    let restate = run_note(&root, &root, Some("reporting.1"), &["--restate", "auth.1"]);
    assert_success(&restate);
    assert_eq!(
        restate.stdout,
        "restated auth.1 \u{2014} now the newest entry, and this task's slot is spent; \
         replacing this task's strike of auth.2\n"
    );
}

/// `rhei` with the run's own identity taken off it, from `cwd`.
fn rhei(root: &Path, cwd: &Path, args: &[&str]) -> CliRun {
    let mut cmd = isolated_command(root);
    cmd.current_dir(cwd);
    cmd.args(args);
    CliRun::from(&cmd.output().expect("rhei should run"))
}

/// The issue's fixture: a Panta project with two member rheis on the built-in
/// machine, one task that changes its mind, and 25 other tasks that leave one
/// note each, so 26 live one-line entries overflow the 24-line cap.
fn superseded_notes_project() -> (TestDir, PathBuf) {
    let dir = unique_temp_dir("note-replace-overflow");
    let root = dir.to_path_buf().join("project");
    fs::create_dir_all(root.join("csa-constants/tasks")).expect("fixture directories");
    write_fixture_file(&root, "index.panta.md", "# Panta: Superseded notes\n");
    write_fixture_file(&root.join("csa-constants"), "index.rhei.md", "# Rhei: CSA constants\n");
    write_fixture_file(
        &root.join("csa-constants/tasks"),
        "deliver.md",
        "### Task deliver: Deliver the CSA constants\n**State:** pending\n\n\
         #### Task deliver.2: Measure the constants\n**State:** pending\n",
    );
    let mut other = String::from("# Rhei: Other\n\n## Tasks\n\n");
    for n in 1..=25 {
        other.push_str(&format!("### Task {n}: Other ticket {n}\n**State:** pending\n\n"));
    }
    write_fixture_file(&root, "other.rhei.md", &other);

    let task = ["--task", "csa-constants.deliver.2"];
    for text in [
        "Do not re-measure the constants; the operator measured them by hand.",
        "Re-measure the constants after the rebase; the earlier measurement no longer holds.",
    ] {
        assert_success(&rhei(&root, &root, &["note", task[0], task[1], text]));
    }
    for n in 1..=25 {
        let id = format!("other.{n}");
        let text = format!("Fact {n} about the other rhei.");
        assert_success(&rhei(&root, &root, &["note", "--task", &id, &text]));
    }
    (dir, root)
}

/// §FS-rhei-memory.4.5 item 5 and §FS-rhei-memory.3.4: the overflow line and
/// the map line still name the store, and now state the fold rule beside it, as
/// does the trail bullet — the file keeps the superseded record unmarked.
#[test]
fn every_pointer_to_the_raw_store_states_the_fold_rule() {
    let (_dir, root) = superseded_notes_project();

    let peek = rhei(&root, &root, &["next", "--peek"]);
    assert_success(&peek);
    let prompt = normalize_prompt(&root, &peek.stdout);

    let overflow = "\u{2026} 2 earlier notes not shown \u{2014} read {ROOT}/runtime/notes.md; \
                    a task's last record there is its live one\n";
    assert!(prompt.contains(overflow), "want {overflow:?}; got:\n{prompt}");

    let map = "- Notes left for later tickets: `{ROOT}/runtime/notes.md` \u{2014} append-only; \
               a task's last record there is its live one\n";
    assert!(prompt.contains(map), "want {map:?}; got:\n{prompt}");

    let trail = "A second call replaces it: `runtime/notes.md` keeps every record, and a task's \
                 last record there is its live one. Spending it on";
    assert!(prompt.contains(trail), "want {trail:?}; got:\n{prompt}");
}

/// §FS-rhei-note.1: `rhei note --help` says that the file keeps every record,
/// so a caller who reads both there does not take the replacement for a failure.
#[test]
fn note_help_says_the_store_keeps_every_record() {
    let (_dir, root) = note_fixture("note-replace-help", None);

    let help = rhei(&root, &root, &["note", "--help"]);
    assert_success(&help);
    let flowing = help.stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    let sentence = "A second call replaces this task's entry in every later prompt, but \
                    runtime/notes.md keeps every record: a task's last record there is its live \
                    one.";
    assert!(flowing.contains(sentence), "want {sentence:?}; got:\n{}", help.stdout);
    assert!(flowing.contains(FOLD_RULE));
}
