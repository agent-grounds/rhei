// What a reset does to the note store: a project-wide reset takes it with the
// rest of `runtime/`, and a narrowed one keeps it and says so.
//
// Its own part because the two verbs disagree on purpose, and the disagreement
// is the point: the store is run state, but it is not ticket-owned.

// §FS-rhei-reset.2 §FS-rhei-reset.2.1 §FS-rhei-note.3.1

use std::fs;

use super::note_store_support::*;
use super::*;

/// §FS-rhei-reset.2: the store lives under `runtime/`, which step 4 deletes
/// wholesale, so reset's authored-state promise is exactly as it was. A
/// control — it passes today and must keep passing.
#[test]
fn a_project_wide_reset_removes_the_store() {
    let (_dir, root) = note_fixture("note-reset-all", Some(&store_contents()));
    let machine = root.join("states.yaml");

    let mut cmd = isolated_command(&root);
    cmd.current_dir(&root);
    cmd.arg("--state-machine").arg(&machine).arg("reset").arg(".").arg("--yes");
    let reset = CliRun::from(&cmd.output().expect("rhei reset should run"));
    assert_success(&reset);

    assert!(!root.join("runtime/notes.md").exists(), "got:\n{}", reset.stdout);
}

/// §FS-rhei-reset.2.1: a narrowed reset keeps the store whole and reports it,
/// the way it already reports the run-scoped output it keeps. It is one shared
/// journal charged to every plan, not an in-scope ticket's artifact.
#[test]
fn a_narrowed_reset_keeps_the_store_and_says_so() {
    let (_dir, root) = note_fixture("note-reset-narrow", Some(&store_contents()));
    let machine = root.join("states.yaml");

    let mut cmd = isolated_command(&root);
    cmd.current_dir(&root);
    cmd.arg("--state-machine").arg(&machine).arg("reset").arg(".");
    cmd.args(["--rhei", "auth", "--yes"]);
    let reset = CliRun::from(&cmd.output().expect("rhei reset should run"));
    assert_success(&reset);

    assert_eq!(
        fs::read_to_string(root.join("runtime/notes.md")).expect("the store survives"),
        store_contents()
    );
    assert!(
        reset.stdout.to_lowercase().contains("note"),
        "a narrowed reset says what it kept; got:\n{}",
        reset.stdout
    );
}
