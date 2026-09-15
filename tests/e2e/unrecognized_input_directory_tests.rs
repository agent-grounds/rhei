//! An existing directory with neither recognized manifest is a plan-shape
//! error, not a generic filesystem failure. §FS-rhei-errors.3.2

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

struct UnrecognizedDirectoryFixture {
    _root: TestDir,
    home: PathBuf,
    container: PathBuf,
    workspace: PathBuf,
    single_file: PathBuf,
}

fn fixture(prefix: &str) -> UnrecognizedDirectoryFixture {
    let root = unique_temp_dir(prefix);
    let container = root.join("panta");
    let workspace = container.join("workspace");
    fs::create_dir_all(workspace.join("tasks")).expect("create workspace fixture");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Workspace\n")
        .expect("write workspace manifest");
    fs::write(
        workspace.join("tasks/01-placeholder.md"),
        "### Task 1: Placeholder\n**State:** completed\n",
    )
    .expect("write inert workspace task");
    let single_file = write_fixture_file(
        &root,
        "single.rhei.md",
        "# Rhei: Single\n\n## Tasks\n\n### Task 1: Placeholder\n**State:** completed\n",
    );
    let home = root.join(".home");
    UnrecognizedDirectoryFixture { _root: root, home, container, workspace, single_file }
}

fn run_dry(home: &Path, input: &Path, extra: &[&str]) -> CliRun {
    let mut command = rhei_command(home);
    command.arg("run").arg(input).args(extra).args(["--dry-run", "--no-tui", "--no-callbacks"]);
    CliRun::from(&command.output().expect("rhei run should execute"))
}

fn run_reported_headless(fixture: &UnrecognizedDirectoryFixture) -> CliRun {
    let output = rhei_command(&fixture.home)
        .arg("run")
        .arg("--headless")
        .arg(&fixture.container)
        .args(["--rhei", "workspace"])
        .output()
        .expect("rhei run --headless should execute");
    CliRun::from(&output)
}

fn assert_unrecognized_directory(
    result: &CliRun,
    container: &Path,
    workspace: &Path,
    invocation: &str,
) {
    assert!(
        !result.status.success(),
        "{invocation} must reject the unrecognized directory\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let diagnostic = &result.stderr;
    assert!(
        diagnostic.contains(&container.display().to_string())
            && diagnostic.contains("not a recognized")
            && diagnostic.contains("Panta Project")
            && diagnostic.contains("Directory Workspace"),
        "{invocation} must classify '{}' as an unrecognized input directory; command exit: {:?}; got:\n{}",
        container.display(),
        result.status.code(),
        diagnostic
    );
    assert!(
        diagnostic.contains("index.panta.md") && diagnostic.contains("index.rhei.md"),
        "{invocation} must name the alternative project and workspace manifests; got:\n{}",
        diagnostic
    );
    assert!(
        diagnostic.contains("rhei run") && diagnostic.contains(&workspace.display().to_string()),
        "{invocation} must correct the wrong-level input with the directly addressable workspace '{}'; got:\n{}",
        workspace.display(),
        diagnostic
    );
    for misleading in ["permission", "writable", "free space"] {
        assert!(
            !diagnostic.to_ascii_lowercase().contains(misleading),
            "{invocation} must not give {misleading:?} advice for a missing manifest; got:\n{}",
            diagnostic
        );
    }
}

/// The ordinary run path rejects the reported container while preserving all
/// three recognized input shapes. Every control is dry-run and cannot spawn an
/// agent or begin orchestration. §FS-rhei-errors.3.2
#[test]
fn run_classifies_an_unrecognized_directory_and_points_to_its_workspace() {
    let fixture = fixture("unrecognized-directory-run");

    let workspace_control = run_dry(&fixture.home, &fixture.workspace, &[]);
    let single_file_control = run_dry(&fixture.home, &fixture.single_file, &[]);
    let rejected = run_dry(&fixture.home, &fixture.container, &["--rhei", "workspace"]);

    fs::write(fixture.container.join("index.panta.md"), "# Panta: Recognized\n")
        .expect("write project manifest for the final control");
    let project_control = run_dry(&fixture.home, &fixture.container, &["--rhei", "workspace"]);

    for (name, result) in [
        ("directly addressed Directory Workspace", &workspace_control),
        ("Single-File Plan", &single_file_control),
        ("Panta Project", &project_control),
    ] {
        assert!(
            result.status.success(),
            "recognized {name} control must retain its behavior\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }

    assert_unrecognized_directory(
        &rejected,
        &fixture.container,
        &fixture.workspace,
        "rhei run DIRECTORY --rhei workspace --dry-run",
    );
}

/// The invocation from agent-grounds/rhei#264 must report the startup failure
/// synchronously instead of leaving the useful diagnosis in a detached log.
/// §FS-rhei-errors.3.2 §FS-rhei-run-headless.1.1
#[cfg(unix)]
#[test]
fn headless_run_propagates_the_unrecognized_directory_diagnosis() {
    let fixture = fixture("unrecognized-directory-headless");
    let rejected = run_reported_headless(&fixture);

    assert_unrecognized_directory(
        &rejected,
        &fixture.container,
        &fixture.workspace,
        "rhei run --headless DIRECTORY --rhei workspace",
    );
}
