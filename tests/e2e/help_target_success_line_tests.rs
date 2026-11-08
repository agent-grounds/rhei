//! Two lines rhei prints on success, each pasted from where the command was typed:
//! the commands of `rhei release`'s reclaim note (§FS-rhei-release.3.1) and the
//! self-loop `rhei next` hands a manual supervisor (§FS-rhei-supervision.3.4). Both
//! name the target and machine the command was given, by §FS-rhei-errors.1.2.1.
//!
//! The fixture and the claims are those of `agent-grounds/rhei#586`'s reproducer:
//! a project of two members, typed from its parent as a member, as the project, and
//! omitted from inside it. Pasting needs `sh`, so each is a Unix claim.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use super::help_target_paste_tests::write;
use super::help_target_tests::{pasted, Case};
use super::*;

/// One supervisor with a child, and one ticket claimed and left past its first state.
const AUTH_PLAN: &str = "# Rhei: auth\n\n## Tasks\n\n\
                         ### Task 1: A supervising ticket\n**State:** supervising\n\n\
                         Supervises what is below it.\n\n\
                         #### Task 1.1: A step\n**State:** todo\n\nDo the step.\n\n\
                         ### Task 2: A ticket claimed and left past its first state\n\
                         **State:** working\n**Assignee:** someone\n\nLeft where it stands.\n";

/// The member beside it, which a widened target would also reach.
const OTHER_PLAN: &str = "# Rhei: other\n\n## Tasks\n\n\
                          ### Task 1: Another member's ticket\n**State:** todo\n";

const MACHINE: &str = r#"name: machine
version: 1.0.0
states:
  todo:
    initial: true
    description: Waiting to be claimed.
  supervising:
    description: Holds the subtree below it.
    execute_on: child-terminal
    agent: claude-code
    instructions: Supervise Task {task_id}.
  working:
    description: Being worked.
    agent: claude-code
    instructions: Work Task {task_id}.
  done:
    final: true
    description: Finished.
transitions:
  - { from: todo, to: working }
  - { from: working, to: todo }
  - { from: working, to: done }
  - { from: supervising, to: supervising }
  - { from: supervising, to: done }
"#;

/// Where a given machine lives, beside the project and not in it.
const MACHINE_ARG: &str = "machines/custom.yaml";

/// `project/` under the case root, with its machine inside it or, when `given`, only
/// at [`MACHINE_ARG`], where nothing finds it unless it is passed on.
fn project(case: &Case, given: bool) -> PathBuf {
    let project = case.root.join("project");
    write(&project, "index.panta.md", "# Panta: Fixture\n");
    write(&project, "auth.rhei.md", AUTH_PLAN);
    write(&project, "other.rhei.md", OTHER_PLAN);
    if given {
        write(&case.root, MACHINE_ARG, MACHINE);
    } else {
        write(&project, "states.yaml", MACHINE);
    }
    project
}

/// The command between `` `{after}`` and the next backtick in `text`.
#[track_caller]
fn quoted_after(text: &str, after: &str) -> String {
    let (_, rest) = text
        .split_once(&format!("{after}`"))
        .unwrap_or_else(|| panic!("no `{after}` in the output:\n{text}"));
    rest.split_once('`').expect("a closing backtick").0.to_owned()
}

/// The reclaim note's two commands: its `rhei next` and its `rhei transition`.
#[track_caller]
fn release_note(case: &Case, cwd: &Path, args: &[&str]) -> (String, String) {
    let out = case.rhei(cwd, args);
    let run = CliRun::from(&out);
    assert!(run.status.success(), "the release works:\n{}{}", run.stdout, run.stderr);
    (
        quoted_after(&run.stdout, "reclaim it explicitly with "),
        quoted_after(&run.stdout, "move it back with "),
    )
}

/// The self-loop `rhei next` hands the manual supervisor it claimed.
#[track_caller]
fn supervisor_handoff(case: &Case, cwd: &Path, args: &[&str]) -> String {
    let out = case.rhei(cwd, args);
    let run = CliRun::from(&out);
    assert!(run.status.success(), "the claim works:\n{}{}", run.stdout, run.stderr);
    run.stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("rhei transition ") && line.contains("--release-subtree"))
        .unwrap_or_else(|| panic!("no release command in the output:\n{}", run.stdout))
        .to_owned()
}

/// `line`, pasted into `sh` from `cwd`, runs and does what `says` names.
#[track_caller]
fn runs_as_printed(line: &str, cwd: &Path, home: &Path, says: &str) {
    let again = pasted(line, cwd, home);
    assert!(
        again.status.success() && again.stdout.contains(says),
        "`{line}` should run as printed from {}:\n{}{}",
        cwd.display(),
        again.stdout,
        again.stderr
    );
}

/// The ticket's own line: a member typed from the project's parent. The undo is
/// printed with no target, so pasted there it finds no plan. §FS-rhei-release.3.1
/// §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #586: the release note's undo has no target and fails pasted from the parent; implement removes this"]
fn help_target_a_release_note_from_the_parent_names_the_member() {
    let case = Case::new("help-target-release-note-member");
    project(&case, false);
    let typed = "project/auth.rhei.md";

    let (reclaim, move_back) =
        release_note(&case, &case.root, &["release", typed, "--task", "auth.2"]);

    let word = shell_quote(typed);
    assert_eq!(reclaim, format!("rhei next {word} --task auth.2"));
    assert_eq!(move_back, format!("rhei transition {word} --task auth.2 --from working --to todo"));
    runs_as_printed(&move_back, &case.root, &case.home, "transitioned");
}

/// A typed project is printed as the project, as typed. §FS-rhei-release.3.1
/// §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #586: the release note's undo has no target; implement removes this"]
fn help_target_a_release_note_for_a_typed_project_names_the_project() {
    let case = Case::new("help-target-release-note-project");
    project(&case, false);

    let (reclaim, move_back) =
        release_note(&case, &case.root, &["release", "project", "--task", "auth.2"]);

    assert_eq!(reclaim, "rhei next project --task auth.2");
    assert_eq!(move_back, "rhei transition project --task auth.2 --from working --to todo");
    runs_as_printed(&move_back, &case.root, &case.home, "transitioned");
}

/// With the target omitted, both halves of the note name what release discovered,
/// the same word, and the undo runs where release was typed. §FS-rhei-release.3.1
/// §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #586: the release note's undo has no target while its reclaim names one; implement removes this"]
fn help_target_a_release_note_with_no_target_names_what_it_discovered() {
    let case = Case::new("help-target-release-note-bare");
    let project = project(&case, false);

    let (reclaim, move_back) = release_note(&case, &project, &["release", "auth.2"]);

    let target = reclaim
        .strip_prefix("rhei next ")
        .and_then(|rest| rest.strip_suffix(" --task auth.2"))
        .unwrap_or_else(|| panic!("the reclaim names a target: {reclaim}"));
    assert_eq!(
        move_back,
        format!("rhei transition {target} --task auth.2 --from working --to todo")
    );
    runs_as_printed(&move_back, &project, &case.home, "transitioned");
}

/// A machine given to `rhei release` is passed on by both halves; without it the
/// pasted undo resolves no machine that declares `working`. §FS-rhei-release.3.1
/// §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #586: the release note drops the machine it was given; implement removes this"]
fn help_target_a_release_note_passes_on_the_machine_it_was_given() {
    let case = Case::new("help-target-release-note-machine");
    project(&case, true);
    let typed = "project/auth.rhei.md";

    let (reclaim, move_back) = release_note(
        &case,
        &case.root,
        &["release", typed, "--task", "auth.2", "--state-machine", MACHINE_ARG],
    );

    let (word, machine) = (shell_quote(typed), shell_quote(MACHINE_ARG));
    assert_eq!(reclaim, format!("rhei next {word} --task auth.2 --state-machine {machine}"));
    assert_eq!(
        move_back,
        format!(
            "rhei transition {word} --task auth.2 --from working --to todo --state-machine {machine}"
        )
    );
    runs_as_printed(&move_back, &case.root, &case.home, "transitioned");
}

/// The ticket's second line: a member typed from the parent is handed back as typed,
/// not as the absolute project it widened to. §FS-rhei-supervision.3.4
/// §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #586: the handoff names the absolute project a typed member widened to; implement removes this"]
fn help_target_a_supervisor_handoff_from_the_parent_names_the_member() {
    let case = Case::new("help-target-handoff-member");
    project(&case, false);
    let typed = "project/auth.rhei.md";

    let line = supervisor_handoff(&case, &case.root, &["next", typed, "--task", "auth.1"]);

    assert_eq!(
        line,
        format!(
            "rhei transition {} --task auth.1 --from supervising --to supervising --release-subtree",
            shell_quote(typed)
        )
    );
    runs_as_printed(&line, &case.root, &case.home, "released the subtree of Task auth.1");
}

/// A typed project is handed back as typed, not made absolute. §FS-rhei-supervision.3.4
/// §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_supervisor_handoff_for_a_typed_project_names_the_project() {
    let case = Case::new("help-target-handoff-project");
    project(&case, false);

    let line = supervisor_handoff(&case, &case.root, &["next", "project", "--task", "auth.1"]);

    assert_eq!(
        line,
        "rhei transition project --task auth.1 --from supervising --to supervising --release-subtree"
    );
    runs_as_printed(&line, &case.root, &case.home, "released the subtree of Task auth.1");
}

/// With the target omitted the handoff names a target, and runs where `rhei next`
/// was typed. §FS-rhei-supervision.3.4 §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_supervisor_handoff_with_no_target_runs_where_it_was_typed() {
    let case = Case::new("help-target-handoff-bare");
    let project = project(&case, false);

    let line = supervisor_handoff(&case, &project, &["next", "--task", "auth.1"]);

    assert!(!line.starts_with("rhei transition --"), "the handoff names a target: {line}");
    runs_as_printed(&line, &project, &case.home, "released the subtree of Task auth.1");
}

/// A machine given to `rhei next` is passed on once, after the member as typed.
/// §FS-rhei-supervision.3.4 §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #586: the handoff names the absolute project a typed member widened to; implement removes this"]
fn help_target_a_supervisor_handoff_passes_on_the_machine_it_was_given() {
    let case = Case::new("help-target-handoff-machine");
    project(&case, true);
    let typed = "project/auth.rhei.md";

    let line = supervisor_handoff(
        &case,
        &case.root,
        &["next", typed, "--task", "auth.1", "--state-machine", MACHINE_ARG],
    );

    assert_eq!(
        line,
        format!(
            "rhei transition {} --task auth.1 --from supervising --to supervising \
             --release-subtree --state-machine {}",
            shell_quote(typed),
            shell_quote(MACHINE_ARG)
        )
    );
    runs_as_printed(&line, &case.root, &case.home, "released the subtree of Task auth.1");
}
