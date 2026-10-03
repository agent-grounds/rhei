//! A mixed task names every invocation kind with that kind's own count.
//! Ported from tool-reports/runtime/repro/rhei.32/run.sh (issue #406), with
//! unequal mixed counts and homogeneous controls. §FS-rhei-run-report.3.2

use super::shape_example_terminal_tests::run_on_a_terminal;
use super::terminal_result_tests::{write_mock_agent_settings, write_result_writing_agent};
use super::*;
use std::fmt::Write as _;

// Task id, entry state, actual invocation order, expected driver label.
const CASES: &[(&str, &str, &[&str], &str)] = &[
    ("plan.1", "commit", &["agent", "program"], "agent+program"),
    ("plan.2", "prepare", &["program", "agent"], "agent+program"),
    ("plan.3", "draft", &["agent", "agent", "agent"], "agent×3"),
    ("plan.4", "double_agent", &["agent", "agent", "program"], "agent×2+program"),
    ("plan.5", "prepare_twice", &["program", "program", "agent"], "agent+program×2"),
    ("plan.6", "polish", &["agent"], "agent"),
    ("plan.7", "opening", &["program"], "program"),
    ("plan.8", "double_program", &["program", "program"], "program×2"),
];

fn fixture() -> TestDir {
    let dir = unique_temp_dir("run-report-driver-labels");
    // The workspace name supplies qualified ids; keep the original repro's plan.N.
    let workspace = dir.join("plan");
    fs::create_dir_all(workspace.join("tasks")).expect("tasks directory");
    write_fixture_file(&workspace, "index.rhei.md", "# Rhei: Driver labels\n");
    let mut tasks = String::new();
    for (id, state, _, _) in CASES {
        let local_id = id.strip_prefix("plan.").expect("fixture task namespace");
        write!(tasks, "### Task {local_id}: Exercise {state}\n**State:** {state}\n\n")
            .expect("task markdown");
    }
    write_fixture_file(&workspace.join("tasks"), "01-labels.md", &tasks);
    let worker = write_result_writing_agent(&workspace, "Done.\n");
    write_mock_agent_settings(&workspace, &worker);
    let machine = include_str!("fixtures/run-report-driver-labels.yaml")
        .replace("PROGRAM_COMMAND", &fixture_command(&worker));
    write_fixture_file(&workspace, "states.yaml", &machine);
    dir
}

fn read_report(workspace: &Path) -> String {
    let report = fs::read_to_string(workspace.join("runtime/run-report.md")).expect("run report");
    let invocations = report
        .split("\n## Invocations\n")
        .nth(1)
        .expect("invocation table")
        .split("\n## ")
        .next()
        .expect("invocation section");
    for (task, _, expected_kinds, _) in CASES {
        let actual: Vec<_> = invocations
            .lines()
            .filter_map(|row| {
                let cells: Vec<_> = row.split('|').map(str::trim).collect();
                (cells.get(1) == Some(task)).then(|| {
                    assert_eq!(cells[3], "exit 0", "invocation failed: {row}");
                    cells[2]
                })
            })
            .collect();
        assert_eq!(&actual, expected_kinds, "{task}: invocation precondition\n{report}");
    }
    report
}

fn assert_labels(surface: &str, rows: &[(&str, &str)]) {
    let mut mismatches = Vec::new();
    for (task, _, _, expected) in CASES {
        let detail = rows
            .iter()
            .find_map(|(id, detail)| (*id == *task).then_some(*detail))
            .unwrap_or_else(|| panic!("{surface}: missing tree row for {task}: {rows:?}"));
        let actual = detail.split_whitespace().next().expect("driver label");
        if actual != *expected {
            mismatches.push(format!("{task}: expected {expected:?}, actual {actual:?}"));
        }
    }
    assert!(mismatches.is_empty(), "{surface} driver labels:\n{}", mismatches.join("\n"));
}

// §FS-rhei-run-report.3.2
#[test]
fn persisted_task_tree_counts_each_invocation_kind() {
    let dir = fixture();
    let workspace = dir.join("plan");
    let run = run_cli_without_machine("run", &workspace, &["--no-dashboard", "--no-tui"]);
    assert_success(&run);
    let report = read_report(&workspace);
    let tree = report
        .split("\n## Task Final States\n")
        .nth(1)
        .expect("Task Final States tree")
        .split("\n## ")
        .next()
        .expect("tree section");
    let rows: Vec<_> = tree
        .lines()
        .filter_map(|row| {
            let (_, after_id) = row.split_once('`')?;
            let (id, after_id) = after_id.split_once('`')?;
            let detail = after_id.strip_prefix(" (completed) — ")?;
            Some((id, detail))
        })
        .collect();
    assert_labels("persisted Task Final States", &rows);
}

// §FS-rhei-run-report.3.2
#[test]
fn console_task_tree_counts_each_invocation_kind() {
    let dir = fixture();
    let workspace = dir.join("plan");
    let transcript = run_on_a_terminal(&workspace);
    read_report(&workspace);
    assert!(transcript.contains("9 agents · 8 programs"), "Work tally:\n{transcript}");
    let tree = transcript
        .split("\nTasks")
        .nth(1)
        .expect("console task tree")
        .split("\n\n")
        .next()
        .expect("console tree rows");
    let rows: Vec<_> = tree
        .lines()
        .filter_map(|row| {
            let (before_state, detail) = row.split_once("completed")?;
            let task = before_state.split_whitespace().last()?;
            Some((task, detail.trim()))
        })
        .collect();
    assert_labels("console task tree", &rows);
}
