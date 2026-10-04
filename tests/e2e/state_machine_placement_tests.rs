// Where a state machine can be kept: beside a single-file plan, at a rhei's own
// execution root, at the project root for the default — and nowhere else
// without `--state-machine`. Each case lays one placement down on a temporary
// tree and runs the real binary against it. Every machine uses states the
// built-in machine lacks, so a plan that validates can only have loaded the
// file the case placed. The spec once recommended `docs/states.yaml`, which
// resolution has never searched (#110).
//
// No document here names a machine: where the file sits is the whole binding.

// §FS-rhei-state-machine-writer.5 §FS-rhei-plan-language.1.3

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

/// A single-file plan on a state only `custom` has.
fn write_single_plan(dir: &Path) {
    write_fixture_file(
        dir,
        "plan.rhei.md",
        "# Rhei: Placement\n\n## Tasks\n\n### Task 1: Placed\n**State:** sketching\n",
    );
}

/// A Panta project running two machines: the project default `alpha` at the
/// project root, which `audit` runs under with no file of its own, and `billing`'s
/// own `custom` at its execution root. Neither machine shares a state with the
/// other or with the built-in one, so each rhei validates only on its own file.
fn two_machine_project(dir: &Path) -> PathBuf {
    let project = dir.join("project");
    for rhei in ["audit", "billing"] {
        std::fs::create_dir_all(project.join(rhei).join("tasks")).expect("create the rhei");
    }
    write_fixture_file(&project, "index.panta.md", "# Panta: Two Machines\n");
    write_fixture_file(&project, "states.yaml", &machine("alpha", "surveying", "signed-off"));

    let audit = project.join("audit");
    write_fixture_file(&audit, "index.rhei.md", "# Rhei: Audit\n");
    write_fixture_file(&audit.join("tasks"), "01.md", "### Task 1: Survey\n**State:** surveying\n");

    let billing = project.join("billing");
    write_fixture_file(&billing, "index.rhei.md", "# Rhei: Billing\n");
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
        "# Rhei: Placement\n",
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

/// A member's own file governs it even when its machine has the project
/// default's name. Validation accepts the local-only state, and member-scoped
/// inspection reports only that member's process.
/// §FS-rhei-plan-language.1.3
#[test]
fn an_explicit_same_name_member_uses_its_local_machine() {
    let dir = unique_temp_dir("placement-restated-default");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
    write_fixture_file(&billing, "states.yaml", &machine("alpha", "drafting", "filed"));
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

/// A malformed member-local candidate is an error even when it names the
/// project default's machine: nothing falls through past a file that will not
/// load.
/// §FS-rhei-plan-language.1.3
#[test]
fn an_invalid_same_name_local_candidate_reports_its_load_error() {
    let dir = unique_temp_dir("placement-restated-default-invalid");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    let billing = project.join("billing");
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

/// Another rhei's root never supplies the project default. With no
/// project-root file the default is the built-in machine, so `audit`'s
/// `drafting` is rejected even though `billing`'s own root holds a machine
/// that has it — which governs `billing` alone. §FS-rhei-plan-language.1.3
#[test]
fn another_rheis_root_never_supplies_the_project_default() {
    let dir = unique_temp_dir("placement-no-cross-root");
    let home = dir.join(".home");
    let project = two_machine_project(&dir);
    std::fs::remove_file(project.join("states.yaml")).expect("remove the project-root machine");
    write_fixture_file(
        &project.join("audit").join("tasks"),
        "01.md",
        "### Task 1: Survey\n**State:** drafting\n",
    );
    let billing = project.join("billing");
    write_fixture_file(&billing, "states.yaml", &machine("alpha", "drafting", "filed"));
    let project_arg = project.display().to_string();

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert!(
        states.stdout.contains("Source: the built-in default state machine"),
        "`rhei states` should report the built-in default; got:\n{}",
        states.stdout
    );

    let validation = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_failure(&validation, "drafting");
    let said = flattened_output(&validation);
    assert!(
        said.contains("invalid state 'drafting'"),
        "audit's state is judged by the built-in default; got:\n{said}"
    );
}

/// Nowhere else: `docs/` is not one of the three places, so the plan runs under
/// the built-in machine, which rejects its state, and the same file loads only
/// through `--state-machine` — the issue's cases A and C.
/// §FS-rhei-plan-language.1.3
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
    assert_failure(&discovered, "sketching");
    let said = flattened_output(&discovered);
    assert!(
        said.contains("invalid state 'sketching'"),
        "a machine under docs/ should not be found; got:\n{said}"
    );

    let overridden =
        rhei_in(&tree, &home, &["--state-machine", "docs/states.yaml", "validate", "plan.rhei.md"]);
    assert_validates(&overridden);
}

/// `--state-machine` replaces resolution for the whole project, so a machine
/// kept outside its rhei's root cannot be supplied as one machine among
/// several: the override governs `audit` too, which it cannot read.
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
    assert_failure(&result, "surveying");
    let said = flattened_output(&result);
    assert!(
        said.contains("invalid state 'surveying'"),
        "the override governs every rhei, audit included; got:\n{said}"
    );
}
