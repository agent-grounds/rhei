//! `rhei new --dir` adopting an authored state-machine workspace.
//! §FS-rhei-new.2.1.1

use std::fs;
use std::path::Path;

use super::new_tests::{assert_failure, empty_project, flattened_output, new_run};
use super::*;

fn machine(name: &str, initial: &str, final_state: &str) -> String {
    format!(
        "name: {name}\nversion: 1\nstates:\n  {initial}:\n    initial: true\n    \
         description: Work\n  {final_state}:\n    final: true\n    description: Done\n\
         transitions:\n  - from: {initial}\n    to: {final_state}\n"
    )
}

fn project(prefix: &str) -> TestDir {
    let dir = empty_project(prefix);
    write_fixture_file(&dir, "index.panta.md", "# Panta: Test\n**States:** alpha\n");
    write_fixture_file(&dir, "states.yaml", &machine("alpha", "surveying", "signed-off"));
    dir
}

fn prospective_billing(dir: &Path, states: &str) {
    fs::create_dir_all(dir.join("billing")).expect("create prospective workspace");
    write_fixture_file(&dir.join("billing"), "states.yaml", states);
}

fn entry_names(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path)
        .expect("read directory")
        .map(|entry| entry.expect("read entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn create_billing(dir: &Path, extra: &[&str]) -> CliRun {
    let mut args = vec!["new", "Billing", "--project", ".", "--dir", "--states", "custom"];
    args.extend_from_slice(extra);
    new_run(&args, dir)
}

/// The machine-first authoring order succeeds through the member execution
/// root, not through the project's different default machine.
/// §FS-rhei-new.1.2 §FS-rhei-new.2.1.1
#[test]
fn adopts_an_authored_machine_and_binds_the_new_rhei_to_it() {
    let dir = project("new-adopt-machine");
    let custom = machine("custom", "drafting", "filed");
    prospective_billing(&dir, &custom);

    let created = create_billing(&dir, &[]);
    assert_success(&created);
    assert_eq!(
        fs::read_to_string(dir.join("billing/index.rhei.md")).expect("workspace index"),
        "# Rhei: Billing\n**States:** custom\n"
    );
    assert_eq!(entry_names(&dir.join("billing/tasks")), Vec::<String>::new());
    assert_eq!(
        fs::read(dir.join("billing/states.yaml")).expect("authored machine"),
        custom.as_bytes()
    );
    assert_success(&new_run(&["validate", "."], &dir));

    let ticket = new_run(&["new", "First invoice", "--project", ".", "--under", "billing"], &dir);
    assert_success(&ticket);
    assert!(
        ticket.stdout.contains("[drafting]"),
        "custom initial state not selected:\n{}",
        ticket.stdout
    );
}

/// An empty real directory is the other admissible prospective workspace.
/// §FS-rhei-new.2.1.1
#[test]
fn adopts_an_empty_directory() {
    let dir = project("new-adopt-empty");
    fs::create_dir(dir.join("billing")).expect("create prospective workspace");

    let result = new_run(&["new", "Billing", "--project", ".", "--dir"], &dir);
    assert_success(&result);
    assert_eq!(entry_names(&dir.join("billing")), ["index.rhei.md", "tasks"]);
}

/// Retry after a failed create or dry run reuses the exact empty sidecar and
/// preserves it as permanent coordination state.
// §FS-rhei-new.2.1.1 §FS-rhei-new.5.1
#[test]
fn issue_95_adopts_a_workspace_containing_only_its_empty_index_sidecar() {
    let dir = project("new-adopt-sidecar");
    fs::create_dir(dir.join("billing")).expect("create prospective workspace");
    fs::write(dir.join("billing/index.rhei.md.lock"), b"").expect("sidecar");

    let result = new_run(&["new", "Billing", "--project", ".", "--dir"], &dir);
    assert_success(&result);
    assert_eq!(entry_names(&dir.join("billing")), ["index.rhei.md", "index.rhei.md.lock", "tasks"]);
}

/// Prompt templates are part of the authored machine bundle and survive
/// adoption byte-for-byte. §FS-rhei-new.2.1.1 §FS-rhei-new.5.1
#[test]
fn preserves_the_authored_machine_and_prompt_templates() {
    let dir = project("new-adopt-prompts");
    let custom = machine("custom", "drafting", "filed");
    prospective_billing(&dir, &custom);
    fs::create_dir(dir.join("billing/prompt_templates")).expect("create prompt templates");
    let prompt = b"Review the invoice exactly as authored.\r\n";
    fs::write(dir.join("billing/prompt_templates/review.md"), prompt).expect("write prompt");

    assert_success(&create_billing(&dir, &[]));
    assert_eq!(
        entry_names(&dir.join("billing")),
        ["index.rhei.md", "prompt_templates", "states.yaml", "tasks"]
    );
    assert_eq!(fs::read(dir.join("billing/states.yaml")).expect("machine"), custom.as_bytes());
    assert_eq!(fs::read(dir.join("billing/prompt_templates/review.md")).expect("prompt"), prompt);
}

/// A task directory, unrelated entry, or templates without a machine is an
/// occupied destination, not an existing rhei. §FS-rhei-new.2.1.1 §FS-rhei-new.4
#[test]
fn refuses_non_bundle_content_before_writing_and_names_the_obstruction() {
    for (prefix, obstruction, directory) in [
        ("new-adopt-tasks", "tasks", true),
        ("new-adopt-extra", "notes.md", false),
        ("new-adopt-prompts-only", "prompt_templates", true),
    ] {
        let dir = project(prefix);
        fs::create_dir(dir.join("billing")).expect("create prospective workspace");
        let path = dir.join("billing").join(obstruction);
        if directory {
            fs::create_dir(&path).expect("create obstructing directory");
        } else {
            fs::write(&path, b"authored notes\n").expect("write obstruction");
        }
        let before = entry_names(&dir.join("billing"));

        let result = new_run(&["new", "Billing", "--project", ".", "--dir"], &dir);
        assert_failure(&result, obstruction);
        let said = flattened_output(&result);
        assert!(!said.contains("already exists"), "a non-rhei was called a rhei:\n{said}");
        assert!(said.contains("move") || said.contains("remove"), "no next action:\n{said}");
        assert_eq!(entry_names(&dir.join("billing")), before);
        assert!(!dir.join("billing/index.rhei.md").exists());
    }
}

/// Actual rheis in either layout remain collisions no matter which layout the
/// new invocation requested. §FS-rhei-new.4
#[test]
fn protects_actual_single_file_and_directory_rheis_in_both_create_layouts() {
    let single = project("new-adopt-collision-single");
    write_fixture_file(&single, "billing.rhei.md", "# Rhei: Existing\n\n## Tasks\n");
    for extra in [None, Some("--dir")] {
        let mut args = vec!["new", "Billing", "--project", "."];
        args.extend(extra);
        assert_failure(&new_run(&args, &single), "already exists");
    }
    assert!(!single.join("billing").exists());

    let workspace = project("new-adopt-collision-dir");
    fs::create_dir_all(workspace.join("billing/tasks")).expect("create existing workspace");
    write_fixture_file(&workspace.join("billing"), "index.rhei.md", "# Rhei: Existing\n");
    for extra in [None, Some("--dir")] {
        let mut args = vec!["new", "Billing", "--project", "."];
        args.extend(extra);
        assert_failure(&new_run(&args, &workspace), "already exists");
    }
    assert_eq!(entry_names(&workspace.join("billing")), ["index.rhei.md", "tasks"]);
}

/// The default create remains single-file and reports an adoptable same-id
/// directory as a layout conflict with the useful alternative. §FS-rhei-new.2.1.1
#[test]
fn single_file_create_does_not_adopt_or_search_the_same_id_directory() {
    let dir = project("new-adopt-single-conflict");
    let custom = machine("custom", "drafting", "filed");
    prospective_billing(&dir, &custom);

    let result = new_run(&["new", "Billing", "--project", ".", "--states", "custom"], &dir);
    assert!(!result.status.success());
    let said = flattened_output(&result);
    assert!(
        said.contains("layout") && said.contains("--dir"),
        "not a useful layout conflict:\n{said}"
    );
    assert!(!said.contains("already exists"), "a non-rhei was called a rhei:\n{said}");
    assert!(!dir.join("billing.rhei.md").exists());
    assert_eq!(fs::read(dir.join("billing/states.yaml")).expect("machine"), custom.as_bytes());
}

/// Invalid content is admitted by shape and rejected by post-write validation;
/// normal rollback owns only the entries this invocation added.
/// §FS-rhei-new.2.1.1 §FS-rhei-new.5.2
#[test]
fn validation_failure_rolls_back_only_invocation_owned_entries() {
    for (prefix, authored) in [
        ("new-adopt-invalid", "not: [valid yaml\n".to_string()),
        ("new-adopt-mismatch", machine("other", "drafting", "filed")),
    ] {
        let dir = project(prefix);
        prospective_billing(&dir, &authored);

        let result = create_billing(&dir, &[]);
        assert!(!result.status.success());
        let said = flattened_output(&result);
        assert!(!said.contains("already exists"), "validation never ran:\n{said}");
        assert!(
            said.contains("states.yaml") || said.contains("state machine"),
            "wrong failure:\n{said}"
        );
        assert_eq!(entry_names(&dir.join("billing")), ["states.yaml"]);
        assert_eq!(
            fs::read(dir.join("billing/states.yaml")).expect("machine"),
            authored.as_bytes()
        );
    }
}

/// Dry run always removes its own index and task directory, even with
/// `--keep-on-error`, while preserving the adopted root and bundle.
/// §FS-rhei-new.5.4
#[test]
fn dry_run_preserves_an_adopted_workspace_on_success_and_failure() {
    let valid = project("new-adopt-dry-valid");
    let custom = machine("custom", "drafting", "filed");
    prospective_billing(&valid, &custom);
    assert_success(&create_billing(&valid, &["--dry-run", "--keep-on-error"]));
    assert_eq!(entry_names(&valid.join("billing")), ["states.yaml"]);
    assert_eq!(fs::read(valid.join("billing/states.yaml")).expect("machine"), custom.as_bytes());

    let invalid = project("new-adopt-dry-invalid");
    let broken = b"not: [valid yaml\n";
    prospective_billing(&invalid, std::str::from_utf8(broken).expect("utf8 fixture"));
    let result = create_billing(&invalid, &["--dry-run", "--keep-on-error"]);
    assert!(!result.status.success());
    assert!(!flattened_output(&result).contains("already exists"));
    assert_eq!(entry_names(&invalid.join("billing")), ["states.yaml"]);
    assert_eq!(fs::read(invalid.join("billing/states.yaml")).expect("machine"), broken);
}

/// Dry-run rollback restores plan data but keeps the destination sidecar and
/// the directory required to preserve that lock identity. `--keep-on-error`
/// cannot turn a dry run into retained plan data.
// §FS-rhei-new.5.4
#[test]
fn issue_95_dry_run_of_a_new_workspace_retains_only_coordination_state() {
    for extra in [&[][..], &["--keep-on-error"][..]] {
        let dir = project("new-dry-coordination");
        let mut args = vec!["new", "Billing", "--project", ".", "--dir", "--dry-run"];
        args.extend_from_slice(extra);

        let result = new_run(&args, &dir);
        assert_success(&result);
        assert!(!dir.join("billing/index.rhei.md").exists(), "plan data must roll back");
        assert!(!dir.join("billing/tasks").exists(), "owned plan directory must roll back");
        assert!(
            dir.join("billing/index.rhei.md.lock").is_file(),
            "the permanent destination sidecar must remain"
        );
        assert_eq!(entry_names(&dir.join("billing")), ["index.rhei.md.lock"]);
    }
}

/// A validation failure follows the same coordination/data ownership split,
/// whether ordinary rollback or dry-run's unconditional rollback selects it.
// §FS-rhei-new.5.2 §FS-rhei-new.5.4
#[test]
fn issue_95_failed_workspace_creation_retains_coordination_but_not_plan_data() {
    for extra in [&[][..], &["--dry-run", "--keep-on-error"][..]] {
        let dir = project("new-failed-coordination");
        let mut args = vec!["new", "Billing", "--project", ".", "--dir", "--states", "missing"];
        args.extend_from_slice(extra);

        let result = new_run(&args, &dir);
        assert!(!result.status.success(), "missing machine must fail");
        assert!(!dir.join("billing/index.rhei.md").exists(), "plan data must roll back");
        assert!(!dir.join("billing/tasks").exists(), "owned plan directory must roll back");
        assert!(dir.join("billing/index.rhei.md.lock").is_file(), "sidecar must remain");
        assert_eq!(entry_names(&dir.join("billing")), ["index.rhei.md.lock"]);
    }
}

/// Outside dry run, `--keep-on-error` keeps the newly created workspace
/// entries for inspection without changing the authored machine.
/// §FS-rhei-new.5.2
#[test]
fn keep_on_error_retains_new_entries_after_machine_validation_fails() {
    let dir = project("new-adopt-keep");
    let mismatched = machine("other", "drafting", "filed");
    prospective_billing(&dir, &mismatched);

    let result = create_billing(&dir, &["--keep-on-error"]);
    assert!(!result.status.success());
    assert!(!flattened_output(&result).contains("already exists"));
    assert!(dir.join("billing/index.rhei.md").is_file());
    assert!(dir.join("billing/tasks").is_dir());
    assert_eq!(fs::read(dir.join("billing/states.yaml")).expect("machine"), mismatched.as_bytes());
}
