//! `--into` and the target's own-root machine, once resolution consults that
//! root whatever the index says.
//!
//! The interim clause that made `--into` write a `**States:**` line goes with
//! the reason for it: a union written into the target's root file now governs
//! the target, so there is nothing left to declare. The first two cases are
//! what that costs — a silent index stays silent, and a target with no file of
//! its own is refused with the one remedy that makes it eligible. The last two
//! are the cases that did *not* change, carried over from the file the interim
//! clause took with it: a declaration the root file agrees with is unioned
//! silently, and one it contradicts is refused naming both.
//! §FS-rhei-library.2.1

use super::into_support::*;
use super::*;

/// A project with a default machine and one member. `member_declares` decides
/// whether that member carries a `**States:** host` line of its own.
/// Returns (dir, member root).
fn project_with_member(prefix: &str, member_declares: bool) -> (TestDir, std::path::PathBuf) {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "index.panta.md", "# Panta: Work\n**States:** host\n");
    write_fixture_file(&dir, "states.yaml", HOST_MACHINE);
    let root = dir.join("release");
    std::fs::create_dir_all(root.join("tasks")).expect("create member");
    let index = if member_declares {
        HOST_INDEX.to_owned()
    } else {
        HOST_INDEX.replace("**States:** host\n", "")
    };
    write_fixture_file(&root, "index.rhei.md", &index);
    write_fixture_file(&root.join("tasks"), "001-ticket.md", HOST_TICKET);
    (dir, root)
}

/// The member that declares nothing, which the two cases above are about.
fn silent_member(prefix: &str) -> (TestDir, std::path::PathBuf) {
    project_with_member(prefix, false)
}

/// The root file exists and the index declares nothing: the union goes into that
/// file and **no declaration is written**, because the file alone is what
/// resolution reads now. §FS-rhei-library.2.1
#[test]
fn a_silent_index_stays_silent_when_a_union_lands_in_its_root() {
    let (dir, root) = silent_member("into-root-silent");
    write_fixture_file(&root, "states.yaml", HOST_MACHINE);
    write_review_template(&dir);

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);

    let index = read(&root.join("index.rhei.md"));
    assert!(
        !index.contains("**States:**"),
        "a union into the root file needs no declaration written; got:\n{index}"
    );
    assert!(
        read(&root.join("states.yaml")).contains("  review:"),
        "the union went into the rhei's own machine"
    );

    // And the rhei runs under what the union wrote, with nothing declared.
    assert_success(&run_into(&["validate", "release"], &dir));
}

/// No file in the rhei's root: refused with **one** remedy. The copy is now all
/// it takes, so printing a declaration to add beside it would tell the reader to
/// write a line that is going away. §FS-rhei-library.2.1
#[test]
fn no_machine_of_its_own_is_refused_with_one_remedy() {
    let (dir, root) = silent_member("into-root-none");
    write_review_template(&dir);
    assert!(!root.join("states.yaml").exists(), "the fixture member has no machine of its own");
    let index_before = read(&root.join("index.rhei.md"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "a target with no machine of its own must be refused");
    assert_stderr_contains(&result, "cp");
    assert_stderr_contains(&result, "release/states.yaml");
    assert_stderr_contains(&result, "project default");
    assert!(
        !result.stderr.contains("**States:**"),
        "the copy is the whole remedy; got:\n{}",
        result.stderr
    );
    assert_eq!(index_before, read(&root.join("index.rhei.md")), "the target is left alone");
    assert!(!root.join("states.yaml").exists(), "--into does not write the file for you");
}

/// Declared and matching: the union goes into that file and no declaration is
/// written or changed. Unchanged by the window, and the only test of it since
/// the interim clause's file went.
/// §FS-rhei-library.2.1
#[test]
fn a_declared_matching_machine_is_unioned_into_silently() {
    let (dir, root) = project_with_member("into-root-declared", true);
    write_fixture_file(&root, "states.yaml", HOST_MACHINE);
    write_review_template(&dir);
    let declaration_count = read(&root.join("index.rhei.md")).matches("**States:**").count();

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);

    let index = read(&root.join("index.rhei.md"));
    assert_eq!(
        index.matches("**States:**").count(),
        declaration_count,
        "a declaration that already matches is neither written nor changed; got:\n{index}"
    );
    assert!(
        read(&root.join("states.yaml")).contains("  review:"),
        "the union went into the rhei's own machine"
    );
}

/// The index declares a name the root file's `name:` does not match: refused
/// naming both. This is a pre-existing load error, not one `--into` introduces,
/// and the window makes the refusal *more* necessary, not less — so it must not
/// be the untested arm.
/// §FS-rhei-library.2.1
#[test]
fn a_declaration_the_root_file_contradicts_is_refused_naming_both() {
    let (dir, root) = project_with_member("into-root-mismatch", true);
    write_fixture_file(&root, "states.yaml", &HOST_MACHINE.replace("name: host", "name: other"));
    write_review_template(&dir);

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "a contradicted declaration must be refused");
    assert_stderr_contains(&result, "host");
    assert_stderr_contains(&result, "other");
    assert_stderr_contains(&result, "make them agree");
}
