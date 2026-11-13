//! `rhei remove` under a machine whose artifact path puts a placeholder before
//! `{task_id}`: it cleans the ticket's own empty residue, keeps every sibling's
//! file, and refuses a path a longer id could stand in (agent-grounds/rhei#679).
//! §FS-rhei-remove.4.2 §FS-rhei-reset.2.1

use std::fs;

use super::new_tests::{assert_failure, empty_project, flattened_output, new_run};
use super::*;

/// A project of one rhei, `auth`, whose machine declares its output at `template`,
/// with the tickets `ids`, untouched, and `files` laid under it.
fn project(name: &str, template: &str, ids: &[&str], files: &[(&str, &str)]) -> TestDir {
    let dir = empty_project(name);
    let machine = format!(
        "name: pattern\nversion: 1\nstates:\n  pending:\n    initial: true\n    description: Open\n    \
         outputs:\n      - name: note\n        path: {template}\n  completed: {{ final: true, description: Done }}\n\
         transitions:\n  - {{ from: pending, to: completed, description: Done }}\n"
    );
    fs::write(dir.join("states.yaml"), machine).expect("the machine");
    let mut plan = String::from("# Rhei: Auth\n\n## Tasks\n");
    for id in ids {
        plan.push_str(&format!("\n### Task {id}: {id}\n**State:** pending\n"));
    }
    fs::write(dir.join("auth.rhei.md"), plan).expect("the plan");
    for (file, text) in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().expect("a parent")).expect("the parent");
        fs::write(path, text).expect("a fixture file");
    }
    dir
}

fn plan_has(dir: &Path, heading: &str) -> bool {
    fs::read_to_string(dir.join("auth.rhei.md")).expect("the plan").contains(heading)
}

/// Under `runtime/{state}/{task_id}.md` an untouched ticket's empty output is its
/// residue and goes, and a sibling's file in the same directory stays.
#[test]
fn a_placeholder_before_the_id_cleans_the_tickets_own_residue_and_keeps_a_siblings() {
    let files = [("runtime/pending/auth.1.md", ""), ("runtime/pending/auth.2.md", "work")];
    let dir = project("remove-pattern-own", "runtime/{state}/{task_id}.md", &["1", "2"], &files);

    let result = new_run(&["remove", "auth.1"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("deleted runtime/pending/auth.1.md"), "got:\n{}", result.stdout);
    assert!(!dir.join("runtime/pending/auth.1.md").exists(), "its own residue goes");
    assert_eq!(fs::read_to_string(dir.join("runtime/pending/auth.2.md")).expect("kept"), "work");
    assert!(!plan_has(&dir, "### Task 1:") && plan_has(&dir, "### Task 2:"));
}

/// A longer id's output the shorter id's pattern also matches is refused as
/// another ticket's, not cleaned.
#[test]
fn a_longer_ids_output_a_pattern_matches_is_refused_not_cleaned() {
    let files = [("runtime/pending/auth.build-docs-1.md", "work")];
    let template = "runtime/{state}/{task_id}-{visit_count}.md";
    let dir = project("remove-pattern-longer", template, &["build", "build-docs"], &files);

    let result = new_run(&["remove", "auth.build"], &dir);
    assert_failure(&result, "auth.build cannot be removed");
    let said = flattened_output(&result);
    assert!(
        said.contains("runtime/pending/auth.build-docs-1.md may belong to another ticket"),
        "got:\n{said}"
    );
    assert_eq!(fs::read_to_string(dir.join(files[0].0)).expect("kept"), "work");
    assert!(plan_has(&dir, "### Task build:"));
}

/// `{state}` reaches `runtime/supervise/` and `{visit_count}` takes `docs`, so
/// `auth.build`'s pattern matches `auth.build-docs`'s empty brief; it is never
/// cleaned as `auth.build`'s residue.
#[test]
fn a_longer_ids_empty_brief_is_never_cleaned_as_residue() {
    let files = [("runtime/supervise/auth.build-docs.md", "")];
    let template = "runtime/{state}/{task_id}-{visit_count}.md";
    let dir = project("remove-pattern-brief", template, &["build", "build-docs"], &files);

    let result = new_run(&["remove", "auth.build"], &dir);
    assert!(
        !result.stdout.contains("deleted runtime/supervise/auth.build-docs.md"),
        "got:\n{}",
        result.stdout
    );
    assert!(dir.join(files[0].0).is_file(), "auth.build-docs's brief stays");
    let said = flattened_output(&result);
    assert!(
        said.contains("runtime/supervise/auth.build-docs.md may belong to another ticket"),
        "got:\n{said}"
    );
}

/// The same pattern matches `auth.build-docs`'s result, which is not
/// `auth.build`'s history.
#[test]
fn a_longer_ids_result_is_not_read_as_the_shorter_ids_history() {
    let files = [("runtime/results/auth.build-docs.md", "result of docs")];
    let template = "runtime/{state}/{task_id}-{visit_count}.md";
    let dir = project("remove-pattern-result", template, &["build", "build-docs"], &files);

    let result = new_run(&["remove", "auth.build"], &dir);
    let said = flattened_output(&result);
    assert!(!said.contains("result: runtime/results/auth.build-docs.md"), "got:\n{said}");
    assert!(
        said.contains("runtime/results/auth.build-docs.md may belong to another ticket"),
        "got:\n{said}"
    );
    assert_eq!(fs::read_to_string(dir.join(files[0].0)).expect("kept"), "result of docs");
}
