//! `--into` and the target's own-root machine, once resolution consults that
//! root whatever the index says.
//!
//! The interim clause that made `--into` write a `**States:**` line goes with
//! the reason for it: a union written into the target's root file now governs
//! the target, so there is nothing left to declare. These two cases are what
//! that costs — a silent index stays silent, and a target with no file of its
//! own is refused with the one remedy that makes it eligible.
//! §FS-rhei-library.2.1

use super::into_support::*;
use super::*;

/// A project with a default machine and one member that declares nothing.
/// Returns (dir, member root).
fn silent_member(prefix: &str) -> (TestDir, std::path::PathBuf) {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "index.panta.md", "# Panta: Work\n**States:** host\n");
    write_fixture_file(&dir, "states.yaml", HOST_MACHINE);
    let root = dir.join("release");
    std::fs::create_dir_all(root.join("tasks")).expect("create member");
    write_fixture_file(&root, "index.rhei.md", &HOST_INDEX.replace("**States:** host\n", ""));
    write_fixture_file(&root.join("tasks"), "001-ticket.md", HOST_TICKET);
    (dir, root)
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
