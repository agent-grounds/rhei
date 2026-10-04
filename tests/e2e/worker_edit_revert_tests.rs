//! One worker's task-body edit that breaks the plan costs that worker's
//! attempt, not the run (agent-grounds/rhei#310).
//!
//! Every case runs sequentially, under `--parallel 2`, and under
//! `--continue-on-error`, because none of the three may change what a reverted
//! edit means. The cases are `#[ignore]`d until the restore lands, because the
//! commit gate runs the suite; the change that makes each pass removes its
//! attribute. §FS-rhei-run.3.7

use std::fs;

use super::worker_edit_revert_support::*;

/// The help the old diagnostic printed, which sent the reader to task metadata.
const METADATA_HELP: &str = "**State:**, **Prior:**";

/// The ticket's own case: Task 1 writes `#### Visit 1 (cover)` into its body
/// and exits 0. Task 2 still completes, Task 1 is reverted and completes on its
/// second attempt, and the operator is told which task broke what, where.
// §FS-rhei-run.3.7.3 §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.7 §FS-rhei-run-tui.1.7
#[test]
#[ignore = "red until #310 restores a worker's region"]
fn a_heading_in_its_own_body_reverts_that_task_and_the_sibling_completes() {
    for mode in MODES {
        let case = Scenario::new("ticket", mode, Edit::Once, false, false);
        let ran = case.run();
        let out = ran.combined();

        assert_eq!(
            case.state_of("02-other.md"),
            "completed",
            "Task 2 must finish\n{}",
            ran.output()
        );
        assert!(ran.run.status.success(), "the run survives the edit\n{}", ran.output());
        assert_eq!(
            case.state_of("01-cover.md"),
            "completed",
            "Task 1 completes on attempt 2\n{}",
            ran.output()
        );
        assert_eq!(case.runs_of("cover"), 2, "Task 1 spent two attempts\n{}", ran.output());
        assert_eq!(case.runs_of("other"), 1, "Task 2 ran once\n{}", ran.output());
        assert!(!case.task_file("01-cover.md").contains("#### Visit"), "the heading left the plan");

        for expected in [
            "Task ws.1's edit to its own task body broke the plan at tasks/01-cover.md:6",
            "Malformed node heading",
            "Reverted Task ws.1",
            "attempt 1 of 2 spent",
            "runtime/logs/task-ws.1-cover.reverted.md",
            "help: write progress as plain paragraphs or lists",
        ] {
            assert!(
                out.contains(expected),
                "the warning must carry `{expected}`\n{}",
                ran.output()
            );
        }
        assert!(!out.contains(METADATA_HELP), "the help is about headings\n{}", ran.output());

        let reverted =
            fs::read_to_string(case.runtime_file("runtime/logs/task-ws.1-cover.reverted.md"))
                .unwrap_or_else(|_| panic!("the worker's text is kept\n{}", ran.output()));
        assert!(reverted.contains("#### Visit 1 (cover)"), "got:\n{reverted}");

        let journal = case.journal();
        let ends = end_records(&journal, "ws.1", "cover");
        assert_eq!(ends.len(), 2, "one record per attempt; journal:\n{journal}");
        assert_eq!(meta_value(&ends[0], "outcome").as_deref(), Some("failed"), "{journal}");
        assert_eq!(
            meta_value(&ends[0], "reverted").as_deref(),
            Some("tasks/01-cover.md:6"),
            "{journal}"
        );
        assert!(ends[1].contains("-attempt2.log"), "attempt 2 follows: {journal}");
        assert_eq!(meta_value(&ends[1], "outcome").as_deref(), Some("completed"), "{journal}");
        assert_eq!(meta_value(&ends[1], "reverted"), None, "{journal}");
        assert_eq!(journal.matches("reverted=").count(), 1, "{journal}");
        for record in end_records(&journal, "ws.2", "other") {
            assert_eq!(meta_value(&record, "outcome").as_deref(), Some("completed"), "{journal}");
        }

        // The JSON form mirrors the journal key. §FS-rhei-run-json.2.1
        let released = slot_released(&case.ws);
        assert!(
            released.iter().any(|record| record["task"] == "ws.1"
                && record["outcome"] == "failed"
                && record["reverted"] == "tasks/01-cover.md:6"),
            "events.jsonl carries the reverted release: {released:#?}"
        );
    }
}

/// A worker that breaks the plan on every attempt spends its own budget and
/// stalls; its sibling still completes, and the halt names the culprit.
// §FS-rhei-run.3.7.4 §FS-rhei-agents.3.2.3
#[test]
#[ignore = "red until #310 restores a worker's region"]
fn a_worker_that_breaks_the_plan_every_time_stalls_alone() {
    for mode in MODES {
        let case = Scenario::new("always", mode, Edit::Always, false, false);
        let ran = case.run();

        assert_eq!(
            case.state_of("02-other.md"),
            "completed",
            "Task 2 must finish\n{}",
            ran.output()
        );
        assert!(!ran.run.status.success(), "the stalled culprit fails the run\n{}", ran.output());
        assert_eq!(case.state_of("01-cover.md"), "cover", "no transition fires\n{}", ran.output());
        assert_eq!(case.runs_of("cover"), 2, "the default budget is two\n{}", ran.output());
        assert!(!case.task_file("01-cover.md").contains("#### Visit"), "both edits reverted");
        assert!(
            ran.combined().contains("halting Task ws.1 in state 'cover': 2 attempts spent"),
            "the halt names the culprit and its spent budget\n{}",
            ran.output()
        );
        assert!(case.runtime_file("runtime/logs/task-ws.1-cover.reverted.md").is_file());
        assert!(case.runtime_file("runtime/logs/task-ws.1-cover-attempt2.reverted.md").is_file());
        let journal = case.journal();
        let ends = end_records(&journal, "ws.1", "cover");
        assert_eq!(ends.len(), 2, "{journal}");
        assert!(ends.iter().all(|line| meta_value(line, "reverted").is_some()), "{journal}");
    }
}

/// A valid child task appended in the same attempt goes with the bad heading,
/// and is kept in the reverted-text file for the retry to add again.
// §FS-rhei-run.3.7.7
#[test]
#[ignore = "red until #310 restores a worker's region"]
fn a_valid_child_appended_beside_the_heading_is_reverted_and_kept() {
    for mode in MODES {
        let case = Scenario::new("child", mode, Edit::ChildAndNote, false, false);
        let ran = case.run();

        assert_eq!(
            case.state_of("02-other.md"),
            "completed",
            "Task 2 must finish\n{}",
            ran.output()
        );
        assert!(ran.run.status.success(), "{}", ran.output());
        let task = case.task_file("01-cover.md");
        assert!(!task.contains("Task 1.1"), "the child is reverted with the note:\n{task}");
        let reverted =
            fs::read_to_string(case.runtime_file("runtime/logs/task-ws.1-cover.reverted.md"))
                .unwrap_or_else(|_| panic!("the worker's text is kept\n{}", ran.output()));
        assert!(reverted.contains("#### Task 1.1: Child the worker added"), "got:\n{reverted}");
        assert!(reverted.contains("#### Visit 1 (cover)"), "got:\n{reverted}");
    }
}

/// An edit to a task that no worker is running is not attributable to an
/// exited worker: the run stops, names the task whose text holds the line, and
/// charges nobody — the in-flight sibling and the worker whose exit found the
/// break are both recorded as interrupted.
// §FS-rhei-run.3.7.2 §FS-rhei-run.3.7.6 §FS-rhei-run.3.2
#[test]
#[ignore = "red until #310 restores a worker's region"]
fn an_edit_to_another_tasks_body_stops_the_run_attributed() {
    for mode in MODES {
        let case = Scenario::new("other-task", mode, Edit::OtherTask, true, false);
        let ran = case.run();
        let out = ran.combined();

        assert!(!ran.run.status.success(), "the run stops\n{}", ran.output());
        assert!(out.contains("tasks/03-later.md:7"), "names file:line\n{}", ran.output());
        assert!(out.contains("Task ws.3"), "names the task whose text broke\n{}", ran.output());
        assert!(out.contains("Malformed node heading"), "{}", ran.output());
        assert!(!out.contains(METADATA_HELP), "the help is about headings\n{}", ran.output());
        assert!(case.task_file("03-later.md").contains("#### Visit"), "nothing is reverted");

        let journal = case.journal();
        let culprit = end_records(&journal, "ws.1", "cover");
        assert_eq!(culprit.len(), 1, "{journal}");
        assert_eq!(meta_value(&culprit[0], "outcome").as_deref(), Some("interrupted"), "{journal}");
        if mode.concurrent() {
            for record in end_records(&journal, "ws.2", "other") {
                assert_eq!(
                    meta_value(&record, "outcome").as_deref(),
                    Some("interrupted"),
                    "{journal}"
                );
            }
        }
    }
}

/// The break lands while its writer is still running and a sibling exits
/// first: the run waits for the writer instead of stopping, restores its region
/// once it exits, and the sibling is processed, not killed.
// §FS-rhei-run.3.7.5
#[test]
#[ignore = "red until #310 restores a worker's region"]
fn a_sibling_that_exits_during_the_break_waits_for_the_writer() {
    let mode = Mode::Parallel;
    let case = Scenario::new("lingering", mode, Edit::Once, false, true);
    let ran = case.run();

    assert_eq!(case.state_of("02-other.md"), "completed", "Task 2 must finish\n{}", ran.output());
    assert!(ran.run.status.success(), "{}", ran.output());
    assert_eq!(case.state_of("01-cover.md"), "completed", "{}", ran.output());
    assert_eq!(case.runs_of("other"), 1, "Task 2's exit was processed once\n{}", ran.output());
    let journal = case.journal();
    let ends = end_records(&journal, "ws.1", "cover");
    assert_eq!(ends.len(), 2, "{journal}");
    assert_eq!(
        meta_value(&ends[0], "reverted").as_deref(),
        Some("tasks/01-cover.md:6"),
        "{journal}"
    );
    for record in end_records(&journal, "ws.2", "other") {
        assert_eq!(meta_value(&record, "outcome").as_deref(), Some("completed"), "{journal}");
    }
}
