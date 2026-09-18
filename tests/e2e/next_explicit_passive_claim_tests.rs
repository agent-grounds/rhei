//! Explicit claims from non-initial states, including the issue-286 fixture.

use std::fs;

use super::*;

const PASSIVE_MACHINE: &str = r#"name: flat
version: 3
states:
  start:
    description: Setup state
  bridge:
    description: Passive bridge state
  staging:
    description: A second passive state
  work:
    description: Runnable work
    target: codex:openai:gpt-5.5
  done:
    description: Finished work
    final: true
transitions:
  - from: start
    to: bridge
  - from: bridge
    to: work
  - from: staging
    to: work
  - from: work
    to: done
profiles:
  primary:
    initial: start
    allowed: [start, bridge, staging, work, done]
node_policy:
  root: primary
  default: primary
"#;

fn single_task_fixture(
    prefix: &str,
    state: &str,
    machine: &str,
) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = format!(
        "# Rhei: Explicit passive claim\n\n## Tasks\n\n### Task job: Claim me\n**State:** {state}\n"
    );
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    (dir, plan_path, machine_path)
}

/// The portable form of the issue-286 reproducer strengthens its exit check
/// with the durable state, owner, ledger, and duplicate-claim boundary.
// §FS-rhei-next.3 §FS-rhei-next.3.1 §FS-rhei-plan-language.3.10
#[test]
fn issue_286_explicit_passive_directory_claim_advances_owns_and_cannot_be_reclaimed() {
    let index = "# Rhei: Flat passive bridge\n**States:** flat\n";
    let task = "### Task job: Claim after passive bridge\n**State:** bridge\n";
    let dir = unique_temp_dir("next-explicit-passive-directory");
    let workspace = dir.join("workspace");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace");
    fs::write(workspace.join("index.rhei.md"), index).expect("write index");
    fs::write(workspace.join("tasks/job.md"), task).expect("write task");
    let machine_path = write_fixture_file(&workspace, "states.yaml", PASSIVE_MACHINE);

    let validation = run_cli("validate", &workspace, &machine_path, &[]);
    assert_success(&validation);

    let claimed = run_cli("next", &workspace, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert_success(&claimed);
    assert!(
        claimed.stdout.contains("Task workspace.job claimed: 'bridge' -> 'work'"),
        "claim output did not report the bounded transition:\n{}",
        claimed.stdout
    );
    let persisted = fs::read_to_string(workspace.join("tasks/job.md")).expect("read task");
    assert!(
        persisted.contains("**State:** work\n**Assignee:** codex"),
        "state and resolved owner were not durable together:\n{persisted}"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("runtime/state-transitions.log")).expect("read ledger"),
        "workspace.job bridge@work\n"
    );

    let duplicate =
        run_cli("next", &workspace, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert!(!duplicate.status.success(), "a second claim must be refused");
    assert_stderr_contains(&duplicate, "Task workspace.job is already assigned to codex");
}

/// Automatic selection keeps its initial-state restriction, while explicit
/// peek can inspect the same ready non-initial task without locks or writes.
// §FS-rhei-next.3
#[test]
fn issue_286_explicit_passive_peek_is_read_only_and_automatic_selection_still_skips() {
    let (dir, plan_path, machine_path) =
        single_task_fixture("next-explicit-passive-peek", "bridge", PASSIVE_MACHINE);
    let original = fs::read_to_string(&plan_path).expect("read original plan");

    let automatic = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!automatic.status.success(), "automatic selection must skip non-initial work");
    assert_stderr_contains(&automatic, "mid-workflow in state 'bridge'");

    let peeked = run_cli("next", &plan_path, &machine_path, &["--task", "job", "--peek"]);
    assert_success(&peeked);
    assert!(peeked.stdout.contains("current state: 'bridge'"), "got:\n{}", peeked.stdout);
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), original);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// A claim stops after its first selected edge even when the result is another
/// passive state with an immediately applicable edge of its own.
// §FS-rhei-next.3 §FS-rhei-next.3.1
#[test]
fn issue_286_explicit_passive_claim_advances_exactly_one_edge() {
    let machine = PASSIVE_MACHINE
        .replace("  - from: bridge\n    to: work", "  - from: bridge\n    to: staging");
    let (dir, plan_path, machine_path) =
        single_task_fixture("next-explicit-passive-one-edge", "bridge", &machine);

    let claimed = run_cli("next", &plan_path, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert_success(&claimed);
    assert_task_state(&plan_path, &machine_path, "job", "staging");
    let persisted = fs::read_to_string(&plan_path).expect("read plan");
    assert!(persisted.contains("**State:** staging\n**Assignee:** manual"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.job bridge@staging\n"
    );
}

/// A non-initial state that already declares execution is claimed in place.
// §FS-rhei-next.3
#[test]
fn issue_286_explicit_non_initial_runnable_state_is_claimed_in_place() {
    let (dir, plan_path, machine_path) =
        single_task_fixture("next-explicit-runnable-in-place", "work", PASSIVE_MACHINE);

    let claimed = run_cli("next", &plan_path, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert_success(&claimed);
    assert_task_state(&plan_path, &machine_path, "job", "work");
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** work\n**Assignee:** codex"));
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// A passive non-initial state does not turn a terminal edge into completion.
// §FS-rhei-next.3
#[test]
fn issue_286_explicit_passive_terminal_first_target_is_claimed_in_place() {
    let machine =
        PASSIVE_MACHINE.replace("  - from: bridge\n    to: work", "  - from: bridge\n    to: done");
    let (dir, plan_path, machine_path) =
        single_task_fixture("next-explicit-terminal-target", "bridge", &machine);

    let claimed = run_cli("next", &plan_path, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert_success(&claimed);
    assert_task_state(&plan_path, &machine_path, "job", "bridge");
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** bridge\n**Assignee:** manual"));
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// An edge disabled by its condition is not a reason to reject an otherwise
/// ready explicit claim; the worker receives the current state.
// §FS-rhei-next.3
#[test]
fn issue_286_explicit_passive_without_an_applicable_edge_is_claimed_in_place() {
    let machine = PASSIVE_MACHINE
        .replace(
            "  bridge:\n    description: Passive bridge state",
            "  bridge:\n    description: Passive bridge state\n    visits: 2",
        )
        .replace(
            "  - from: bridge\n    to: work",
            "  - from: bridge\n    to: work\n    condition: visitCount >= visits",
        );
    let (dir, plan_path, machine_path) =
        single_task_fixture("next-explicit-no-applicable-edge", "bridge", &machine);

    let claimed = run_cli("next", &plan_path, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert_success(&claimed);
    assert_task_state(&plan_path, &machine_path, "job", "bridge");
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** bridge\n**Assignee:** manual"));
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// A parent becomes eligible only after its subtree closes, then follows the
/// same bounded passive-state claim rule as a leaf.
// §FS-rhei-next.3.4
#[test]
fn issue_286_explicit_passive_parent_advances_after_descendants_are_terminal() {
    let plan = r#"# Rhei: Explicit passive parent

---
structure:
  maxLevels: 3
---

## Tasks

### Task parent: Coordinate
**State:** bridge

#### Task parent.child: Finished child
**State:** done
"#;
    let dir = unique_temp_dir("next-explicit-passive-parent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PASSIVE_MACHINE);

    let claimed =
        run_cli("next", &plan_path, &machine_path, &["--task", "parent", "--no-callbacks"]);
    assert_success(&claimed);
    assert_task_state(&plan_path, &machine_path, "parent", "work");
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** work\n**Assignee:** codex"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.parent bridge@work\n"
    );
}

/// Explicit selection does not bypass successful-prior readiness merely
/// because the selected task is already beyond its initial state.
// §FS-rhei-next.3
#[test]
fn issue_286_explicit_non_initial_claim_still_requires_successful_priors() {
    let plan = r#"# Rhei: Explicit passive prior

## Tasks

### Task prerequisite: Still open
**State:** bridge

### Task job: Blocked work
**State:** bridge
**Prior:** Task prerequisite
"#;
    let dir = unique_temp_dir("next-explicit-passive-prior");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PASSIVE_MACHINE);

    let refused = run_cli("next", &plan_path, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert!(!refused.status.success(), "an incomplete prior must refuse the claim");
    assert_stderr_contains(&refused, "blocked by incomplete prerequisites");
    assert_stderr_contains(&refused, "waiting on Task plan.prerequisite (bridge)");
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), plan);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// Release drops ownership only: automatic selection still skips the resulting
/// non-initial state, while a later explicit claim can take it in place.
// §FS-rhei-next.3 §FS-rhei-release.3.1
#[test]
fn issue_286_released_non_initial_task_is_explicitly_reclaimable_only() {
    let (dir, plan_path, machine_path) =
        single_task_fixture("next-explicit-reclaim-after-release", "bridge", PASSIVE_MACHINE);

    assert_success(&run_cli(
        "next",
        &plan_path,
        &machine_path,
        &["--task", "job", "--no-callbacks"],
    ));
    assert_success(&run_cli("release", &plan_path, &machine_path, &["--task", "job"]));

    let automatic = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!automatic.status.success(), "release must not add non-initial work to auto selection");
    assert_stderr_contains(&automatic, "mid-workflow in state 'work'");

    let reclaimed =
        run_cli("next", &plan_path, &machine_path, &["--task", "job", "--no-callbacks"]);
    assert_success(&reclaimed);
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** work\n**Assignee:** codex"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.job bridge@work\n",
        "re-claiming runnable work must not invent a second transition"
    );
}
