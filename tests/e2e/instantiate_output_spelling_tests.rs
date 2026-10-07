//! Output spelling preserves immediate-child membership, project validation,
//! publication and settings ownership. §FS-rhei-templates.6.2
//!
//! Port of the rhei.51 reproducer: a one-task workspace and an unrelated
//! sibling with an invalid chapter after Tasks, using real writes from the
//! project root. Absolute and dot-relative destinations are controls for the
//! bare name whose immediate parent is empty.

use std::fs;
use std::path::Path;

use super::templates_tests::run_raw;
use super::*;

const SETTINGS: &str = ".agent-grounds/rhei/settings.json";
const PROJECT_SETTINGS: &str = r#"{
  "defaults": { "agent_timeout": "5m" },
  "agents": { "project-only": { "command": ["project-tool"] } }
}
"#;

fn fixture(project: &Path) -> PathBuf {
    let source = fixture_path("instantiate-output-spelling");
    let template = project.join("template/mini");
    copy_dir_recursive(&source.join("mini"), &template);
    write_fixture_file(project, "index.panta.md", "# Panta: Spelling\n");
    template
}

fn seed_settings(project: &Path) {
    fs::create_dir_all(project.join(".agent-grounds/rhei")).unwrap();
    write_fixture_file(project, SETTINGS, PROJECT_SETTINGS);
}

// Controls run first so a failure for bare `new` cannot hide a broken fixture.
fn outputs(project: &Path) -> [String; 3] {
    [project.join("new").to_str().unwrap().to_owned(), "./new".into(), "new".into()]
}

fn instantiate(project: &Path, template: &Path, output: &str) -> CliRun {
    run_raw(&["instantiate", template.to_str().unwrap(), "--output", output], project)
}

/// Successful real writes must both report membership and load as `new.1`,
/// keeping the workspace's machine and publishing under the intended id.
/// §FS-rhei-templates.6.2
#[test]
fn every_output_spelling_joins_a_clean_project() {
    let project = unique_temp_dir("instantiate-spelling-clean");
    let template = fixture(&project);
    for output in outputs(&project) {
        let result = instantiate(&project, &template, &output);
        assert_success(&result);
        assert!(project.join("new/index.rhei.md").is_file(), "{output}: member is published");
        let listed = run_raw(&["list"], &project);
        assert_success(&listed);
        assert!(
            listed.stdout.contains("new.1"),
            "{output}: intended member id:\n{}",
            listed.stdout
        );
        assert!(result.stdout.contains("Task new.1: Only [todo]"), "{output}: {}", result.stdout);
        assert!(
            result.stdout.contains("Added to the Panta project"),
            "--output {output}: membership must be reported; stdout:\n{}",
            result.stdout
        );
        assert_success(&run_raw(&["validate"], &project));
        eprintln!("PASS clean project --output {output}: published and reported member new.1");
        fs::remove_dir_all(project.join("new")).unwrap();
    }
}

fn refuses_malformed_sibling(existing_settings: bool) {
    let project = unique_temp_dir("instantiate-spelling-refusal");
    let template = fixture(&project);
    // A clean control for each spelling proves refusal is due to the sibling.
    for output in outputs(&project) {
        assert_success(&instantiate(&project, &template, &output));
        fs::remove_dir_all(project.join("new")).unwrap();
    }
    fs::remove_dir_all(project.join(".agent-grounds")).unwrap();
    if existing_settings {
        seed_settings(&project);
    }
    let previous = fs::read(project.join(SETTINGS)).ok();
    fs::copy(
        fixture_path("instantiate-output-spelling/broken-sibling.rhei.md"),
        project.join("broken-sibling.rhei.md"),
    )
    .unwrap();
    for output in outputs(&project) {
        let result = instantiate(&project, &template, &output);
        let published = project.join("new").exists();
        let unchanged = fs::read(project.join(SETTINGS)).ok() == previous;
        assert!(
            result.status.code() == Some(1)
                && result.stderr.contains("broken-sibling.rhei.md")
                && result.stderr.contains("Tasks section must be the final '##' chapter")
                && !published
                && unchanged,
            "--output {output}: expected exit 1 naming broken-sibling.rhei.md, no publication, \
             unchanged project settings; got exit {:?}, published={published}, \
             settings_unchanged={unchanged}\nstdout:\n{}\nstderr:\n{}",
            result.status.code(),
            result.stdout,
            result.stderr
        );
        if !existing_settings {
            assert!(
                !project.join(".agent-grounds").exists(),
                "no settings directories after refusal"
            );
        }
        eprintln!("PASS malformed sibling --output {output}: parse refusal, no publication or settings change");
    }
}

/// The project parse refuses all spellings before publication; even bundled
/// settings must leave no new project settings file or directories behind.
/// §FS-rhei-templates.6.2
#[test]
fn every_output_spelling_refuses_malformed_sibling_without_settings() {
    refuses_malformed_sibling(false);
}

/// A refusal also preserves an existing project settings file byte-for-byte.
/// §FS-rhei-templates.6.2
#[test]
fn every_output_spelling_refuses_malformed_sibling_preserving_settings() {
    refuses_malformed_sibling(true);
}

/// Existing values win, missing bundled keys are added at the project root,
/// and no workspace copy survives. §FS-rhei-templates.6.2
#[test]
fn every_output_spelling_reconciles_settings_at_project_root() {
    let project = unique_temp_dir("instantiate-spelling-settings");
    let template = fixture(&project);
    for output in outputs(&project) {
        seed_settings(&project);
        let result = instantiate(&project, &template, &output);
        assert_success(&result);
        let settings: serde_json::Value =
            serde_json::from_slice(&fs::read(project.join(SETTINGS)).unwrap()).unwrap();
        let workspace_copy = project.join("new").join(SETTINGS).exists();
        assert!(
            settings["defaults"]["agent_timeout"] == "5m"
                && settings["agents"]["project-only"]["command"] == serde_json::json!(["project-tool"])
                && settings["agents"]["codex"]["command"] == serde_json::json!(["codex", "exec"])
                && settings["agents"]["codex"]["modes"]["xhigh"] == serde_json::json!(["--effort", "xhigh"])
                && !workspace_copy,
            "--output {output}: expected project values preserved, bundled keys added at project \
             root, no workspace settings copy; workspace_copy={workspace_copy}, project settings:\n{settings}"
        );
        assert!(!project.join("new/settings.json").exists(), "no root settings copy");
        assert!(!project.join("new/.rhei/settings.json").exists(), "no deprecated settings copy");
        assert!(
            result.stdout.contains("Merged the template's agent settings"),
            "{}",
            result.stdout
        );
        assert!(
            result.stdout.contains("added:")
                && result.stdout.contains("kept your existing values for:"),
            "{}",
            result.stdout
        );
        eprintln!("PASS settings --output {output}: project values kept, bundled keys added, no workspace copy");
        fs::remove_dir_all(project.join("new")).unwrap();
    }
}
