//! A task heading authored with no `## Tasks` section above it (#685): the
//! plan is refused, and the refusal names the missing section at the heading's
//! line with the edit that fixes it. §FS-rhei-errors.1.7

use super::*;

/// The plan of #685, line 3 its task heading.
const ITALY: &str = "# Rhei: Italy\n\n### Task 1: Book the trains\n**State:** in-progress\n";
/// The control: the same plan with `## Tasks` above the heading.
const ITALY_WITH_TASKS: &str =
    "# Rhei: Italy\n\n## Tasks\n\n### Task 1: Book the trains\n**State:** in-progress\n";

fn run_on(prefix: &str, plan: &str, command: &str) -> CliRun {
    let dir = unique_temp_dir(prefix);
    let path = write_fixture_file(&dir, "italy.rhei.md", plan);
    run_cli_without_machine(command, &path, &[])
}

fn help_of(run: &CliRun) -> String {
    run.stderr.split_once("help:").map(|(_, help)| help.trim().to_owned()).unwrap_or_default()
}

fn assert_names_the_missing_section(run: &CliRun) {
    assert_eq!(run.status.code(), Some(1), "the plan stays refused: {}", run.stderr);
    assert!(
        !run.stderr.contains("Metadata field appears outside a task"),
        "the field sits under a task heading; the section is what is missing:\n{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("3| ### Task 1: Book the trains"),
        "the frame must point at the heading's line:\n{}",
        run.stderr
    );
    let help = help_of(run);
    assert!(
        help.contains("## Tasks") && help.contains("line 3"),
        "the help must be the edit: add '## Tasks' above line 3; got: {help}\n{}",
        run.stderr
    );
    assert!(
        !help.contains("check the plan's task metadata"),
        "the generic metadata help points at lines that are correct: {help}"
    );
}

#[test]
fn the_control_with_a_tasks_section_lists_the_task() {
    let run = run_on("rhei-685-control", ITALY_WITH_TASKS, "list");

    assert!(run.status.success(), "{}", run.stderr);
    assert!(run.stdout.contains("Task italy.1: Book the trains [in-progress]"), "{}", run.stdout);
}

#[test]
fn list_names_the_missing_tasks_section_at_the_task_heading() {
    let run = run_on("rhei-685-list", ITALY, "list");

    assert_names_the_missing_section(&run);
}

#[test]
fn validate_reports_the_missing_tasks_section_once_as_the_cause() {
    let run = run_on("rhei-685-validate", ITALY, "validate");

    assert_names_the_missing_section(&run);
    assert!(
        !run.stderr.contains("problems)") && !run.stderr.contains("distinct"),
        "one cause, not two distinct faults:\n{}",
        run.stderr
    );
}

#[test]
fn list_names_a_task_heading_left_in_a_content_section() {
    let plan = "# Rhei: Italy\n\n## Notes\n\n### Task 1: Book the trains\n**State:** pending\n\n\
                ## Tasks\n\n### Task 2: Pack\n**State:** pending\n";
    let run = run_on("rhei-685-content", plan, "list");

    assert_eq!(run.status.code(), Some(1), "the plan stays refused: {}", run.stderr);
    assert!(!run.stderr.contains("Metadata field appears outside a task"), "{}", run.stderr);
    assert!(run.stderr.contains("5| ### Task 1: Book the trains"), "{}", run.stderr);
    let help = help_of(&run);
    assert!(help.contains("## Tasks"), "the help must move the task under '## Tasks': {help}");
    assert!(!help.contains("check the plan's task metadata"), "generic help: {help}");
}
