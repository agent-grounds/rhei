//! An existing destination refuses its preview like the real write, and workspace
//! and basin sibling remedies retain strict prospective project validation.
//! §FS-rhei-templates.6.1.2

use std::fs;
use std::path::Path;

use super::instantiate_project_validation_support::*;
use super::*;

/// An existing workspace refuses the preview exactly as it refuses the real
/// write, before any project validation, preserving directories, files and
/// settings. §FS-rhei-templates.6.1.2
#[test]
fn existing_workspace_refuses_preview_like_real_write_and_preserves_project() {
    let scenario = Scenario::new();
    assert_success(&scenario.output_run(&[]));
    assert!(scenario.output().join("index.rhei.md").is_file());
    scenario.add_incoming_settings();
    scenario.add_broken_sibling();
    let before = snapshot(&scenario.project);
    let real = scenario.output_run(&[]);
    assert_eq!(real.status.code(), Some(1), "existing output must refuse a real write");
    assert_eq!(snapshot(&scenario.project), before, "real refusal preserves existing output");
    for flags in [&["--dry-run"][..], &["--dry-run", "--keep-on-error"][..]] {
        let preview = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before, "preview changes no existing bytes");
        assert_eq!(preview.status.code(), Some(1), "preview refuses:\n{}", preview.stdout);
        assert_eq!(preview.stderr, real.stderr, "preview reports the real write's error");
        assert!(!preview.stdout.contains("Instantiate this template with:"), "{}", preview.stdout);
    }
}

/// A different entry already holding the new member's id refuses the preview.
/// §FS-rhei-templates.6.1.2
#[test]
fn preview_refuses_other_entry_with_the_same_id() {
    let scenario = Scenario::new();
    scenario.clean_control();
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
        assert_remedy_reproduces(&scenario, &refused, &targets);
        assert_eq!(snapshot(&scenario.project), before, "the suggested remedy changes no bytes");
    }
}

/// A `basin/` file is a task fragment the project owns, so the remedy that
/// reproduces its parse error validates the project, not the bare fragment.
/// §FS-rhei-templates.6.1.2
#[test]
fn basin_sibling_remedy_validates_owning_project() {
    let scenario = Scenario::new();
    scenario.clean_control();
    fs::create_dir(scenario.project.join("basin")).unwrap();
    write_fixture_file(
        &scenario.project,
        "basin/001-only.md",
        "### Task 1: Only\n**State:** todo\n## Appendix\n",
    );
    let before = snapshot(&scenario.project);
    for flags in [&["--dry-run"][..], &["--dry-run", "--keep-on-error"][..], &[][..]] {
        let refused = scenario.output_run(flags);
        assert_eq!(snapshot(&scenario.project), before);
        assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
        assert!(refused.stderr.contains("existing sibling"), "{}", refused.stderr);
        let fragment = Path::new("basin").join("001-only.md");
        assert!(refused.stderr.contains(&*fragment.to_string_lossy()), "{}", refused.stderr);
        assert!(refused.stderr.contains("Tasks section must be the final"), "{}", refused.stderr);
        assert!(refused.stderr.contains("## Appendix"), "{}", refused.stderr);
        let targets = [scenario.project.clone(), Path::new("..").join("project")];
        assert_remedy_reproduces(&scenario, &refused, &targets);
        assert_eq!(snapshot(&scenario.project), before, "the suggested remedy changes no bytes");
    }
}
