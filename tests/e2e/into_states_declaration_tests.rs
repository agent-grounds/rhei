//! `--into` and the target's **effective** state machine.
//! §FS-rhei-library.2.1
//!
//! Machine resolution in this slice is exactly today's, and under
//! §FS-rhei-plan-language.1.3 rule 2 a rhei that omits `**States:**` inherits
//! the project default wholesale with its own root *not consulted*. So a
//! `states.yaml` a union wrote into such a rhei would be inert, and `--into` has
//! to say so rather than write a file nothing reads. These are the three cases
//! and the one pre-existing load error.
//!
//! The declaration-writing case is the interim: it exists because resolution is
//! still today's, and it goes when resolution is simplified.

use super::into_support::*;
use super::*;

/// A project with a default machine and one member. Returns (dir, member root).
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

/// Declared and matching: the union goes into that file and no declaration is
/// written or changed. §FS-rhei-library.2.1
#[test]
fn a_declared_matching_machine_is_unioned_into_silently() {
    let (dir, root) = project_with_member("into-states-declared", true);
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

/// The root file exists but the index declares nothing: the union is written and
/// `--into` adds the declaration in the same write, saying so in the summary.
/// This is the interim clause. §FS-rhei-library.2.1
#[test]
fn a_silent_index_with_a_root_file_gets_the_declaration_written_and_reported() {
    let (dir, root) = project_with_member("into-states-silent", false);
    write_fixture_file(&root, "states.yaml", HOST_MACHINE);
    write_review_template(&dir);
    assert!(
        !read(&root.join("index.rhei.md")).contains("**States:**"),
        "the fixture's index declares nothing"
    );

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert_success(&result);

    let index = read(&root.join("index.rhei.md"));
    assert!(
        index.contains("**States:** host"),
        "the declaration should be added so the union is not inert; got:\n{index}"
    );
    assert!(
        result.stdout.contains("**States:** host") || result.stdout.contains("States: host"),
        "the summary should say the declaration was added; got:\n{}",
        result.stdout
    );

    let validate = run_into(&["validate", "release"], &dir);
    assert_success(&validate);
}

/// No file in the rhei's root: refused, printing **both** remedies, because the
/// copy alone changes nothing while resolution is today's.
/// §FS-rhei-library.2.1
#[test]
fn no_machine_of_its_own_is_refused_with_both_remedies() {
    let (dir, root) = project_with_member("into-states-none", false);
    write_review_template(&dir);
    assert!(!root.join("states.yaml").exists(), "the fixture member has no machine of its own");
    let index_before = read(&root.join("index.rhei.md"));

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "a target with no machine of its own must be refused");
    // Remedy one: the copy.
    assert_stderr_contains(&result, "cp");
    assert_stderr_contains(&result, "release/states.yaml");
    // Remedy two: the declaration, without which the copy is inert.
    assert_stderr_contains(&result, "**States:**");
    // And the consequence, so nobody follows the remedy without knowing it.
    assert_stderr_contains(&result, "project default");
    assert_eq!(index_before, read(&root.join("index.rhei.md")), "the target is left alone");
    assert!(!root.join("states.yaml").exists(), "--into does not write the file for you");
}

/// The index declares a name the root file's `name:` does not match: refused
/// naming both. This is a pre-existing load error, not one `--into` introduces,
/// and the point of pinning it is that `--into` must not paper over it.
/// §FS-rhei-library.2.1
#[test]
fn a_declaration_the_root_file_contradicts_is_refused_naming_both() {
    let (dir, root) = project_with_member("into-states-mismatch", true);
    write_fixture_file(&root, "states.yaml", &HOST_MACHINE.replace("name: host", "name: other"));
    write_review_template(&dir);

    let result =
        run_into(&["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"], &dir);
    assert!(!result.status.success(), "a contradicted declaration must be refused");
    assert_stderr_contains(&result, "host");
    assert_stderr_contains(&result, "other");
}
