//! Rebinding a project's **default machine**: what a replacement must check
//! before it writes, and the manifests it must not lay a default under.
//! §FS-rhei-library.2.3 §FS-rhei-library.2.2
//!
//! Every rhei with no machine of its own and the basin inherit the default
//! wholesale, so replacing it changes what they run under. The replacement
//! therefore refuses before writing a byte when it would strand a ticket, and
//! which tickets it reads is resolution's answer rather than a rule of its own:
//! a member with a machine of its own is not the default's to strand. A
//! manifest still carrying the retired `**States:**` line is refused at parse.

use super::into_project_lay_support::*;
use super::*;

/// The machine each project here runs under before the rebind: its working
/// state `work` is one the laid machine does not have.
fn house_machine() -> String {
    machine("housemachine", "work", "task")
}

/// A single-file member and a basin ticket run under the default and hold a
/// state the replacement lacks, so the rebind refuses naming both with their
/// states, and the project is byte-identical. A member with a machine of its
/// own holding a ticket in the same state is not the default's, and is not
/// named. §FS-rhei-library.2.3 §FS-rhei-panta.2
#[test]
fn a_rebind_refuses_and_names_every_stranded_ticket() {
    let dir = unique_temp_dir("rebind-stranded");
    write_lifecycle_templates(&dir);
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&house_machine()));
    write_fixture_file(
        &project,
        "stranded.rhei.md",
        "# Rhei: Stranded\n\n## Tasks\n\n### Task one: Holds a state the new default lacks\n**State:** work\n",
    );
    std::fs::create_dir_all(project.join("basin")).expect("create the basin");
    write_fixture_file(
        &project.join("basin"),
        "001-capture.md",
        "### Task 1: An unfiled capture\n**State:** work\n",
    );
    write_member(
        &project,
        "own",
        "# Rhei: Own\n\n## Overview\n\nRuns under a machine of its own.\n",
        Some(&machine("ownmachine", "work", "task")),
        "### Task 1: Holds its own machine's state\n**State:** work\n",
    );
    assert_success(&rhei_in(&dir, &dir, &["validate", "reports"]));
    let before = snapshot(&project);

    let rebound = rhei_in(&dir, &project, &["instantiate", "lifecycle", "--into", "."]);
    assert!(
        !rebound.status.success(),
        "a replacement that strands a ticket is refused; got:\n{}",
        rebound.stdout
    );
    assert_stderr_contains(&rebound, "stranded.one");
    assert_stderr_contains(&rebound, "basin.1");
    assert_stderr_contains(&rebound, "work");
    assert!(
        !rebound.stderr.contains("own.1"),
        "a member with a machine of its own is not the default's to strand; got:\n{}",
        rebound.stderr
    );
    assert_same_tree(&before, &snapshot(&project), "a refused rebind writes nothing");
}

/// A replacement whose `by_type` names a node kind no rhei declares is refused
/// naming the member it would govern, the kind, and the `structure.nodeKinds`
/// entry to add. The command checks node kinds; it never edits another rhei's
/// frontmatter. §FS-rhei-library.2.3
#[test]
fn a_member_that_lacks_a_node_kind_the_new_default_names_is_refused() {
    let dir = unique_temp_dir("rebind-kinds");
    write_project_template(&dir, "kinds-lifecycle", &laid_machine(&["ticket"]), "");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&house_machine()));
    write_member(
        &project,
        "legacy",
        "# Rhei: Legacy\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - task\n---\n\n## Overview\n\nWritten by hand before the lifecycle.\n",
        None,
        "### Task 1: Already finished\n**State:** completed\n",
    );
    assert_success(&rhei_in(&dir, &dir, &["validate", "reports"]));
    let before = snapshot(&project);

    let rebound = rhei_in(&dir, &dir, &["instantiate", "kinds-lifecycle", "--into", "reports"]);
    assert!(
        !rebound.status.success(),
        "a default naming an undeclared kind is refused; got:\n{}",
        rebound.stdout
    );
    assert_stderr_contains(&rebound, "legacy");
    assert_stderr_contains(&rebound, "ticket");
    assert_stderr_contains(&rebound, "nodeKinds");
    assert_same_tree(&before, &snapshot(&project), "a refused rebind writes nothing");
}

/// A manifest still carrying the retired `**States:**` line is a parse error,
/// so the rebind is refused before writing, naming the line to delete.
/// §FS-rhei-library.2.2 §FS-rhei-plan-language.2.2
#[test]
fn a_manifest_carrying_the_retired_line_refuses_the_rebind() {
    let dir = unique_temp_dir("rebind-manifest-line");
    write_lifecycle_templates(&dir);
    let manifest =
        "# Panta: Reports\n**States:** housemachine\n\n## Overview\n\nDeclares its default.\n";
    let project = write_project(&dir, "reports", manifest, Some(&house_machine()));
    let before = snapshot(&project);

    let rebound = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports"]);
    assert!(
        !rebound.status.success(),
        "a manifest carrying the retired line is refused; got:\n{}",
        rebound.stdout
    );
    assert_stderr_contains(&rebound, "**States:**");
    assert_stderr_contains(&rebound, "delete this line");
    assert_same_tree(&before, &snapshot(&project), "a refused rebind writes nothing");
}
