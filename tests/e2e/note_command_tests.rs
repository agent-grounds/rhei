// `rhei note` as a caller meets it: the three forms, where the record lands,
// the two refusals, and the one exception that gets past the duplicate rule.
//
// Its own part beside `memory_note_store_tests.rs`: those read what a prompt
// composes, these read what the verb writes and what it refuses to write.

// §FS-rhei-note.1 §FS-rhei-note.2 §FS-rhei-note.4 §FS-rhei-note.5

use std::fs;

use super::note_store_support::*;
use super::*;

/// The store as it stands after the run, read from the project execution root.
fn store(root: &Path) -> String {
    fs::read_to_string(root.join("runtime/notes.md")).unwrap_or_default()
}

/// §FS-rhei-note.3.1: one record, at the project execution root — not under
/// the owning rhei, even when the writer's rhei is a directory workspace with
/// a runtime tree of its own.
#[test]
fn a_record_lands_at_the_project_execution_root() {
    let (_dir, root) = note_fixture("note-verb-record", None);

    let run = run_note(&root, &root.join("auth"), Some("auth.2"), &[TRAP]);
    assert_success(&run);
    assert!(run.stdout.contains("noted as auth.2"), "got:\n{}", run.stdout);

    assert_eq!(store(&root), format!("- [auth.2] {TRAP}\n"));
    assert!(
        !root.join("auth/runtime/notes.md").exists(),
        "the store is the project's, not the rhei's"
    );
}

/// §FS-rhei-note.1: a manual worker has no `RHEI_TASK_ID`, and names the task.
/// With neither, the verb cannot know whose slot it is spending.
#[test]
fn the_writing_task_comes_from_the_environment_or_from_task() {
    let (_dir, root) = note_fixture("note-verb-task", None);

    let named = run_note(&root, &root, None, &["--task", "reporting.1", "A fact worth one line."]);
    assert_success(&named);
    assert!(store(&root).starts_with("- [reporting.1] "), "got:\n{}", store(&root));

    let anonymous = run_note(&root, &root, None, &["Another fact."]);
    assert!(!anonymous.status.success(), "got:\n{}", anonymous.stdout);
    assert_eq!(anonymous.status.code(), Some(2), "usage, not a refusal");
    assert!(anonymous.stderr.contains("--task"), "got:\n{}", anonymous.stderr);
}

/// §FS-rhei-note.2: the slot is keyed to the task, so a second call replaces
/// the first — and it does so through the fold, leaving both records in place.
#[test]
fn a_second_record_by_one_task_supersedes_its_first() {
    let (_dir, root) = note_fixture("note-verb-second", None);

    assert_success(&run_note(&root, &root, Some("auth.2"), &["The first thing I found."]));
    assert_success(&run_note(&root, &root, Some("auth.2"), &["What it actually was."]));

    let written = store(&root);
    assert!(written.contains("- [auth.2] The first thing I found.\n"), "got:\n{written}");
    assert!(written.contains("- [auth.2] What it actually was.\n"), "got:\n{written}");
    assert!(
        written.find("The first thing I found") < written.find("What it actually was"),
        "nothing is rewritten; the fold decides. got:\n{written}"
    );
}

/// §FS-rhei-note.4: over the bound the command refuses and names it, because
/// an agent can retry in the same breath and half a fact is worse than none.
#[test]
fn a_four_line_entry_is_refused_with_the_bound_named() {
    let (_dir, root) = note_fixture("note-verb-long", None);

    let long = "one\ntwo\nthree\nfour";
    let run = run_note(&root, &root, Some("auth.2"), &[long]);
    assert!(!run.status.success(), "got:\n{}", run.stdout);
    assert_eq!(run.status.code(), Some(1));
    assert!(run.stderr.contains('3') && run.stderr.contains('4'), "got:\n{}", run.stderr);
    assert!(!root.join("runtime/notes.md").exists(), "nothing is written on a refusal");

    let three = "one\ntwo\nthree";
    assert_success(&run_note(&root, &root, Some("auth.2"), &[three]));
}

/// §FS-rhei-note.4: an exact duplicate of a live entry is refused, so the
/// store cannot carry the same sentence twice at everyone's expense.
#[test]
fn an_exact_duplicate_of_a_live_entry_is_refused() {
    let (_dir, root) = note_fixture("note-verb-duplicate", None);
    assert_success(&run_note(&root, &root, Some("auth.2"), &[TRAP]));

    let again = run_note(&root, &root, Some("reporting.1"), &[TRAP]);
    assert!(!again.status.success(), "got:\n{}", again.stdout);
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(store(&root), format!("- [auth.2] {TRAP}\n"));
}

/// §FS-rhei-note.4: and of the writer's own standing context, which every
/// prompt already carries verbatim.
#[test]
fn an_exact_duplicate_of_the_projects_own_context_is_refused() {
    let (_dir, root) = note_fixture("note-verb-context", None);

    let run = run_note(&root, &root, Some("auth.3"), &["Run the gate before shipping."]);
    assert!(!run.status.success(), "got:\n{}", run.stdout);
    assert_eq!(run.status.code(), Some(1));
    assert!(!root.join("runtime/notes.md").exists(), "got:\n{}", store(&root));
}

/// §FS-rhei-note.4: restatement is the one exception, and it is one because
/// it costs the restater its own slot.
#[test]
fn the_same_text_is_accepted_through_restate() {
    let (_dir, root) = note_fixture("note-verb-restate", None);
    assert_success(&run_note(&root, &root, Some("auth.2"), &[TRAP]));
    assert_success(&run_note(&root, &root, Some("billing.1"), &["Money paths retry with keys."]));

    let restate = run_note(&root, &root, Some("billing.1"), &["--restate", "auth.2"]);
    assert_success(&restate);
    assert!(restate.stdout.contains("restated auth.2"), "got:\n{}", restate.stdout);
    assert!(store(&root).ends_with("- [billing.1 restates auth.2]\n"), "got:\n{}", store(&root));
}

/// §FS-rhei-note.2: removal authority is project-wide — the store charges
/// every plan, so whoever pays may stop the charge.
#[test]
fn a_task_in_another_rhei_may_strike_an_entry() {
    let (_dir, root) = note_fixture("note-verb-strike", None);
    assert_success(&run_note(&root, &root, Some("auth.2"), &[TRAP]));

    let strike = run_note(&root, &root, Some("reporting.1"), &["--strike", "auth.2"]);
    assert_success(&strike);
    assert!(strike.stdout.contains("struck auth.2"), "got:\n{}", strike.stdout);
    // §FS-rhei-note.3.4: out of composition, and still in the file.
    let written = store(&root);
    assert!(written.contains(&format!("- [auth.2] {TRAP}\n")), "got:\n{written}");
    assert!(written.contains("- [reporting.1 strikes auth.2]\n"), "got:\n{written}");
}

/// §FS-rhei-note.1: exactly one form per call. Two is a usage error, and so
/// is none.
#[test]
fn exactly_one_form_is_given() {
    let (_dir, root) = note_fixture("note-verb-forms", None);

    let both = run_note(&root, &root, Some("auth.2"), &["--restate", "auth.1", "A fact."]);
    assert_eq!(both.status.code(), Some(2), "got:\n{}", both.stderr);
    // Named, so the refusal is about the forms rather than about the verb: an
    // unknown subcommand exits 2 as well, and would pass this case for free.
    assert!(both.stderr.contains("--restate"), "got:\n{}", both.stderr);

    let neither = run_note(&root, &root, Some("auth.2"), &[]);
    assert_eq!(neither.status.code(), Some(2), "got:\n{}", neither.stderr);
    assert!(neither.stderr.contains("--restate"), "got:\n{}", neither.stderr);

    assert!(!root.join("runtime/notes.md").exists(), "got:\n{}", store(&root));
}

/// §FS-rhei-note.4: a `--restate` naming nothing live is accepted and inert.
/// The entry may be struck already, or may arrive later; a writer holds no
/// lock on the fold.
#[test]
fn a_record_naming_no_live_entry_is_accepted_and_inert() {
    let (_dir, root) = note_fixture("note-verb-inert", None);

    let restate = run_note(&root, &root, Some("billing.1"), &["--restate", "auth.2"]);
    assert_success(&restate);
    assert_eq!(store(&root), "- [billing.1 restates auth.2]\n");
}

/// §FS-rhei-note.3.3: two writers contend for the sidecar and both records
/// land whole. Two processes rather than two threads, because the hold is an
/// `fs2` lock on a file and that is the thing under test (§REQ-cross-platform).
#[test]
fn two_concurrent_writers_both_land_a_record() {
    let (_dir, root) = note_fixture("note-verb-concurrent", None);

    let spawn = |task: &str, text: &str| {
        let mut cmd = isolated_command(&root);
        cmd.current_dir(&root);
        cmd.arg("note").arg(text).env("RHEI_TASK_ID", task);
        cmd.spawn().expect("rhei note should start")
    };
    let first = spawn("auth.2", "The first writer got here.");
    let second = spawn("reporting.1", "The second writer got here too.");
    for mut child in [first, second] {
        let status = child.wait().expect("rhei note should finish");
        assert!(status.success(), "both writers succeed; got {status:?}");
    }

    let written = store(&root);
    assert!(written.contains("- [auth.2] The first writer got here.\n"), "got:\n{written}");
    assert!(
        written.contains("- [reporting.1] The second writer got here too.\n"),
        "got:\n{written}"
    );
    assert_eq!(written.lines().filter(|line| line.starts_with("- [")).count(), 2);
}
