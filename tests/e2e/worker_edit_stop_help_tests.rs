//! The `help:` of an attributed stop follows the line that broke: a break no
//! task's region holds gets the loader's help, the one `rhei validate` prints
//! for the same plan, and a break inside a task's region keeps the help about
//! what a task body may hold (agent-grounds/rhei#522).
//!
//! Every case here is a plan broken before the run starts, so no worker runs:
//! the startup load is attributed the way a mid-run reload is, and where the
//! line falls is all that tells the two helps apart.
// §FS-rhei-run.3.7.6

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// The sentence of the help about a task body's text.
const HEADING_HELP: &str = "a heading inside a task body declares a child task";

/// A directory plan named `name`, with `index` as its `index.rhei.md` and each
/// of `tasks` under `tasks/`.
fn directory_plan(dir: &TestDir, name: &str, index: &str, tasks: &[(&str, &str)]) -> PathBuf {
    let plan = dir.join(name);
    fs::create_dir_all(plan.join("tasks")).expect("create the plan's directories");
    fs::write(plan.join("index.rhei.md"), index).expect("write the index");
    for (file, text) in tasks {
        fs::write(plan.join("tasks").join(file), text).expect("write a task file");
    }
    plan
}

/// `rhei <args> <plan>` with its state under `dir`, read with miette's wrap undone.
fn rhei(dir: &TestDir, args: &[&str], plan: &Path) -> CliRun {
    let out = rhei_command(dir.join(".home"))
        .current_dir(dir)
        .args(args)
        .arg(plan)
        .output()
        .expect("rhei should run");
    CliRun::from(&out)
}

/// What a run on `plan` prints, in the order a reader meets it.
fn run_once(dir: &TestDir, plan: &Path) -> CliRun {
    let run = rhei(dir, &["run", "--no-tui", "--no-dashboard"], plan);
    assert!(!run.status.success(), "the plan is broken, so the run refuses\n{}", run.stderr);
    run
}

/// The diagnostic's `help:` footer, as one line.
fn help_of(run: &CliRun) -> String {
    run.stderr
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("help:"))
        .map(|help| help.trim().to_string())
        .unwrap_or_else(|| panic!("the diagnostic carries a help line:\n{}", run.stderr))
}

/// A plan broken outside every task's region stops `rhei run` with the head it
/// has today and the help `rhei validate` gives the same plan.
fn assert_loader_help(dir: &TestDir, plan: &Path, head: &str, message: &str) {
    let validate = rhei(dir, &["validate"], plan);
    assert!(!validate.status.success(), "validate refuses it too\n{}", validate.stderr);
    let loader_help = help_of(&validate);

    let run = run_once(dir, plan);
    assert!(run.stderr.contains(head), "the head names the line: `{head}`\n{}", run.stderr);
    assert!(run.stderr.contains(message), "the head carries the loader's message\n{}", run.stderr);
    assert!(!run.stderr.contains("'s text broke the plan"), "no task holds it\n{}", run.stderr);
    let help = help_of(&run);
    assert!(
        !help.contains(HEADING_HELP),
        "no task body holds the break, so the help must not send the reader to one: {help}"
    );
    assert!(
        help.contains(&loader_help),
        "the help is the loader's, as `rhei validate` prints it (`{loader_help}`): {help}"
    );
}

/// The report's first fixture: a retired `**States:**` header line, on line 2
/// of the index, above every task.
#[test]
#[ignore = "red until #522: a break outside every task region carries the worker-edit heading help; implement removes this"]
fn a_retired_states_line_stops_the_run_with_the_loaders_help() {
    let dir = unique_temp_dir("stop-help-states-line");
    let index = "# Rhei: demo\n**States:** x\n\n### Task 1: One\n**State:** pending\n\nBody.\n";
    let plan = directory_plan(&dir, "states-line", index, &[]);
    assert_loader_help(
        &dir,
        &plan,
        "the plan broke at index.rhei.md:2 (",
        "is no longer part of the plan language",
    );
}

/// The report's second fixture: a `## Tasks` section in a workspace index.
#[test]
#[ignore = "red until #522: a break outside every task region carries the worker-edit heading help; implement removes this"]
fn a_tasks_section_in_the_index_stops_the_run_with_the_loaders_help() {
    let dir = unique_temp_dir("stop-help-tasks-section");
    let index = "# Rhei: demo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\nBody.\n";
    let plan = directory_plan(&dir, "tasks-section", index, &[]);
    assert_loader_help(
        &dir,
        &plan,
        "the plan broke at index.rhei.md:3 (",
        "must not contain a '## Tasks' section",
    );
}

/// The boundary: a heading inside a task body, there before the run started, is
/// a break inside that task's region, so it keeps the help about task bodies.
#[test]
fn a_heading_in_a_task_body_before_the_run_keeps_the_heading_help() {
    let dir = unique_temp_dir("stop-help-heading-body");
    let task = "### Task 1: One\n**State:** pending\n\nBody.\n\n#### Notes\n\nMore.\n";
    let plan = directory_plan(&dir, "heading-body", "# Rhei: demo\n", &[("01-one.md", task)]);

    let run = run_once(&dir, &plan);
    assert!(
        run.stderr.contains("Task heading-body.1's text broke the plan at tasks/01-one.md:6"),
        "the break is attributed to the task whose region holds it\n{}",
        run.stderr
    );
    let help = help_of(&run);
    assert!(help.contains(HEADING_HELP), "the help is about the task body's text: {help}");
    assert!(!help.contains("**State:**, **Prior:**"), "not about task metadata: {help}");
}
