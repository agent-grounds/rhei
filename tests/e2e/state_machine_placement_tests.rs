// Where a state machine can be kept: beside a single-file plan, at a rhei's own
// execution root, at the project root for the default — and nowhere else
// without `--state-machine`. Each case lays one placement down on a temporary
// tree and runs the real binary against it. Every machine uses states the
// built-in machine lacks, so a plan that validates can only have loaded the
// file the case placed. The spec once recommended `docs/states.yaml`, which
// resolution has never searched (#110).
//
// Every case here carries a `**States:**` declaration, because every one of
// them predates the rule that made the declaration unnecessary, and under the
// deprecation window that declaration is still what resolves them: these are
// therefore the regression controls for the window, and each resolves to
// exactly the machine it did before. Three of them now also print one
// `warning:` line on stderr, which none of them asserts on; the trees where
// the *placement* alone resolves are in `state_machine_resolution_tests.rs`,
// and the warnings themselves in `state_machine_deprecation_window_tests.rs`.

// §FS-rhei-state-machine-writer.5 §FS-rhei-states-deprecation.1

use std::path::{Path, PathBuf};

use super::new_tests::{assert_failure, flattened_output};
use super::*;

/// A machine named `name` whose two states are `working` and `done`. Callers
/// pick names the built-in machine lacks.
fn machine(name: &str, working: &str, done: &str) -> String {
    format!(
        "name: {name}\nversion: 1\nstates:\n  {working}:\n    initial: true\n    \
         description: Not a built-in state\n  {done}:\n    final: true\n    \
         description: Done\ntransitions:\n  - from: {working}\n    to: {done}\n"
    )
}

/// Run `rhei <args>` from `cwd`, with no `--state-machine` unless `args` passes
/// one: the placement is what the case is about.
fn rhei_in(cwd: &Path, home: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(home);
    cmd.current_dir(cwd).args(args);
    let output = cmd.output().expect("rhei command should run");
    CliRun::from(&output)
}

fn assert_validates(result: &CliRun) {
    assert_success(result);
    assert!(
        result.stdout.contains("Validation succeeded"),
        "validation should succeed; got:\n{}\n{}",
        result.stdout,
        result.stderr
    );
}

/// A single-file plan declaring `custom`, on a state only `custom` has.
fn write_single_plan(dir: &Path) {
    write_fixture_file(
        dir,
        "plan.rhei.md",
        "# Rhei: Placement\n**States:** custom\n\n## Tasks\n\n\
         ### Task 1: Placed\n**State:** sketching\n",
    );
}

/// A Panta project running two machines: the project default `alpha` at the
/// project root, which `audit` runs under by declaring nothing, and `billing`'s
/// own `custom` at its execution root. Neither machine shares a state with the
/// other or with the built-in one, so each rhei validates only on its own file.
fn two_machine_project(dir: &Path) -> PathBuf {
    let project = dir.join("project");
    for rhei in ["audit", "billing"] {
        std::fs::create_dir_all(project.join(rhei).join("tasks")).expect("create the rhei");
    }
    write_fixture_file(&project, "index.panta.md", "# Panta: Two Machines\n**States:** alpha\n");
    write_fixture_file(&project, "states.yaml", &machine("alpha", "surveying", "signed-off"));

    let audit = project.join("audit");
    write_fixture_file(&audit, "index.rhei.md", "# Rhei: Audit\n");
    write_fixture_file(&audit.join("tasks"), "01.md", "### Task 1: Survey\n**State:** surveying\n");

    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** custom\n");
    write_fixture_file(&billing, "states.yaml", &machine("custom", "drafting", "filed"));
    write_fixture_file(&billing.join("tasks"), "01.md", "### Task 1: Draft\n**State:** drafting\n");
    project
}

/// Beside a single-file plan: the `states.yaml` in the plan's own directory is
/// the machine — the issue's working case, and clause 1 for a plan whose
/// execution root is the directory holding it. §FS-rhei-plan-language.1.3
#[test]
fn a_machine_beside_a_single_file_plan_is_found() {
    let dir = unique_temp_dir("placement-single");
    let tree = dir.join("tree");
    std::fs::create_dir_all(&tree).expect("create the tree");
    write_single_plan(&tree);
    write_fixture_file(&tree, "states.yaml", &machine("custom", "sketching", "shipped"));

    assert_validates(&rhei_in(&tree, &dir.join(".home"), &["validate", "plan.rhei.md"]));
}

/// At a rhei's own execution root: a Directory Workspace's root is that root,
/// and the `states.yaml` there is its machine. §FS-rhei-plan-language.1.3
#[test]
fn a_machine_at_a_directory_workspace_root_is_found() {
    // The helper's own `states.yaml` sits beside the workspace, not in it, and
    // is named for another machine: it cannot be the file that satisfies this.
    let (dir, ws, _outside) = create_workspace(
        "placement-workspace",
        "# Rhei: Placement\n**States:** custom\n",
        &[("01.md", "### Task 1: Placed\n**State:** sketching\n")],
    );
    write_fixture_file(&ws, "states.yaml", &machine("custom", "sketching", "shipped"));

    let ws_arg = ws.display().to_string();
    assert_validates(&rhei_in(&dir, &dir.join(".home"), &["validate", &ws_arg]));
}

/// The two placements side by side: the default at the project root and one
/// rhei's own machine at that rhei's execution root are both found, and
/// `rhei states` names both files. §FS-rhei-plan-language.1.3
#[test]
fn a_project_default_and_a_rheis_own_machine_are_both_found() {
    let dir = unique_temp_dir("placement-project");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    let project_arg = project.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    let default_source = format!("Source: '{}'", project.join("states.yaml").display());
    let own_source = format!(
        "Source: '{}' (rhei: billing)",
        project.join("billing").join("states.yaml").display()
    );
    for source in [&default_source, &own_source] {
        assert!(
            states.stdout.lines().any(|line| line == source.as_str()),
            "`rhei states` should list {source:?}; got:\n{}",
            states.stdout
        );
    }
}

/// A member may opt into the built-in machine even when its Panta project has
/// a differently named default. The two vocabularies are deliberately
/// disjoint: `audit` proves omitted declarations still inherit `alpha`, while
/// `billing` proves an explicit `rhei` declaration reaches the built-in
/// machine without a matching file. §FS-rhei-plan-language.1.3
#[test]
fn a_member_declaring_rhei_under_a_custom_default_falls_back_to_builtin() {
    let dir = unique_temp_dir("placement-member-builtin-fallback");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** rhei\n");
    std::fs::remove_file(billing.join("states.yaml")).expect("remove the member machine");
    write_fixture_file(
        &billing.join("tasks"),
        "01.md",
        "### Task 1: Built-in work\n**State:** pending\n",
    );
    let project_arg = project.display().to_string();

    assert_validates(&rhei_in(&dir, &dir.join(".home"), &["validate", &project_arg]));
}

/// The built-in fallback is last: a member-local definition named `rhei`
/// remains authoritative when it exists. Its `drafting` state is absent from
/// both the built-in machine and the project's `alpha` default.
/// §FS-rhei-plan-language.1.3
#[test]
fn a_member_declaring_rhei_prefers_its_matching_file_to_the_builtin() {
    let dir = unique_temp_dir("placement-member-rhei-file");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** rhei\n");
    write_fixture_file(&billing, "states.yaml", &machine("rhei", "drafting", "filed"));

    let project_arg = project.display().to_string();
    assert_validates(&rhei_in(&dir, &dir.join(".home"), &["validate", &project_arg]));
}

/// Nothing that errors stops erroring: a declaration naming a machine nothing
/// supplies, with no file at the rhei's own root either, is still a resolution
/// error rather than a fall through to the project default.
/// §FS-rhei-plan-language.1.3
#[test]
fn a_member_declaring_an_unknown_machine_without_a_file_is_rejected() {
    let dir = unique_temp_dir("placement-member-unknown");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(
        &billing,
        "index.rhei.md",
        "# Rhei: Billing\n**States:** missing-member-machine\n",
    );
    std::fs::remove_file(billing.join("states.yaml")).expect("remove the member machine");
    let project_arg = project.display().to_string();

    let result = rhei_in(&dir, &dir.join(".home"), &["validate", &project_arg]);
    assert_failure(&result, "missing-member-machine");
    let said = flattened_output(&result);
    assert!(
        said.contains("no states file declaring it was found"),
        "an unknown member machine should keep the missing-definition diagnostic; got:\n{said}"
    );
}

/// An explicitly declaring member gets local-file precedence even when its
/// machine has the project default's name. Validation accepts the local-only
/// state, and member-scoped inspection reports only that member's process.
/// §FS-rhei-plan-language.1.3
#[test]
fn an_explicit_same_name_member_uses_its_local_machine() {
    let dir = unique_temp_dir("placement-restated-default");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** alpha\n");
    write_fixture_file(&billing, "states.yaml", &machine("alpha", "drafting", "filed"));
    write_fixture_file(&billing.join("tasks"), "01.md", "### Task 1: Draft\n**State:** drafting\n");
    let project_arg = project.display().to_string();

    let validation = rhei_in(&dir, &home, &["validate", &project_arg]);
    let states = rhei_in(&dir, &home, &["states", &project_arg, "--rhei", "billing"]);
    let own_source = format!("Source: '{}' (rhei: billing)", billing.join("states.yaml").display());
    assert!(
        validation.status.success()
            && validation.stdout.contains("Validation succeeded")
            && states.status.success()
            && states.stdout.lines().any(|line| line == own_source)
            && states.stdout.contains("drafting")
            && states.stdout.contains("filed")
            && !states.stdout.contains("surveying")
            && !states.stdout.contains("signed-off"),
        "the member should validate and inspect through {own_source:?}\n\
         validate stdout:\n{}\nvalidate stderr:\n{}\n\
         states stdout:\n{}\nstates stderr:\n{}",
        validation.stdout,
        validation.stderr,
        states.stdout,
        states.stderr
    );
}

/// Whole-project inspection groups by complete content, regardless of source
/// path, leaving the default unqualified for the omitted `audit` member.
/// Narrowed inspection retains `billing`'s selected local source.
/// §FS-rhei-states-cmd.3 §FS-rhei-plan-language.1.3
#[test]
fn same_name_member_grouping_uses_content_independent_of_source() {
    let dir = unique_temp_dir("placement-same-name-grouping");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** alpha\n");
    write_fixture_file(&billing, "states.yaml", &machine("alpha", "drafting", "filed"));
    let project_arg = project.display().to_string();
    let default_source = format!("Source: '{}'", project.join("states.yaml").display());
    let local_source =
        format!("Source: '{}' (rhei: billing)", billing.join("states.yaml").display());

    for identical in [false, true] {
        if identical {
            std::fs::copy(project.join("states.yaml"), billing.join("states.yaml"))
                .expect("copy identical content to a different source path");
            write_fixture_file(
                &billing.join("tasks"),
                "01.md",
                "### Task 1: Survey locally\n**State:** surveying\n",
            );
        }
        assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

        let whole = rhei_in(&dir, &home, &["states", &project_arg]);
        assert_success(&whole);
        let expected_sources = if identical {
            vec![default_source.as_str()]
        } else {
            vec![default_source.as_str(), local_source.as_str()]
        };
        let sources =
            whole.stdout.lines().filter(|line| line.starts_with("Source:")).collect::<Vec<_>>();
        assert_eq!(sources, expected_sources, "identical={identical}:\n{}", whole.stdout);
        assert_eq!(
            whole.stdout.matches("State machine: alpha (version: 1)").count(),
            expected_sources.len(),
            "one block per distinct complete machine; identical={identical}:\n{}",
            whole.stdout
        );

        let narrowed = rhei_in(&dir, &home, &["states", &project_arg, "--rhei", "billing"]);
        assert_success(&narrowed);
        let sources =
            narrowed.stdout.lines().filter(|line| line.starts_with("Source:")).collect::<Vec<_>>();
        assert_eq!(
            sources,
            vec![local_source.as_str()],
            "identical={identical}:\n{}",
            narrowed.stdout
        );
        assert_eq!(
            narrowed.stdout.matches("State machine: alpha (version: 1)").count(),
            1,
            "narrowed inspection should show only billing's machine:\n{}",
            narrowed.stdout
        );
    }
}

/// A declaration restating the project default still resolves, so the member
/// keeps the default where it has no local candidate — and, in the second
/// iteration, where its own root holds a `beta` that clause 1 would otherwise
/// take: the window defers that file by a release and says so in a warning
/// this case does not read. §FS-rhei-states-deprecation.2.2
#[test]
fn same_name_member_without_a_matching_local_file_uses_the_project_default() {
    for local_machine in [None, Some(machine("beta", "queuing", "settled"))] {
        let dir = unique_temp_dir("placement-restated-default-fallback");
        let home = dir.join(".home");
        let project = two_machine_project(&dir);
        let billing = project.join("billing");
        write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** alpha\n");
        write_fixture_file(
            &billing.join("tasks"),
            "01.md",
            "### Task 1: Survey\n**State:** surveying\n",
        );
        match local_machine {
            Some(contents) => {
                write_fixture_file(&billing, "states.yaml", &contents);
            }
            None => std::fs::remove_file(billing.join("states.yaml"))
                .expect("remove the member-local machine"),
        }
        let project_arg = project.display().to_string();

        assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));
        let states = rhei_in(&dir, &home, &["states", &project_arg, "--rhei", "billing"]);
        assert_success(&states);
        let default_source =
            format!("Source: '{}' (rhei: billing)", project.join("states.yaml").display());
        assert!(
            states.stdout.lines().any(|line| line == default_source),
            "same-name fallback should report {default_source:?}; got:\n{}",
            states.stdout
        );
    }
}

/// A malformed member-local candidate is an error even when the explicit
/// declaration repeats the resolved project default's name.
/// §FS-rhei-plan-language.1.3
#[test]
fn an_invalid_same_name_local_candidate_reports_its_load_error() {
    let dir = unique_temp_dir("placement-restated-default-invalid");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** alpha\n");
    write_fixture_file(&billing, "states.yaml", "name: alpha\nstates: [\n");
    write_fixture_file(
        &billing.join("tasks"),
        "01.md",
        "### Task 1: Survey\n**State:** surveying\n",
    );
    let project_arg = project.display().to_string();

    let result = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_failure(&result, &billing.join("states.yaml").display().to_string());
}

/// The deprecated cross-root match, still resolving: with no project-root
/// file, the manifest's `alpha` is found in `billing`'s root and becomes the
/// default for the whole project, `audit` included. This resolution is the
/// previous release's and must not move; it now also prints the cross-root
/// warning, which this case does not read.
/// §FS-rhei-states-deprecation.2.3
#[test]
fn a_restated_default_found_in_the_rheis_own_root_runs_from_there() {
    let dir = unique_temp_dir("placement-adopted-default");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    std::fs::remove_file(project.join("states.yaml")).expect("remove the project-root machine");
    write_fixture_file(
        &project.join("audit").join("tasks"),
        "01.md",
        "### Task 1: Survey\n**State:** drafting\n",
    );
    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** alpha\n");
    write_fixture_file(&billing, "states.yaml", &machine("alpha", "drafting", "filed"));
    let project_arg = project.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    let own_source = format!("Source: '{}'", billing.join("states.yaml").display());
    assert!(
        states.stdout.lines().any(|line| line == own_source),
        "`rhei states` should list {own_source:?} as the default's source; got:\n{}",
        states.stdout
    );
}

/// A declaration that resolves to the built-in machine counts as resolving, so
/// a project whose effective default is built-in `rhei` does not adopt a
/// matching machine found only in a member root — including that member's own.
/// `agent-grounds/rhei#244`'s contract, carried through the window: the
/// inheriting member remains valid on `pending`, the restating member's
/// `drafting` is rejected, and `rhei states` reports the built-in source.
/// §FS-rhei-states-deprecation.1
#[test]
fn issue_244_contract_builtin_project_default_ignores_a_member_only_rhei_machine() {
    let dir = unique_temp_dir("placement-builtin-project-default");
    let home = dir.join(".home");
    let project = dir.join("project");
    for rhei in ["audit", "billing"] {
        std::fs::create_dir_all(project.join(rhei).join("tasks")).expect("create the rhei");
    }
    write_fixture_file(&project, "index.panta.md", "# Panta: Built-in Default\n**States:** rhei\n");

    let audit = project.join("audit");
    write_fixture_file(&audit, "index.rhei.md", "# Rhei: Audit\n");
    write_fixture_file(&audit.join("tasks"), "01.md", "### Task 1: Audit\n**State:** pending\n");

    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n**States:** rhei\n");
    write_fixture_file(&billing, "states.yaml", &machine("rhei", "drafting", "filed"));
    write_fixture_file(&billing.join("tasks"), "01.md", "### Task 1: Draft\n**State:** drafting\n");

    let project_arg = project.display().to_string();
    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert!(
        states.stdout.contains("Source: the built-in default state machine")
            && !states.stdout.contains(&billing.join("states.yaml").display().to_string()),
        "`rhei states` should select the built-in source, not the member file; got:\n{}",
        states.stdout
    );
    println!("passing `rhei states` control:\n{}", states.stdout);

    let validation = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_failure(&validation, "drafting");
    let said = flattened_output(&validation);
    assert!(
        said.contains("built-in default state machine")
            && said.contains("invalid state 'drafting'")
            && said.contains("Allowed: [pending, completed]"),
        "validation should reject the member-only custom state under built-in rhei; got:\n{said}"
    );
    println!("passing `rhei validate` control:\n{said}");
}

/// Nowhere else: `docs/` is not one of the three places, so the plan fails with
/// the not-found error and the same file loads only through `--state-machine` —
/// the issue's cases A and C. §FS-rhei-plan-language.1.3
#[test]
fn a_machine_under_docs_is_found_only_through_the_override() {
    let dir = unique_temp_dir("placement-docs");
    let home = dir.join(".home");
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("docs")).expect("create the tree");
    write_single_plan(&tree);
    write_fixture_file(
        &tree.join("docs"),
        "states.yaml",
        &machine("custom", "sketching", "shipped"),
    );

    let discovered = rhei_in(&tree, &home, &["validate", "plan.rhei.md"]);
    assert_failure(&discovered, "custom");
    let said = flattened_output(&discovered);
    assert!(
        said.contains("plan declares state machine 'custom', but no auto-discovered states file"),
        "a machine under docs/ should not be found; got:\n{said}"
    );

    let overridden =
        rhei_in(&tree, &home, &["--state-machine", "docs/states.yaml", "validate", "plan.rhei.md"]);
    assert_validates(&overridden);
}

/// `--state-machine` replaces resolution for the whole project, so a machine
/// kept outside its rhei's root cannot be supplied as one machine among
/// several: the override meets the default's `alpha` and the project fails.
/// §FS-rhei-plan-language.1.3
#[test]
fn the_override_cannot_supply_one_machine_among_several() {
    let dir = unique_temp_dir("placement-override-scope");
    let project = two_machine_project(&dir);
    let shared = dir.join("shared");
    std::fs::create_dir_all(&shared).expect("create the shared directory");
    let kept_elsewhere = shared.join("custom.yaml");
    std::fs::rename(project.join("billing").join("states.yaml"), &kept_elsewhere)
        .expect("move billing's machine out of its root");

    let override_arg = kept_elsewhere.display().to_string();
    let project_arg = project.display().to_string();
    let result = rhei_in(
        &dir,
        &dir.join(".home"),
        &["--state-machine", &override_arg, "validate", &project_arg],
    );
    assert_failure(&result, "--state-machine");
    let said = flattened_output(&result);
    assert!(
        said.contains("'alpha'") && said.contains("'custom'"),
        "the override should meet the project default's declaration; got:\n{said}"
    );
}
