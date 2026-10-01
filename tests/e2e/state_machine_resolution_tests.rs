//! The resolution rule itself: a rhei's machine is the `states.yaml` in its own
//! execution root, then the project root's, then the built-in machine.
//!
//! Every case here is a tree whose answer the previous rule got differently, or
//! one whose refusal the new rule must not weaken. The deprecation window that
//! runs ahead of this rule has its own file.

// §FS-rhei-plan-language.1.3

use super::new_tests::{assert_failure, flattened_output};
use super::state_machine_resolution_support::*;
use super::*;

/// The ticket's own premise. A manifest that declares nothing, holding one
/// member that declares nothing, whose own root holds a valid ticket machine:
/// the member runs under that file. Under the previous rule the file was inert —
/// the member inherited the project default wholesale and its own root was never
/// consulted — so a `cancelled` task the local machine declares final was judged
/// against the built-in machine and rejected.
///
/// The control is the same tree with one line added, `**States:** repro-ticket`,
/// which passed before this change and still passes: it is what isolates the
/// declaration as the single cause. §FS-rhei-plan-language.1.3
#[test]
fn a_members_own_machine_resolves_with_no_declaration_anywhere() {
    for declaration in [None, Some("repro-ticket")] {
        let dir = unique_temp_dir("resolution-own-root");
        let home = dir.join(".home");
        let root = project(&dir, None);
        let rhei = member(&root, "a-ticket", declaration, "completed");
        member_machine(&rhei, &ticket_machine("repro-ticket"));
        write_fixture_file(
            &rhei.join("tasks"),
            "02.md",
            "### Task 2: Abandoned\n**State:** cancelled\n",
        );
        let project_arg = root.display().to_string();

        assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

        let states = rhei_in(&dir, &home, &["states", &project_arg]);
        assert_success(&states);
        assert_source(&states, &rhei_source(&rhei.join("states.yaml"), "a-ticket"));
        assert!(
            states.stdout.contains("repro-ticket"),
            "the file's own `name:` is the effective machine name; got:\n{}",
            states.stdout
        );
    }
}

/// A `states.yaml` at the project root is the project default by its presence,
/// whether or not `index.panta.md` names it. It governs a member with no file of
/// its own and a single-file member alike. Under the previous rule a manifest
/// that declared nothing meant the built-in machine and the file beside it was
/// ignored. §FS-rhei-plan-language.1.3
#[test]
fn an_undeclared_project_root_machine_is_the_project_default() {
    let dir = unique_temp_dir("resolution-undeclared-default");
    let home = dir.join(".home");
    let root = project(&dir, None);
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    member(&root, "audit", None, "surveying");
    write_fixture_file(
        &root,
        "loose.rhei.md",
        "# Rhei: Loose\n\n## Tasks\n\n### Task 1: Surveyed\n**State:** surveying\n",
    );
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &default_source(&root.join("states.yaml")));
    assert!(
        !states.stdout.contains("Source: the built-in default state machine"),
        "the file beside the manifest is the default, not the built-in machine; got:\n{}",
        states.stdout
    );
}

/// A declaration naming a machine nothing supplies no longer fails where the
/// rhei's own root holds some other `states.yaml`: the own-root file resolves,
/// whatever its `name:`. The previous rule exited 1 here.
/// §FS-rhei-plan-language.1.3
#[test]
fn a_declaration_nothing_supplies_falls_through_to_the_own_root_file() {
    let dir = unique_temp_dir("resolution-unsupplied-declaration");
    let home = dir.join(".home");
    let root = project(&dir, None);
    let rhei = member(&root, "a-ticket", Some("grounded-ticket"), "drafting");
    member_machine(&rhei, &machine("agora", "drafting", "ruled"));
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &rhei_source(&rhei.join("states.yaml"), "a-ticket"));
    assert!(
        states.stdout.contains("agora"),
        "the resolved file's own name is the machine's name; got:\n{}",
        states.stdout
    );
}

/// Nothing that errors stops erroring. A declaration naming a machine nothing
/// supplies, with no file in the rhei's own root at all, is still a validation
/// error — the project-root file does not rescue it, because a declaration that
/// names one machine must never silently resolve to another.
/// §FS-rhei-plan-language.1.3
#[test]
fn a_declaration_nothing_supplies_with_no_own_file_is_still_an_error() {
    let dir = unique_temp_dir("resolution-unsupplied-no-file");
    let home = dir.join(".home");
    let root = project(&dir, None);
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    member(&root, "a-ticket", Some("missing-member-machine"), "surveying");
    let project_arg = root.display().to_string();

    let result = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_failure(&result, "missing-member-machine");
    let said = flattened_output(&result);
    assert!(
        said.contains("no states file declaring it was found"),
        "the missing-definition diagnostic survives the new rule; got:\n{said}"
    );
}

/// The basin is synthetic, so a `states.yaml` sitting in `basin/` is nobody's
/// own machine: the project default governs the basin's tickets, in the CLI and
/// in the rendered graph alike. Clause 1 reads a rhei's own execution root, and
/// the basin has no authored root to be one.
/// §FS-rhei-panta.2 §FS-rhei-plan-language.1.3
#[test]
fn a_machine_at_the_basins_root_does_not_govern_the_basin() {
    let dir = unique_temp_dir("resolution-basin-own-file");
    let home = dir.join(".home");
    let root = project(&dir, None);
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    member(&root, "audit", None, "surveying");
    std::fs::create_dir_all(root.join("basin")).expect("create the basin");
    write_fixture_file(
        &root.join("basin"),
        "001-loose.md",
        "### Task 1: Captured\n**State:** surveying\n",
    );
    // A machine whose only state is one the project default lacks, so a basin
    // running under it could not validate at all.
    write_fixture_file(
        &root.join("basin"),
        "states.yaml",
        &machine("basin-machine", "filed", "shelved"),
    );
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &default_source(&root.join("states.yaml")));
    assert!(
        !states.stdout.contains("basin-machine"),
        "the file in `basin/` is not a rhei's own machine; got:\n{}",
        states.stdout
    );

    let out = dir.join("view.html");
    let viz = rhei_in(
        &dir,
        &home,
        &["viz", &project_arg, "--output", out.to_str().expect("utf8 output path")],
    );
    assert_success(&viz);
    let html = std::fs::read_to_string(&out).expect("read the rendered view");
    assert!(
        !html.contains("basin-machine"),
        "the graph must not give the basin a machine the CLI does not"
    );
}

/// The control for `agent-grounds/rhei#314`, which this change is measured
/// against rather than fixing. With the project default fully resolved — a
/// declaration plus the matching file at the project root, which is what the new
/// clause 2 gives — `rhei summary` still resolves one project-level machine
/// instead of each ticket's owning rhei's, so a member's `cancelled` task is
/// counted inside `N in progress` and the workflow is named after the project
/// default. The tally below is today's and is pinned here so that this change
/// cannot move it in either direction; what it reports is #314's defect, not a
/// contract worth keeping. §FS-rhei-summary.2.1
#[test]
fn summary_still_resolves_one_project_machine() {
    let dir = unique_temp_dir("resolution-summary-control");
    let home = dir.join(".home");
    let root = project(&dir, Some("proj-machine"));
    // A project default with no `cancelled` state, so that resolving the default
    // correctly cannot by itself make the tally right.
    project_machine(&root, &machine("proj-machine", "pending", "completed"));
    let rhei = member(&root, "a-ticket", Some("repro-ticket"), "completed");
    member_machine(&rhei, &ticket_machine("repro-ticket"));
    write_fixture_file(
        &rhei.join("tasks"),
        "02.md",
        "### Task 2: Abandoned\n**State:** cancelled\n",
    );
    let project_arg = root.display().to_string();

    let summary = rhei_in(&dir, &home, &["summary", &project_arg]);
    assert_success(&summary);
    let first = summary.stdout.lines().next().unwrap_or_default();
    assert!(
        first.contains("`proj-machine` workflow")
            && first.contains("1 task completed, 1 in progress")
            && !first.contains("cancelled"),
        "#314 is left exactly where it is: summary names the project default and \
         counts the member's cancelled task as in progress; got:\n{first}"
    );
}
