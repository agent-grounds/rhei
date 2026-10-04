//! The retired `**States:**` line: refused, with the file, the line and the
//! remedy, by every command that reads the document — and nowhere it never
//! meant anything.
//!
//! The refusal cases are `#[ignore]`d until the refusal lands: they are the
//! contract the change is written against, and the commit that makes them pass
//! removes the attribute.

// §FS-rhei-plan-language.2.2

use std::path::Path;

use super::new_tests::{flattened_output, new_run};
use super::state_machine_resolution_support::*;
use super::*;

/// The refusal: a non-zero exit, and one message naming the file, the line,
/// and the single remedy. A refusal that only says "unknown field" sends the
/// reader looking for a value to change. §FS-rhei-plan-language.2.2
fn assert_refused(result: &CliRun, file: &str, line: usize) {
    let said = flattened_output(result);
    assert!(!result.status.success(), "the `**States:**` line must be refused; got:\n{said}");
    assert!(said.contains(file), "the refusal names the file {file:?}; got:\n{said}");
    assert!(
        said.contains(&format!(":{line}")) || said.contains(&format!("line {line}")),
        "the refusal names line {line}; got:\n{said}"
    );
    assert!(said.contains("delete this line"), "the remedy is to delete the line; got:\n{said}");
}

/// A Panta project whose manifest carries the line on line 2, holding one
/// member that carries none — the shape of a work root laid before the removal.
fn project_with_a_manifest_line(dir: &Path) -> std::path::PathBuf {
    let root = project(dir, Some("rhei"));
    member(&root, "audit", None, "pending");
    root
}

#[test]
#[ignore = "red until #350 refuses the `**States:**` line"]
fn a_single_file_plan_carrying_the_line_is_refused() {
    let dir = unique_temp_dir("states-line-single-file");
    let home = dir.join(".home");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Lone\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n",
    );

    let result = rhei_in(&dir, &home, &["validate", "plan.rhei.md"]);
    assert_refused(&result, "plan.rhei.md", 2);
}

/// Anywhere outside a fence, not only right below the header: a line further
/// down was never legal, but its refusal must carry the same remedy.
/// §FS-rhei-plan-language.2.2
#[test]
#[ignore = "red until #350 refuses the `**States:**` line"]
fn the_line_is_refused_below_the_header_too() {
    let dir = unique_temp_dir("states-line-content-section");
    let home = dir.join(".home");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Lone\n\n## Notes\n\nSome context.\n**States:** rhei\n\n## Tasks\n\n\
         ### Task 1: A\n**State:** pending\n",
    );

    let result = rhei_in(&dir, &home, &["validate", "plan.rhei.md"]);
    assert_refused(&result, "plan.rhei.md", 6);
}

#[test]
#[ignore = "red until #350 refuses the `**States:**` line"]
fn a_workspace_index_carrying_the_line_is_refused() {
    let dir = unique_temp_dir("states-line-workspace-index");
    let home = dir.join(".home");
    let workspace = dir.join("billing");
    std::fs::create_dir_all(workspace.join("tasks")).expect("create the workspace");
    write_fixture_file(&workspace, "index.rhei.md", "# Rhei: Billing\n**States:** rhei\n");
    write_fixture_file(&workspace.join("tasks"), "01.md", "### Task 1: A\n**State:** pending\n");

    let result = rhei_in(&dir, &home, &["validate", "billing"]);
    assert_refused(&result, "index.rhei.md", 2);
}

#[test]
#[ignore = "red until #350 refuses the `**States:**` line"]
fn a_project_manifest_carrying_the_line_is_refused() {
    let dir = unique_temp_dir("states-line-manifest");
    let home = dir.join(".home");
    let root = project_with_a_manifest_line(&dir);
    let project_arg = root.display().to_string();

    let result = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_refused(&result, "index.panta.md", 2);
}

/// A parse error, so the commands that only read refuse too: a tree that would
/// run under another machine once the line is gone must not look healthy in
/// `list`, `states` or a rendered export meanwhile. §FS-rhei-plan-language.2.2
#[test]
#[ignore = "red until #350 refuses the `**States:**` line"]
fn read_only_commands_refuse_the_line_too() {
    let dir = unique_temp_dir("states-line-read-only");
    let home = dir.join(".home");
    let root = project_with_a_manifest_line(&dir);
    let project_arg = root.display().to_string();

    for args in [
        vec!["list", project_arg.as_str()],
        vec!["states", project_arg.as_str()],
        vec!["render", project_arg.as_str(), "--format", "json"],
    ] {
        let result = rhei_in(&dir, &home, &args);
        assert_refused(&result, "index.panta.md", 2);
    }
}

/// Inside a fence every line is content, so an index may still quote the old
/// syntax. Passes before and after the change: it pins that the refusal does
/// not reach past the fence rule. §FS-rhei-plan-language.2.1
#[test]
fn a_fenced_line_in_an_index_is_content() {
    let dir = unique_temp_dir("states-line-fenced");
    let home = dir.join(".home");
    let root = project(&dir, None);
    let rhei = member(&root, "audit", None, "pending");
    write_fixture_file(
        &rhei,
        "index.rhei.md",
        "# Rhei: audit\n\n## History\n\nPlans used to open:\n\n```markdown\n\
         # Rhei: Old\n**States:** rhei\n```\n",
    );
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));
}

/// The grammar never gave the line a meaning below the header, so in a task
/// body it is ordinary text and no machine changes because of it. Passes before
/// and after the change. §FS-rhei-plan-language.2.2
#[test]
fn a_line_in_a_task_file_is_text() {
    let dir = unique_temp_dir("states-line-task-file");
    let home = dir.join(".home");
    let root = project(&dir, None);
    let rhei = member(&root, "audit", None, "pending");
    write_fixture_file(
        &rhei.join("tasks"),
        "02.md",
        "### Task 2: Review the syntax\n**State:** pending\n\nCheck that no plan still says\n\
         **States:** rhei\nbelow its header.\n",
    );
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));
}

/// Nothing in a plan names a machine, so `rhei new` has no flag that writes one:
/// clap's own unknown-argument error, exit 2. §FS-rhei-new.1.2
#[test]
#[ignore = "red until #350 deletes `rhei new --states`"]
fn rhei_new_has_no_states_flag() {
    let dir = unique_temp_dir("states-line-new-flag");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Test\n");
    write_fixture_file(&dir, "states.yaml", &machine("y", "drafting", "filed"));

    let result = new_run(&["new", "x", "--states", "y"], &dir);
    assert_eq!(result.status.code(), Some(2), "clap's usage exit; stderr:\n{}", result.stderr);
    assert!(
        result.stderr.contains("unexpected argument '--states'"),
        "clap names the flag it does not know; stderr:\n{}",
        result.stderr
    );
    assert!(!dir.join("x.rhei.md").exists(), "a refused create writes nothing");
}

/// `rheis[].states` is the machine the rhei resolves, and the entry carries no
/// `states_declared`: there is no declaration left to report. The member with
/// its own file is the case the old field got wrong — it reported the project
/// default it inherited by declaration, not the file that governs it.
/// §FS-rhei-render.3.1
#[test]
#[ignore = "red until #350 reports the resolved machine in render JSON"]
fn render_json_reports_each_rheis_resolved_machine() {
    let dir = unique_temp_dir("states-line-render-json");
    let home = dir.join(".home");
    let root = project(&dir, None);
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    member(&root, "auth", None, "surveying");
    let billing = member(&root, "billing", None, "drafting");
    member_machine(&billing, &machine("billing-machine", "drafting", "filed"));
    let project_arg = root.display().to_string();

    let result = rhei_in(&dir, &home, &["render", &project_arg, "--format", "json"]);
    assert_success(&result);
    let doc: serde_json::Value = serde_json::from_str(&result.stdout).expect("render emits JSON");
    assert_eq!(doc["states"], "proj-machine", "the top level is the project default");
    let rheis = doc["rheis"].as_array().expect("a project render carries `rheis`");
    let entry = |id: &str| {
        rheis.iter().find(|e| e["id"] == id).unwrap_or_else(|| panic!("no entry for {id}"))
    };
    assert_eq!(entry("auth")["states"], "proj-machine");
    assert_eq!(entry("billing")["states"], "billing-machine");
    for e in rheis {
        assert!(e.get("states_declared").is_none(), "`states_declared` is gone; got {e}");
    }
}
