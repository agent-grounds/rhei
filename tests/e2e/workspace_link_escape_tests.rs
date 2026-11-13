// A Directory Workspace link whose joined-and-normalized target leaves the
// workspace root is invalid whether or not a file sits where it lands; a link that
// leaves and comes back, and a Single-File Plan's `../`, stay valid (#684).

// §FS-rhei-plan-language.3.6

use std::fs;
use std::path::Path;

use super::new_tests::flattened_output;
use super::*;

const ESCAPE: &str = "../design/local-profile.json";

/// The issue's workspace: `ws/` beside `design/`, the escaping link in a task
/// file or in the index's Overview, and the outside file there or not.
fn escape_workspace(prefix: &str, in_task: bool, outside_file: bool) -> (TestDir, CliRun) {
    let link = format!("Read [profile]({ESCAPE}) before starting.\n");
    let (index, task) = if in_task {
        (
            "# Rhei: Escape\n\n## Overview\n\nA workspace.\n".to_string(),
            format!("### Task t07: Load\n**State:** pending\n\n{link}"),
        )
    } else {
        (
            format!("# Rhei: Escape\n\n## Overview\n\n{link}"),
            "### Task t07: Load\n**State:** pending\n".to_string(),
        )
    };
    let (dir, ws, machine) = create_workspace(prefix, &index, &[("07.md", &task)]);
    if outside_file {
        fs::create_dir_all(dir.join("design")).expect("create the outside directory");
        write_fixture_file(&dir.join("design"), "local-profile.json", "{}\n");
    }
    let run = validate(&dir, &machine, &ws);
    (dir, run)
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

#[track_caller]
fn assert_refused_as_escaping(run: &CliRun, case: &str) {
    let said = flattened_output(run);
    assert!(!run.status.success(), "{case}: the escaping link should be refused; got:\n{said}");
    assert!(said.contains(ESCAPE), "{case}: the refusal should name the link; got:\n{said}");
    assert!(
        said.contains("leaves the workspace root"),
        "{case}: the refusal should say the link leaves the workspace root; got:\n{said}"
    );
}

/// The task-file link of the issue: refused as leaving the workspace with the
/// outside file present, and with the same words when it is absent.
/// §FS-rhei-plan-language.3.6
#[test]
fn an_escaping_task_link_is_refused_whether_or_not_its_target_exists() {
    let (_present, run) = escape_workspace("link-escape-task-present", true, true);
    assert_refused_as_escaping(&run, "task link, outside file present");
    let (_absent, run) = escape_workspace("link-escape-task-absent", true, false);
    assert_refused_as_escaping(&run, "task link, outside file absent");
}

/// The same link in `index.rhei.md`'s Overview section. §FS-rhei-plan-language.3.6
#[test]
fn an_escaping_section_link_is_refused_whether_or_not_its_target_exists() {
    let (_present, run) = escape_workspace("link-escape-section-present", false, true);
    assert_refused_as_escaping(&run, "section link, outside file present");
    let (_absent, run) = escape_workspace("link-escape-section-absent", false, false);
    assert_refused_as_escaping(&run, "section link, outside file absent");
}

/// With nothing outside the workspace the escaping links are refused today, and
/// must stay refused. §FS-rhei-plan-language.3.6
#[test]
fn an_escaping_link_with_nothing_where_it_lands_stays_refused() {
    for (prefix, in_task) in
        [("link-escape-guard-task", true), ("link-escape-guard-section", false)]
    {
        let (_dir, run) = escape_workspace(prefix, in_task, false);
        let said = flattened_output(&run);
        assert!(!run.status.success(), "{prefix}: should be refused; got:\n{said}");
        assert!(said.contains(ESCAPE), "{prefix}: the refusal should name the link; got:\n{said}");
    }
}

/// `../workspace/docs/x.md` leaves the root and comes back: normalized it is
/// inside, so it is checked like any in-root link and accepted. A fix that refuses
/// every `..` breaks this. §FS-rhei-plan-language.3.6
#[test]
fn a_link_that_leaves_and_returns_to_the_workspace_is_accepted() {
    let task = "### Task 1: Load\n**State:** pending\n\nRead [x](../workspace/docs/x.md).\n";
    let (dir, ws, machine) =
        create_workspace("link-escape-return", "# Rhei: Return\n", &[("01.md", task)]);
    fs::create_dir_all(ws.join("docs")).expect("create docs inside the workspace");
    write_fixture_file(&ws.join("docs"), "x.md", "# x\n");

    let run = validate(&dir, &machine, &ws);
    assert_success(&run);
}

/// A Single-File Plan's links resolve against the plan file's directory and may
/// reach above it: containment is a workspace rule. §FS-rhei-plan-language.3.6
#[test]
fn a_single_file_plan_may_link_above_its_directory() {
    let dir = unique_temp_dir("link-escape-single-file");
    fs::create_dir_all(dir.join("plans")).expect("create the plan directory");
    write_fixture_file(&dir, "shared.md", "# shared\n");
    let plan = write_fixture_file(
        &dir.join("plans"),
        "plan.rhei.md",
        "# Rhei: Single\n\n## Tasks\n\n### Task 1: Load\n**State:** pending\n\n\
         Read [shared](../shared.md).\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    let run = validate(&dir, &machine, &plan);
    assert_success(&run);
}

/// A Panta project of one Directory Workspace rhei `ws/` and one Single-File rhei
/// `single.rhei.md`, so links run through the per-task bases (§AR-rhei-panta.5).
/// `ws_escapes` puts the escaping link in the workspace rhei's task.
fn panta_project(prefix: &str, ws_escapes: bool) -> (TestDir, CliRun) {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("proj");
    let ws_tasks = project.join("ws").join("tasks");
    fs::create_dir_all(&ws_tasks).expect("create the workspace rhei");
    fs::create_dir_all(project.join("design")).expect("create the outside directory");
    write_fixture_file(&project.join("design"), "local-profile.json", "{}\n");
    write_fixture_file(&dir, "shared.md", "# shared\n");
    write_fixture_file(&project, "index.panta.md", "# Panta: Two Rheis\n");
    write_fixture_file(&project.join("ws"), "index.rhei.md", "# Rhei: Ws\n");
    let ws_body = if ws_escapes { format!("Read [profile]({ESCAPE}).\n") } else { String::new() };
    write_fixture_file(
        &ws_tasks,
        "01.md",
        &format!("### Task 1: Load\n**State:** pending\n\n{ws_body}"),
    );
    write_fixture_file(
        &project,
        "single.rhei.md",
        "# Rhei: Single\n\n## Tasks\n\n### Task 1: Load\n**State:** pending\n\n\
         Read [shared](../shared.md).\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let run = validate(&dir, &machine, &project);
    (dir, run)
}

/// Through the per-task bases the workspace rhei's escaping link is refused while
/// the single-file rhei's `../` is not. §FS-rhei-plan-language.3.6
#[test]
fn a_panta_project_holds_each_link_to_its_own_rheis_rule() {
    let (_dir, run) = panta_project("link-escape-panta", true);
    assert_refused_as_escaping(&run, "panta workspace rhei");
    let said = flattened_output(&run);
    assert!(
        !said.contains("../shared.md"),
        "the single-file rhei's link should not be refused; got:\n{said}"
    );
}

/// The single-file half of the seam alone validates today and must keep doing
/// so. §FS-rhei-plan-language.3.6
#[test]
fn a_panta_single_file_rhei_may_link_above_its_directory() {
    let (_dir, run) = panta_project("link-escape-panta-single", false);
    assert_success(&run);
}
