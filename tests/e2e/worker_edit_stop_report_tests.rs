//! A run that stops on a plan a worker broke still writes its run report, from
//! the plan as the run last loaded it, and the report records the worker whose
//! exit found the break as interrupted (agent-grounds/rhei#531).
//!
//! The scenario is the issue's: Task 1's program writes a heading into Task 3's
//! body, which no in-flight region holds, and exits 0, so the run's reload
//! meets the broken plan and stops, attributed. The control is the same plan
//! with Task 1 failing and editing nothing, an ordinary failed run, so the
//! broken-plan stop is the only difference between the two.
// §FS-rhei-run-report.1 §FS-rhei-run.3.7.6 §FS-rhei-run.3.2

use std::fs;
use std::path::PathBuf;

use super::accounting_support::last_run_id;
use super::worker_edit_revert_support::{Edit, Mode, Ran, Scenario};

/// The history entries the run left under `runtime/run-reports/`.
fn history_entries(case: &Scenario) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(case.runtime_file("runtime/run-reports")) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths
}

/// The lines of the report's section under `heading`, up to the next section.
fn section<'a>(report: &'a str, heading: &str) -> Vec<&'a str> {
    report
        .lines()
        .skip_while(|line| *line != heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect()
}

/// The rows of the report's table under `heading`, each split into trimmed cells.
fn table_rows(report: &str, heading: &str) -> Vec<Vec<String>> {
    section(report, heading)
        .into_iter()
        .filter(|line| line.starts_with('|') && !line.starts_with("| ---"))
        .skip(1)
        .map(|line| line.trim_matches('|').split('|').map(|cell| cell.trim().to_string()).collect())
        .collect()
}

/// What a failing assertion shows: the run's streams, and what `runtime/` holds.
fn diagnostics(case: &Scenario, ran: &Ran) -> String {
    let runtime: Vec<String> = fs::read_dir(case.runtime_file("runtime"))
        .map(|entries| {
            entries.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into())).collect()
        })
        .unwrap_or_default();
    format!("{}\nruntime/ holds: {runtime:?}", ran.output())
}

/// The run wrote `runtime/run-report.md` and exactly one history entry, both
/// this run's report by its `Run:` header, and pointed at it; returns the
/// report. §FS-rhei-run-report.1
fn assert_reported(case: &Scenario, ran: &Ran) -> String {
    let output = diagnostics(case, ran);
    let latest = case.runtime_file("runtime/run-report.md");
    let report = fs::read_to_string(&latest)
        .unwrap_or_else(|_| panic!("the run writes runtime/run-report.md\n{output}"));
    let history = history_entries(case);
    assert_eq!(history.len(), 1, "one runtime/run-reports/ entry: {history:?}\n{output}");
    let entry = fs::read_to_string(&history[0]).expect("read the history entry");
    assert_eq!(entry, report, "the history entry is the latest report\n{output}");

    let id = last_run_id(&case.ws);
    let header = report.lines().find(|line| line.starts_with("Run: ")).unwrap_or_default();
    assert!(header.ends_with(&format!(" / {id}")), "`Run:` carries run {id}: {header}\n{report}");
    assert!(
        ran.run.stdout.contains("Report: runtime/run-report.md"),
        "the run points at the report it wrote\n{output}"
    );
    report
}

/// The issue's break: the run stops attributed, as it does today, and writes its
/// report from the plan as it last loaded it, with Task 1's invocation in the
/// ledger and the invocations as `interrupted`. §FS-rhei-run.3.7.6
#[test]
fn a_run_stopped_on_a_plan_a_worker_broke_still_writes_its_report() {
    let case = Scenario::new("stop-report", Mode::Sequential, Edit::OtherTask, true, false);
    let ran = case.run();
    let output = diagnostics(&case, &ran);

    assert!(!ran.run.status.success(), "the run stops\n{output}");
    assert!(
        ran.combined().contains("Task ws.3's text broke the plan at tasks/03-later.md:7 ("),
        "the stop is the attributed one\n{output}"
    );
    assert!(case.task_file("03-later.md").contains("#### Visit"), "the break stands\n{output}");

    let report = assert_reported(&case, &ran);

    // The plan as the run last loaded it: every task, Task 1 still in its state.
    let states = section(&report, "## Task Final States").join("\n");
    for row in ["`ws.1` (cover)", "`ws.2` (other)", "`ws.3` (other)"] {
        assert!(states.contains(row), "the final states list {row}\n{report}");
    }

    // Interrupted, not completed, with no transition out of `cover`. §FS-rhei-run.3.2
    let ledger = table_rows(&report, "## Transition Ledger");
    let row = ledger
        .iter()
        .find(|cells| cells[0].contains("ws.1"))
        .unwrap_or_else(|| panic!("the ledger has a row for ws.1\n{report}"));
    assert_eq!(row[1], "cover", "the row records the state it ran in\n{report}");
    assert_eq!(row[2], "-", "and no transition out of it\n{report}");
    assert_eq!(row[5], "interrupted", "the ledger reason\n{report}");

    let invocations = table_rows(&report, "## Invocations");
    let invocation = invocations
        .iter()
        .find(|cells| cells[0].contains("ws.1"))
        .unwrap_or_else(|| panic!("the invocations list ws.1\n{report}"));
    assert_eq!(invocation[2], "interrupted", "the invocation's exit\n{report}");
}

/// The control: the same plan, Task 1 failing without an edit, is an ordinary
/// failed run, and it writes its report today. §FS-rhei-run-report.1
#[test]
fn an_ordinary_failed_run_of_the_same_plan_writes_its_report() {
    let case = Scenario::new("stop-report-control", Mode::Sequential, Edit::Fail, true, false);
    let ran = case.run();
    let output = diagnostics(&case, &ran);

    assert!(!ran.run.status.success(), "Task 1 fails the run\n{output}");
    assert!(!ran.combined().contains("broke the plan"), "nothing broke the plan\n{output}");
    assert_eq!(case.runs_of("cover"), 1, "Task 1 ran once\n{output}");

    let report = assert_reported(&case, &ran);
    let ledger = table_rows(&report, "## Transition Ledger");
    assert!(ledger.iter().any(|cells| cells[0].contains("ws.1")), "ws.1 has a row\n{report}");
}
