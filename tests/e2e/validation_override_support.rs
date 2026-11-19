//! Fixtures and assertions for project override provenance. §FS-rhei-validate.6.1

use std::fs;
use std::path::{Path, PathBuf};

use super::new_tests::flattened_output;
use super::*;

pub(super) const CONTEXT: &str = "State-machine override '";
pub(super) const DISPLACED: &str = "Automatic sources displaced:";

pub(super) fn machine(name: &str, states: &[&str]) -> String {
    let mut yaml = format!("name: {name}\nversion: 1\nstates:\n");
    for (i, state) in states.iter().enumerate() {
        yaml.push_str(&format!("  {state}:\n    description: {state}\n"));
        if i == 0 {
            yaml.push_str("    initial: true\n");
        }
        if i == states.len() - 1 {
            yaml.push_str("    final: true\n");
        }
    }
    yaml.push_str("transitions:\n");
    for pair in states.windows(2) {
        yaml.push_str(&format!("  - {{ from: {}, to: {} }}\n", pair[0], pair[1]));
    }
    yaml
}

pub(super) fn member(project: &Path, id: &str, tasks: &[(&str, &str)]) -> PathBuf {
    let root = project.join(id);
    fs::create_dir_all(root.join("tasks")).expect("create member");
    write_fixture_file(&root, "index.rhei.md", &format!("# Rhei: {id}\n"));
    for (task, state) in tasks {
        write_fixture_file(
            &root.join("tasks"),
            &format!("{task}.md"),
            &format!("### Task {task}: {task}\n**State:** {state}\n"),
        );
    }
    root
}

/// Self-contained port of #753's two-rhei reproduction. Version 1 legacy
/// syntax remains valid, so no syntax adaptation or external runtime fixture
/// is needed. Neither machine accepts the other rhei's working states.
pub(super) fn project(dir: &Path) -> PathBuf {
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    write_fixture_file(&project, "index.panta.md", "# Panta: CSA streams\n");
    let selective = member(&project, "csa-selective", &[("slice", "frame")]);
    write_fixture_file(
        &selective,
        "states.yaml",
        &machine("csa-stream", &["frame", "implement", "completed"]),
    );
    let design = member(
        &project,
        "csa-loop-design",
        &[("approve", "approve-design"), ("publish", "publish")],
    );
    write_fixture_file(
        &design,
        "states.yaml",
        &machine("csa-design", &["approve-design", "publish", "completed"]),
    );
    project
}

pub(super) fn union_override(dir: &Path) -> PathBuf {
    write_fixture_file(
        dir,
        "edited.yaml",
        &machine("edited", &["frame", "implement", "approve-design", "publish", "completed"]),
    )
}

pub(super) fn command(
    cwd: &Path,
    home: &Path,
    target: &str,
    override_file: Option<&Path>,
) -> std::process::Command {
    let mut cmd = rhei_command(home);
    cmd.current_dir(cwd).env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.args(["validate", target]);
    if let Some(path) = override_file {
        cmd.arg("--state-machine").arg(path);
    }
    cmd
}

pub(super) fn validate(
    cwd: &Path,
    home: &Path,
    target: &str,
    override_file: Option<&Path>,
) -> CliRun {
    CliRun::from(&command(cwd, home, target, override_file).output().expect("run validate"))
}

pub(super) fn context(result: &CliRun, override_file: &Path) -> String {
    let said = flattened_output(result);
    context_text(&said, override_file)
}

pub(super) fn assert_header(said: &str, override_file: &Path) {
    let header =
        format!("{CONTEXT}{}' applies to every rhei in the project.", override_file.display());
    assert!(
        said.contains(&header),
        "missing whole-project override explanation {header:?}; got:\n{said}"
    );
    assert_eq!(said.matches(CONTEXT).count(), 1, "duplicate context in one pass:\n{said}");
}

pub(super) fn context_text(said: &str, override_file: &Path) -> String {
    assert_header(said, override_file);
    assert_eq!(said.matches(DISPLACED).count(), 1, "missing/duplicate displaced list:\n{said}");
    said.split_once(DISPLACED).expect("displaced sources").1.to_owned()
}

pub(super) fn assert_source(displaced: &str, cwd: &Path, path: &Path, owners: &str) -> usize {
    assert_owned_source(displaced, cwd, path, &format!("rhei: {owners}"))
}

/// Check source identity, allowing the normal absolute/relative path spelling
/// of each equivalent target. The ownership entry remains an exact contract.
pub(super) fn assert_owned_source(displaced: &str, cwd: &Path, path: &Path, label: &str) -> usize {
    let suffix = format!("' ({label})");
    assert_eq!(
        displaced.matches(&suffix).count(),
        1,
        "missing/duplicate owner {label}:\n{displaced}"
    );
    let end = displaced.find(&suffix).expect("owner suffix");
    let start = displaced[..end].rfind('\'').expect("quoted source path") + 1;
    assert_same_path(&cwd.join(&displaced[start..end]), &cwd.join(path));
    start
}
