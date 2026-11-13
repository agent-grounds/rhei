// A link refused because its target does not exist names the base it was resolved
// against and the path it tried, so a link written relative to a nested task file
// is not refused with a bare "'x' does not exist" while x sits beside it (#668).

// §FS-rhei-plan-language.3.6

use std::fs;
use std::path::Path;

use super::new_tests::flattened_output;
use super::*;

/// A Directory Workspace whose `tasks/07.md` links `link`, with `notes.md` beside
/// the task file under `tasks/` and nothing at the workspace root.
fn nested_link_workspace(prefix: &str, link: &str) -> (TestDir, std::path::PathBuf, CliRun) {
    let task = format!("### Task t07: Load\n**State:** pending\n\nRead [notes]({link}) first.\n");
    let (dir, ws, machine) = create_workspace(
        prefix,
        "# Rhei: Base\n\n## Overview\n\nA workspace.\n",
        &[("07.md", &task)],
    );
    write_fixture_file(&ws.join("tasks"), "notes.md", "# notes\n");
    let run = validate(&dir, &machine, &ws);
    (dir, ws, run)
}

fn validate(dir: &Path, machine: &Path, target: &Path) -> CliRun {
    let output = rhei_command(dir.join(".home"))
        .arg("--state-machine")
        .arg(machine)
        .arg("validate")
        .arg(target)
        .output()
        .expect("rhei validate should run");
    CliRun::from(&output)
}

/// The refusal sentence for `link` alone, from where it names the link up to the
/// closing recommendation, with all whitespace removed so a path the renderer
/// wrapped still matches. The diagnostic's `in <target>` header names the plan
/// path too, so the header must not count as the refusal naming its base.
#[track_caller]
fn refusal_for(run: &CliRun, link: &str) -> String {
    let said = flattened_output(run);
    let marker = format!("contains a link [notes]({link})");
    let start = said
        .find(&marker)
        .unwrap_or_else(|| panic!("the refusal should name the link {link}; got:\n{said}"));
    let rest = &said[start..];
    let end = rest.find("I recommend").unwrap_or(rest.len());
    rest[..end].split_whitespace().collect()
}

/// Whether `sentence` (whitespace already removed) names `dir`, as passed or
/// canonicalized.
fn names_dir(sentence: &str, dir: &Path) -> bool {
    let squeeze = |p: &Path| p.display().to_string().split_whitespace().collect::<String>();
    let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    sentence.contains(&squeeze(dir)) || sentence.contains(&squeeze(&canonical))
}

/// `notes.md` from `tasks/07.md` is right there beside the task file, but task
/// links resolve against the workspace root, where it is not: the refusal must
/// say the link resolves against the workspace root and name that root (or the
/// path it tried under it), not only `'notes.md' does not exist`.
/// §FS-rhei-plan-language.3.6
#[test]
#[ignore = "red until #668: missing-link refusal names no base or resolved path; implement removes this"]
fn a_missing_task_link_in_a_workspace_names_the_workspace_root_it_resolved_against() {
    let (_dir, ws, run) = nested_link_workspace("link-base-nested", "notes.md");
    let said = flattened_output(&run);
    assert!(!run.status.success(), "the link should still be refused; got:\n{said}");
    let refusal = refusal_for(&run, "notes.md");
    assert!(
        names_dir(&refusal, &ws),
        "the refusal should name the workspace root {} it resolved against (or the path it \
         tried under it); got:\n{said}",
        ws.display()
    );
    assert!(
        refusal.contains("workspaceroot"),
        "the refusal should say links resolve against the workspace root; got:\n{said}"
    );
}

/// The control: the same file linked relative to the workspace root validates,
/// so the rule itself is unchanged. §FS-rhei-plan-language.3.6
#[test]
fn a_root_relative_task_link_in_a_workspace_validates() {
    let (_dir, _ws, run) = nested_link_workspace("link-base-control", "tasks/notes.md");
    assert_success(&run);
}

/// The report's own link leaves the root; it stays refused, naming the root.
/// §FS-rhei-plan-language.3.6
#[test]
fn the_reports_escaping_link_stays_refused_naming_the_root() {
    let (_dir, ws, run) = nested_link_workspace("link-base-escape", "../design/local-profile.json");
    let said = flattened_output(&run);
    assert!(!run.status.success(), "the escaping link should be refused; got:\n{said}");
    let refusal = refusal_for(&run, "../design/local-profile.json");
    assert!(names_dir(&refusal, &ws), "the refusal should name the root; got:\n{said}");
}

/// A Single-File Plan's missing link names the plan file's directory it was
/// resolved against. §FS-rhei-plan-language.3.6
#[test]
#[ignore = "red until #668: missing-link refusal names no base or resolved path; implement removes this"]
fn a_missing_link_in_a_single_file_plan_names_the_plan_directory() {
    let dir = unique_temp_dir("link-base-single-file");
    let plans = dir.join("plans");
    fs::create_dir_all(&plans).expect("create the plan directory");
    let plan = write_fixture_file(
        &plans,
        "plan.rhei.md",
        "# Rhei: Single\n\n## Tasks\n\n### Task 1: Load\n**State:** pending\n\n\
         Read [notes](notes.md).\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let run = validate(&dir, &machine, &plan);
    let said = flattened_output(&run);
    assert!(!run.status.success(), "the missing link should be refused; got:\n{said}");
    let refusal = refusal_for(&run, "notes.md");
    assert!(
        names_dir(&refusal, &plans),
        "the refusal should name the plan's directory {} it resolved against; got:\n{said}",
        plans.display()
    );
}
