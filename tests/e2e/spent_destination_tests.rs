//! Port of the issue #764 reproducer: successful completion loses the reason
//! an existing forward edge was refused, in the warning and both report runs.
// §FS-rhei-errors.1.5.1 §FS-rhei-run-report.2

use super::spent_destination_support::{text, Worker, World, REFUSAL};

fn row_gaps(row: &str) -> Vec<String> {
    let mut gaps = Vec::new();
    if !row.contains(REFUSAL) {
        gaps.push(format!("Attention omits spent destination: {row}"));
    }
    if !(row.contains("raise")
        && row.contains("visits:")
        && row.contains("check-review")
        && row.contains("cancel"))
    {
        gaps.push(format!("Attention omits destination remedy: {row}"));
    }
    for wrong in ["declare a transition", "inspect logs", "rhei retry"] {
        if row.contains(wrong) {
            gaps.push(format!("Attention gives inapplicable {wrong:?}: {row}"));
        }
    }
    gaps
}

fn completion_names_spent_destination(worker: Worker, parallel: usize) {
    let world = World::new(worker, parallel, false);
    let spending = world.run();
    world.assert_spent(&spending, true);
    let spending_rows = [world.row("1"), world.row("2")];
    let events = world.read("runtime/events.jsonl");
    let logged_warnings: Vec<String> = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|event| event["event"] == "message" && event["level"] == "warn")
        .filter_map(|event| event["text"].as_str().map(str::to_owned))
        .collect();
    let later = world.run();
    world.assert_spent(&later, true);
    let later_rows = [world.row("1"), world.row("2")];

    // Collect all missing surfaces: a first-warning failure must not hide the
    // independently broken spending-run and later-run Attention rows.
    let mut gaps = Vec::new();
    let console_warnings: Vec<_> = text(&spending)
        .lines()
        .filter(|line| line.contains("exited 0 but task"))
        .map(str::to_owned)
        .collect();
    for id in ["1", "2"] {
        let warning = format!(
            "warning: {} exited 0 but task index.{id} did not advance from 'audit': {REFUSAL}",
            world.worker.name()
        );
        if !text(&spending).contains(&warning) {
            gaps.push(format!(
                "completion warning omits spent destination; expected {warning:?}; got {console_warnings:?}"
            ));
        }
        if !logged_warnings.iter().any(|line| line.contains(&warning)) {
            gaps.push(format!("logged warning omits spent destination: {logged_warnings:?}"));
        }
    }
    for (run, rows) in [("spending run", spending_rows), ("later run", later_rows)] {
        gaps.extend(rows.iter().flat_map(|row| row_gaps(row)).map(|gap| format!("{run}: {gap}")));
    }
    assert!(gaps.is_empty(), "{:?} --parallel {parallel}\n{}", worker, gaps.join("\n"));
}

#[test]
fn issue_764_agent_sequential_names_spent_destination() {
    completion_names_spent_destination(Worker::Agent, 1);
}

#[test]
fn issue_764_agent_parallel_names_spent_destination() {
    completion_names_spent_destination(Worker::Agent, 2);
}

#[test]
fn issue_764_program_sequential_names_spent_destination() {
    completion_names_spent_destination(Worker::Program, 1);
}

#[test]
fn issue_764_program_parallel_names_spent_destination() {
    completion_names_spent_destination(Worker::Program, 2);
}
