//! Existing-destination previews and workspace sibling remedies retain strict
//! prospective project validation. §FS-rhei-templates.6.1.2

use std::fs;
use std::path::Path;

use super::instantiate_project_validation_support::*;
use super::*;

/// Preview scratch in place of an existing workspace, preserving directories,
/// files and settings, while real writes still refuse that destination.
/// §FS-rhei-templates.6.1.2
#[test]
fn existing_workspace_preview_uses_scratch_and_preserves_project() {
    let scenario = Scenario::new();
    assert_success(&scenario.output_run(&[]));
    assert!(scenario.output().join("index.rhei.md").is_file());
    scenario.add_incoming_settings();
    write_fixture_file(
        &scenario.template,
        "tasks/001-only.md",
        "### Task 2: Prospective task\n**State:** todo\n",
    );
    let before = snapshot(&scenario.project);
    for flags in [&["--dry-run"][..], &["--dry-run", "--keep-on-error"][..]] {
        let preview = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before, "preview changes no existing bytes");
        assert_success(&preview);
        assert!(preview.stdout.contains("new.2"), "preview must use scratch:\n{}", preview.stdout);
    }
    let real = scenario.output_run(&[]);
    assert_eq!(real.status.code(), Some(1), "existing output must refuse a real write");
    assert_eq!(snapshot(&scenario.project), before, "real refusal preserves existing output");

    scenario.add_broken_sibling();
    let before = snapshot(&scenario.project);
    let refused = scenario.output_run(&["--dry-run", "--keep-on-error"]);
    assert_eq!(snapshot(&scenario.project), before, "refusal changes no existing bytes");
    assert_sibling_remedy(&scenario, &refused);
}

/// Replacing one destination must not hide a different entry with the same id.
/// §FS-rhei-templates.6.1.2
#[test]
fn existing_workspace_preview_keeps_other_entry_id_collisions() {
    let scenario = Scenario::new();
    assert_success(&scenario.output_run(&[]));
    write_fixture_file(
        &scenario.project,
        "new.rhei.md",
        "# Rhei: Other new\n\n## Tasks\n\n### Task 1: Other task\n**State:** todo\n",
    );
    let before = snapshot(&scenario.project);
    let refused = scenario.output_run(&["--dry-run", "--keep-on-error"]);
    assert_eq!(snapshot(&scenario.project), before);
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stderr.contains("duplicate rhei id 'new'"), "{}", refused.stderr);
    assert!(refused.stderr.contains("new.rhei.md"), "{}", refused.stderr);
}

/// Workspace fragment errors keep their source context but suggest validating
/// the owning workspace, using a shell-safe path from the invocation directory.
/// §FS-rhei-templates.6.1.2
#[test]
fn workspace_sibling_remedy_validates_owning_workspace() {
    let mut scenario = Scenario::new();
    let project = scenario.project.with_file_name("project's home");
    fs::rename(&scenario.project, &project).unwrap();
    scenario.project = project;
    scenario.clean_control();
    let workspace = scenario.project.join("old");
    fs::create_dir_all(workspace.join("tasks")).unwrap();
    write_fixture_file(&workspace, "index.rhei.md", "# Rhei: Old\n");
    write_fixture_file(
        &workspace,
        "tasks/001-only.md",
        "### Task 1: Only\n**State:** todo\n## Appendix\n",
    );
    let before = snapshot(&scenario.project);
    for flags in [&["--dry-run", "--keep-on-error"][..], &[][..]] {
        let refused = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before);
        assert_eq!(refused.status.code(), Some(1));
        assert!(refused.stderr.contains("existing sibling"), "{}", refused.stderr);
        let fragment = Path::new("tasks").join("001-only.md");
        assert!(refused.stderr.contains(&*fragment.to_string_lossy()), "{}", refused.stderr);
        assert!(refused.stderr.contains("## Appendix"), "{}", refused.stderr);
        let targets = [workspace.clone(), Path::new("..").join("project's home").join("old")];
        let target = targets
            .iter()
            .find(|target| {
                refused
                    .stderr
                    .contains(&format!("rhei validate {}", shell_quote(&target.to_string_lossy())))
            })
            .expect("remedy names the owning workspace with shell-safe quoting");
        let home = unique_temp_dir("workspace-remedy-home");
        let output = rhei_command(&home)
            .current_dir(&scenario.cwd)
            .arg("validate")
            .arg(target)
            .output()
            .expect("run the remedy's target from the invocation directory");
        let validation = CliRun::from(&output);
        assert_eq!(validation.status.code(), Some(1));
        assert!(
            validation.stderr.contains("Tasks section must be the final"),
            "{}",
            validation.stderr
        );
        assert!(validation.stderr.contains("## Appendix"), "{}", validation.stderr);
        assert!(!validation.stderr.contains("Missing '# Rhei:"), "{}", validation.stderr);
        assert!(
            !validation.stderr.contains("Metadata field appears outside a task"),
            "{}",
            validation.stderr
        );
        assert_eq!(snapshot(&scenario.project), before, "the suggested remedy changes no bytes");
    }
}
