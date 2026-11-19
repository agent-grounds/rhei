//! Explain project-wide overrides without changing what validation accepts.
//! §FS-rhei-validate.6.1

use std::fs;
use std::path::Path;

use super::new_tests::flattened_output;
use super::validation_override_support::*;
use super::*;

#[test]
#[ignore = "red until #753: explain the other rhei's displaced machine; implement removes this"]
fn validation_override_failure_explains_displaced_member_machine() {
    let dir = unique_temp_dir("rhei-753-failure");
    let project = project(&dir);
    let home = dir.join("home");
    let automatic = validate(&project, &home, "csa-selective", None);
    assert_success(&automatic);
    assert!(!flattened_output(&automatic).contains(CONTEXT));

    let selected = Path::new("csa-selective").join("states.yaml");
    let result = validate(&project, &home, "csa-selective", Some(&selected));
    assert_eq!(result.status.code(), Some(1), "{}", result.stderr);
    let said = flattened_output(&result);
    for (task, state) in [("approve", "approve-design"), ("publish", "publish")] {
        assert!(
            said.contains(&format!(
                "Task csa-loop-design.{task} has invalid state '{state}'. Allowed: [frame, implement, completed]"
            )),
            "the override must still judge the unnamed rhei:\n{said}"
        );
    }
    assert!(said.contains(&format!(
        "I validated this plan using '{}', but found a problem.",
        selected.display()
    )));
    let displaced = context(&result, &selected);
    assert_source(
        &displaced,
        &project,
        &Path::new("csa-loop-design").join("states.yaml"),
        "csa-loop-design",
    );
    assert!(!displaced.contains("(rhei: csa-selective)"), "source replaced itself");
}

#[test]
#[ignore = "red until #753: successful equivalent targets explain the override; implement removes this"]
fn validation_override_success_explains_equivalent_targets() {
    let dir = unique_temp_dir("rhei-753-targets");
    let project = project(&dir);
    let home = dir.join("home");
    let selected = union_override(&dir);
    let member = project.join("csa-selective");
    // Exercise all target forms before checking the new output contract.
    let results = [
        (&project, "csa-selective"),
        (&project, "csa-selective/index.rhei.md"),
        (&member, "."),
        (&member, "index.rhei.md"),
        (&project, "."),
        (&project, "index.panta.md"),
    ]
    .map(|(cwd, target)| {
        let result = validate(cwd, &home, target, Some(&selected));
        assert_success(&result);
        assert!(result.stdout.contains("Validation succeeded"));
        (cwd, result)
    });
    for (cwd, result) in results {
        let displaced = context(&result, &selected);
        let design = assert_source(
            &displaced,
            cwd,
            &project.join("csa-loop-design/states.yaml"),
            "csa-loop-design",
        );
        let selective = assert_source(
            &displaced,
            cwd,
            &project.join("csa-selective/states.yaml"),
            "csa-selective",
        );
        assert!(design < selective, "unstable source order");
    }
}

#[test]
#[ignore = "red until #753: provenance bypasses invalid YAML and groups default owners; implement removes this"]
fn validation_override_reports_bypassed_yaml_without_loading_it() {
    let dir = unique_temp_dir("rhei-753-bypass");
    let project = project(&dir);
    let home = dir.join("home");
    let selected = union_override(&dir);
    // Both automatic levels are malformed; describing them must never load them.
    fs::write(project.join("states.yaml"), "states: [not: valid: yaml").unwrap();
    fs::write(project.join("csa-loop-design/states.yaml"), "states: [broken").unwrap();
    member(&project, "zeta", &[("one", "frame")]);
    member(&project, "audit", &[("one", "frame")]);
    let result = validate(&project, &home, ".", Some(&selected));
    assert_success(&result);
    let displaced = context(&result, &selected);
    let default = assert_owned_source(
        &displaced,
        &project,
        Path::new("states.yaml"),
        "project default; rhei: audit, zeta",
    );
    let design = Path::new("csa-loop-design").join("states.yaml");
    let design = assert_source(&displaced, &project, &design, "csa-loop-design");
    assert!(default < design);
}

#[test]
#[ignore = "red until #753: identify built-in fallback and exclude the selected source; implement removes this"]
fn validation_override_names_builtin_and_does_not_replace_itself() {
    let dir = unique_temp_dir("rhei-753-builtin");
    let project = project(&dir);
    let home = dir.join("home");
    fs::remove_file(project.join("csa-loop-design/states.yaml")).unwrap();
    let own = project.join("csa-selective/states.yaml");
    fs::write(&own, machine("edited", &["frame", "approve-design", "publish", "completed"]))
        .unwrap();
    // The absolute override and relative automatic lookup select the same file.
    let result = validate(&project, &home, ".", Some(&own));
    assert_success(&result);
    let displaced = context(&result, &own);
    assert!(
        displaced.contains(
            "the built-in default state machine (project default; rhei: csa-loop-design)"
        ),
        "built-in fallback must name its real owner:\n{displaced}"
    );
    assert!(
        !displaced.contains("states.yaml"),
        "invented fallback or self replacement:\n{displaced}"
    );
}

#[test]
#[ignore = "red until #753: explain an override that displaces no member source; implement removes this"]
fn validation_override_reports_no_displaced_source() {
    let dir = unique_temp_dir("rhei-753-no-displacement");
    let project = project(&dir);
    fs::remove_dir_all(project.join("csa-loop-design")).unwrap();
    let selected = project.join("states.yaml");
    fs::rename(project.join("csa-selective/states.yaml"), &selected).unwrap();
    let result = validate(&project, &dir.join("home"), ".", Some(&selected));
    assert_success(&result);
    let said = flattened_output(&result);
    assert_header(&said, &selected);
    assert_eq!(said.matches("No automatic sources displaced.").count(), 1, "{said}");
    assert!(!said.contains(DISPLACED), "invented replacements:\n{said}");
}

/// Only the initial failed watch pass is needed: wait for its existing end
/// marker, never for new text whose absence would cause a timeout. No file
/// edits, debounce sleeps or revalidation timing are involved.
#[test]
#[ignore = "red until #753: watch passes explain displaced sources exactly once; implement removes this"]
fn validation_override_watch_pass_explains_displaced_sources_once() {
    use super::provider_limit_support::{wait_for, RunningChild};

    let dir = unique_temp_dir("rhei-753-watch");
    let project = project(&dir);
    let selected = Path::new("csa-selective").join("states.yaml");
    let mut cmd = command(&project, &dir.join("home"), "csa-selective", Some(&selected));
    cmd.arg("--watch");
    let mut child = RunningChild::spawn(&mut cmd, &dir, "watch");
    let stderr = dir.join("watch.stderr");
    wait_for("complete initial validation failure", &mut child, || {
        stderr_from_file(&stderr).contains("fix the errors above, then re-check with:")
    });
    child.stop();
    let said =
        child.output().replace('\u{2502}', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(said.contains("Watch mode started"), "{said}");
    assert!(
        said.contains("Task csa-loop-design.approve has invalid state 'approve-design'"),
        "{said}"
    );
    let displaced = context_text(&said, &selected);
    assert_source(
        &displaced,
        &project,
        &Path::new("csa-loop-design").join("states.yaml"),
        "csa-loop-design",
    );
}
