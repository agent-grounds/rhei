//! Reset owns exact ticket export directories even without machine artifact
//! contracts. Ported from the rhei.123 intake reproducer for #703.
//! §FS-rhei-reset.2.1 §FS-rhei-panta.6.4

use std::fs;

use super::reset_remedy_support::RemedyWorld;
use super::{assert_stderr_contains, assert_success, CliRun};

// Deliberately no inputs/outputs: Provides exports must be cleaned in their own right.
const MACHINE: &str = r#"name: reset-exports
version: 1
states:
  pending: {initial: true}
  completed: {final: true}
  cancelled: {final: true}
transitions:
  - {from: pending, to: completed}
  - {from: pending, to: cancelled}
"#;

const ALPHA: &str = "# Rhei: Alpha\n\n## Tasks\n\n### Task 1: Publish a summary\n\
                     **State:** pending\n**Provides:** summary\n";
const BETA: &str = "# Rhei: Beta\n\n## Tasks\n\n### Task 1: Publish notes\n\
                    **State:** pending\n**Provides:** notes\n";

fn project(name: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    world.write("index.panta.md", "# Panta: Exports\n");
    world.write("states.yaml", MACHINE);
    world.write("alpha.rhei.md", ALPHA);
    world.write("beta.rhei.md", BETA);
    world.step("validate", &[]);
    world
}

fn complete(world: &RemedyWorld, id: &str) -> CliRun {
    world.rhei(
        "transition",
        &[
            "--task",
            id,
            "--from",
            "pending",
            "--to",
            "completed",
            "--result",
            "run finished",
            "--no-callbacks",
        ],
    )
}

fn first_run(world: &RemedyWorld) {
    world.write("runtime/exports/alpha.1/summary.md", "first alpha summary\n");
    world.write("runtime/exports/beta.1/notes.md", "beta notes stay byte-identical\n");
    assert_success(&complete(world, "alpha.1"));
    assert_success(&complete(world, "beta.1"));
}

fn assert_missing_export(world: &RemedyWorld) {
    let refused = complete(world, "alpha.1");
    assert!(
        !refused.status.success(),
        "completion after reset must refuse stale exports; exit {:?}\n{}{}",
        refused.status.code(),
        refused.stdout,
        refused.stderr
    );
    assert_stderr_contains(&refused, "Missing or blank declared exports");
    assert_stderr_contains(&refused, "runtime/exports/alpha.1/summary.md");
    assert_eq!(world.state_of("alpha.1"), "pending");
    assert!(!world.root.join("runtime/results/alpha.1.md").exists());
}

/// Initial refusal and the full-reset control precede the narrowed regression;
/// beta and an exact-id neighbour must survive, and fresh output permits retry.
/// §FS-rhei-reset.2.1 §FS-rhei-reset.3 §FS-rhei-plan-language.3.12.3
#[test]
#[ignore = "red until #703: narrowed reset removes stale exports; implement removes this"]
fn narrowed_reset_exports_require_fresh_completion_output() {
    let control = project("rhei-703-reset-export-control");
    assert_missing_export(&control);
    first_run(&control);
    control.step("reset", &["--yes"]);
    assert!(!control.root.join("runtime/exports/alpha.1").exists());
    assert!(!control.root.join("runtime/exports/beta.1").exists());
    assert_missing_export(&control);

    let world = project("rhei-703-reset-export-narrowed");
    first_run(&world);
    // An unselected directory with a longer id must not match alpha.1's target.
    world.write("runtime/exports/alpha.10/summary.md", "exact-id neighbour\n");
    let beta_plan = fs::read(world.root.join("beta.rhei.md")).expect("beta plan");
    world.step("reset", &["--rhei", "alpha", "--yes"]);
    assert_eq!(world.state_of("alpha.1"), "pending");
    assert_eq!(world.state_of("beta.1"), "completed");
    assert_eq!(fs::read(world.root.join("beta.rhei.md")).expect("beta plan"), beta_plan);
    assert_eq!(
        fs::read_to_string(world.root.join("runtime/exports/beta.1/notes.md")).unwrap(),
        "beta notes stay byte-identical\n"
    );
    assert_eq!(
        fs::read_to_string(world.root.join("runtime/exports/alpha.10/summary.md")).unwrap(),
        "exact-id neighbour\n"
    );
    assert_missing_export(&world);
    assert!(!world.root.join("runtime/exports/alpha.1").exists(), "remove the whole export home");
    world.write("runtime/exports/alpha.1/summary.md", "fresh second-run summary\n");
    assert_success(&complete(&world, "alpha.1"));
    assert_eq!(world.state_of("alpha.1"), "completed");
}

/// Both roots carry qualified output; only the owning root's bare local id is
/// unambiguous because beta owns the project root and is outside the scope.
/// §FS-rhei-reset.2.1 §FS-rhei-panta.6.4
#[test]
#[ignore = "red until #703: reset removes exports at both execution roots; implement removes this"]
fn narrowed_reset_exports_sweep_owning_and_project_roots() {
    let world = project("rhei-703-reset-export-roots");
    fs::remove_file(world.root.join("alpha.rhei.md")).unwrap();
    world.write("alpha/index.rhei.md", "# Rhei: Alpha\n");
    world.write(
        "alpha/tasks/01-summary.md",
        "### Task 1: Publish a summary\n**State:** pending\n**Provides:** summary\n",
    );
    world.step("validate", &[]);
    let removed =
        ["alpha/runtime/exports/alpha.1", "runtime/exports/alpha.1", "alpha/runtime/exports/1"];
    for path in removed {
        world.write(&format!("{path}/summary.md"), "old alpha output\n");
    }
    let kept = [
        "runtime/exports/1/summary.md",
        "runtime/exports/beta.1/notes.md",
        "alpha/runtime/exports/alpha.10/summary.md",
        "alpha/runtime/exports/10/summary.md",
    ];
    for path in kept {
        world.write(path, "outside the exact owned target\n");
    }
    world.step("reset", &["--rhei", "alpha", "--yes"]);
    for path in kept {
        assert_eq!(
            fs::read_to_string(world.root.join(path)).unwrap(),
            "outside the exact owned target\n",
            "must preserve {path}"
        );
    }
    let remaining: Vec<_> = removed.iter().filter(|path| world.root.join(path).exists()).collect();
    assert!(remaining.is_empty(), "reset left owned export directories: {remaining:?}");
}

/// Shared-root local ids stay ambiguous until every root owner is selected;
/// this cleans old residue without changing the export reader's rename hint.
/// §FS-rhei-panta.6.4 §FS-rhei-reset.2.1
#[test]
#[ignore = "red until #703: reset sweeps exports under guarded legacy ids; implement removes this"]
fn narrowed_reset_exports_guard_shared_root_legacy_ids() {
    let world = project("rhei-703-reset-export-legacy");
    world.write("runtime/exports/1/summary.md", "ambiguous legacy output\n");
    world.write("runtime/exports/10/summary.md", "different local id\n");
    world.step("reset", &["--rhei", "alpha", "--yes"]);
    assert_eq!(
        fs::read_to_string(world.root.join("runtime/exports/1/summary.md")).unwrap(),
        "ambiguous legacy output\n"
    );
    world.step("reset", &["--rhei", "alpha", "--rhei", "beta", "--yes"]);
    assert_eq!(
        fs::read_to_string(world.root.join("runtime/exports/10/summary.md")).unwrap(),
        "different local id\n"
    );
    assert!(
        !world.root.join("runtime/exports/1").exists(),
        "all shared-root owners are selected, so legacy export directory 1 must be removed"
    );
}
