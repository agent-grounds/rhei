//! Resolution and conflict cases for task read exclusions.
//! §FS-rhei-plan-language.3.13 §FS-rhei-validate.4

use std::fs;

use super::*;

/// Cross-rhei references use the same qualification rules as consumed exports,
/// and resolve from declarations before the file exists. §FS-rhei-plan-language.3.13
#[test]
fn cross_rhei_future_export_exclusion_resolves_from_the_project_graph() {
    let dir = unique_temp_dir("exclude-cross-rhei");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Cross-rhei exclusions\n");
    write_fixture_file(
        &dir,
        "source.rhei.md",
        "# Rhei: Source\n\n## Tasks\n\n### Task publish: Publish\n**State:** pending\n**Provides:** statement\n",
    );
    write_fixture_file(
        &dir,
        "review.rhei.md",
        "# Rhei: Review\n\n## Tasks\n\n### Task blind: Review blind\n**State:** pending\n**Excludes:** source.publish:statement\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    let validate = run_cli("validate", &dir, &machine, &[]);
    assert_success(&validate);
}

/// A local dotted task id wins over a same-named rhei prefix, matching the
/// existing task/export qualifier. §AR-rhei-panta.3 §FS-rhei-plan-language.3.13
#[test]
fn local_dotted_task_wins_when_an_exclusion_is_qualified() {
    let dir = unique_temp_dir("exclude-local-wins");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Local precedence\n");
    write_fixture_file(
        &dir,
        "source.rhei.md",
        "# Rhei: Other source\n\n## Tasks\n\n### Task publish: Other\n**State:** pending\n",
    );
    write_fixture_file(
        &dir,
        "review.rhei.md",
        r#"# Rhei: Review

---
structure:
  maxLevels: 2
---

## Tasks

### Task source: Local parent
**State:** pending

#### Task source.publish: Local producer
**State:** pending
**Provides:** statement

### Task blind: Review blind
**State:** pending
**Excludes:** source.publish:statement
"#,
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    let validate = run_cli("validate", &dir, &machine, &[]);
    assert_success(&validate);
}

/// Existing symlink aliases canonicalize to one target and are rejected as a
/// duplicate rather than weakening the boundary. §FS-rhei-plan-language.3.13
#[cfg(unix)]
#[test]
fn symlink_aliases_are_duplicate_exclusions() {
    use std::os::unix::fs::symlink;

    let plan = r#"# Rhei: Alias

## Tasks

### Task 1: Blind
**State:** pending
**Excludes:** artifact=real/private.md, artifact=alias/private.md
"#;
    let (dir, plan_path, machine) = setup_single_file("exclude-alias", plan);
    fs::create_dir_all(dir.join("real")).expect("real directory");
    fs::write(dir.join("real/private.md"), "private\n").expect("private file");
    symlink(dir.join("real"), dir.join("alias")).expect("directory symlink");

    let validate = run_cli("validate", &plan_path, &machine, &[]);
    assert!(!validate.status.success());
    assert!(validate.stderr.contains("same canonical target"), "got:\n{}", validate.stderr);
}

/// Escapes through an existing symlink are rejected even when a missing leaf
/// requires longest-existing-ancestor resolution. §FS-rhei-plan-language.3.13
#[cfg(unix)]
#[test]
fn symlink_escape_with_missing_leaf_is_rejected() {
    use std::os::unix::fs::symlink;

    let plan = r#"# Rhei: Escape

## Tasks

### Task 1: Blind
**State:** pending
**Excludes:** artifact=outside/future.md
"#;
    let (dir, plan_path, machine) = setup_single_file("exclude-symlink-escape", plan);
    let outside = unique_temp_dir("exclude-outside");
    symlink(&outside, dir.join("outside")).expect("escaping symlink");

    let validate = run_cli("validate", &plan_path, &machine, &[]);
    assert!(!validate.status.success());
    assert!(validate.stderr.contains("escapes artifact root"), "got:\n{}", validate.stderr);
}

/// Named native inheritance is deliberately incompatible in v1 because the
/// inherited transcript can already contain excluded bytes. §FS-rhei-snapshots.11
#[test]
fn named_snapshot_inheritance_conflicts_with_task_exclusions() {
    let plan = r#"# Rhei: Snapshot conflict

## Tasks

### Task 1: Worker
**State:** review
**Excludes:** checkout=private.md
"#;
    let machine = r#"name: snapshot-conflict
version: 1
states:
  source:
    initial: true
    target: fake:acme:model-a
    snapshot:
      emit: { name: impl, on: always }
  review:
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: impl
        required: true
        select: { state: source }
  completed: { final: true }
transitions:
  - { from: source, to: review }
  - { from: review, to: completed }
"#;
    let (dir, plan_path, machine_path) = setup_single_file("exclude-snapshot", plan);
    fs::write(&machine_path, machine).expect("snapshot machine");
    fs::create_dir_all(dir.join(".agent-grounds/rhei")).expect("settings directory");
    fs::write(
        dir.join(".agent-grounds/rhei/settings.json"),
        r#"{
  "agents": {
    "fake": {
      "command": ["fake"],
      "session": {
        "resume": {"flag": "--resume"},
        "session_dir_flag": "--session-dir",
        "layout": {"kind": "FlatById", "ext": "jsonl"}
      }
    }
  }
}"#,
    )
    .expect("snapshot settings");

    let validate = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(!validate.status.success());
    assert!(
        validate.stderr.contains("snapshot.inherit") && validate.stderr.contains("Excludes"),
        "got:\n{}",
        validate.stderr
    );
}

/// Optional inputs may be omitted, while required handoffs remain conflicts.
/// §FS-rhei-plan-language.3.13 §FS-rhei-states.3.2
#[test]
fn optional_input_can_be_excluded_but_required_handoff_cannot() {
    let optional_plan = r#"# Rhei: Optional input

## Tasks

### Task 1: Worker
**State:** pending
**Excludes:** artifact=runtime/optional.md
"#;
    let optional_machine = r#"name: optional-input
version: 1
states:
  pending:
    initial: true
    inputs:
      - { name: notes, path: runtime/optional.md, optional: true }
  completed: { final: true }
transitions: [{ from: pending, to: completed }]
"#;
    let (_dir, plan_path, machine_path) = setup_single_file("exclude-optional", optional_plan);
    fs::write(&machine_path, optional_machine).expect("optional machine");
    assert_success(&run_cli("validate", &plan_path, &machine_path, &[]));

    let handoff_plan = r#"# Rhei: Required handoff

## Tasks

### Task 1: Worker
**State:** review
**Excludes:** artifact=runtime/handoffs/plan.1/implementation.md
"#;
    let handoff_machine = r#"name: required-handoff
version: 1
states:
  implement:
    initial: true
    outputs:
      - { name: implementation, kind: handoff, path: runtime/handoffs/{task_id}/implementation.md }
  review:
    handoff:
      inherit:
        - { from: transition.previous, required: true }
  completed: { final: true }
transitions:
  - { from: implement, to: review }
  - { from: review, to: completed }
"#;
    let (_dir, plan_path, machine_path) = setup_single_file("exclude-handoff", handoff_plan);
    fs::write(&machine_path, handoff_machine).expect("handoff machine");
    let validate = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(!validate.status.success());
    assert!(validate.stderr.contains("required handoff"), "got:\n{}", validate.stderr);
}
