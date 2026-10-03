//! The run report's `## Bounds` end row is the account the run left behind.
//!
//! "Consumed at end" is read once the run's last receipt is durable, so the
//! invocation that ended the run is in it — its count, its spend, and the mark
//! saying how much of that spend was charged at the fallback. Reading the
//! account at the run's last *admission* instead left exactly that invocation
//! out: a one-ticket run reported `$0.00` and no mark for a day `rhei budget
//! show` said was charged `$20.00` (agent-grounds/rhei#412).
//!
//! Every case here compares the end row with what `rhei budget show` prints for
//! the same account right after the run, which is the one reading of "the end
//! of the run" a person can check by hand.

// §FS-rhei-run-report.3.1 §FS-rhei-budgets.9 §FS-rhei-run-json.2.1 §FS-rhei-budgets.6.2

use std::collections::BTreeMap;

use super::budget_support::*;
use super::*;

/// The built-in price book's one entry, so the invocation is measured and
/// priced at [`PRICED_INVOCATION`] with no fallback. §FS-rhei-cost-accounting.5.1
const PRICED_MODEL: &str = "claude-sonnet-4-6";
const PRICED_TARGET: &str = "claude-code[yolo]:anthropic:claude-sonnet-4-6";

/// An agent id no accounting extractor is chosen by, so each invocation is
/// settled at the reserve as `unmeasurable` — the shape of the issue's `cdx`
/// profile without a `family`. §FS-rhei-cost-accounting.3.2
const UNMEASURABLE_TARGET: &str = "mock[yolo]:anthropic:claude-sonnet-4-6";

/// Two tickets in the same agent state, so the run's last admission already
/// carries the first ticket's settle and only the second's is at stake.
const TWO_TICKET_PLAN: &str = r#"# Rhei: Bounded work

## Tasks

### Task 1: Work
**State:** work

### Task 2: More work
**State:** work
"#;

/// A finishing workspace whose one agent state targets `target`, with
/// `tickets` tickets in it.
fn setup_end_row(
    prefix: &str,
    target: &str,
    agent_id: &str,
    tickets: usize,
) -> (TestDir, PathBuf, PathBuf) {
    let (dir, plan, machine) = setup_spend(
        prefix,
        &spend_finishing(target),
        PRICED_FINISHING_AGENT,
        agent_id,
        PRICED_MODEL,
        "",
    );
    if tickets == 2 {
        fs::write(&plan, TWO_TICKET_PLAN).expect("write the two-ticket plan");
    }
    (dir, plan, machine)
}

/// The `## Bounds` row of `dimension`, keyed by its column header, so a column
/// added beside the others does not move what a cell is read as.
fn bounds_row(root: &Path, dimension: &str) -> BTreeMap<String, String> {
    let report =
        fs::read_to_string(root.join("runtime/run-report.md")).expect("read the run report");
    let section = report
        .split("## Bounds\n")
        .nth(1)
        .unwrap_or_else(|| panic!("the run report has no ## Bounds section:\n{report}"));
    let cells = |line: &str| -> Vec<String> {
        line.trim().trim_matches('|').split('|').map(|cell| cell.trim().to_owned()).collect()
    };
    let header = section
        .lines()
        .find(|line| line.starts_with("| Dimension |"))
        .unwrap_or_else(|| panic!("the ## Bounds section has no header:\n{section}"));
    let row = section
        .lines()
        .find(|line| line.starts_with(&format!("| {dimension} |")))
        .unwrap_or_else(|| panic!("the ## Bounds section has no {dimension} row:\n{section}"));
    cells(header).into_iter().zip(cells(row)).collect()
}

/// `consumed`, `outstanding` and `remaining` from `rhei budget show`'s line for
/// `dimension`: `<dim>: <c> consumed + <o> outstanding / <bound>; <r> remaining …`.
fn account(shown: &str, dimension: &str) -> (String, String, String) {
    let line = shown
        .lines()
        .find(|line| line.starts_with(&format!("{dimension}: ")))
        .unwrap_or_else(|| panic!("rhei budget show printed no {dimension} line:\n{shown}"));
    let words: Vec<&str> = line.split_whitespace().collect();
    let after = |marker: &str| -> String {
        let at = words
            .iter()
            .position(|w| *w == marker)
            .unwrap_or_else(|| panic!("no {marker} in {line}"));
        words[at - 1].trim_end_matches(';').to_owned()
    };
    (after("consumed"), after("outstanding"), after("remaining"))
}

fn cell<'a>(row: &'a BTreeMap<String, String>, column: &str, dimension: &str) -> &'a str {
    row.get(column)
        .unwrap_or_else(|| panic!("the {dimension} row has no `{column}` column; row: {row:?}"))
}

/// One dimension's end row against the account the run left behind, in the
/// order that names the reported defect first: the lagging consumed amount and
/// its mark, then the remainder, then the outstanding column the row must carry
/// so its amounts add up to the bound. §FS-rhei-run-report.3.1
fn assert_end_row(
    root: &Path,
    shown: &str,
    dimension: &str,
    consumed_cell: &str,
    remaining: &str,
    outstanding: &str,
) {
    let row = bounds_row(root, dimension);
    let (shown_consumed, shown_outstanding, shown_remaining) = account(shown, dimension);
    assert_eq!(
        (shown_outstanding.as_str(), shown_remaining.as_str()),
        (outstanding, remaining),
        "premise: rhei budget show holds nothing outstanding after the run; got:\n{shown}"
    );
    assert!(
        consumed_cell.starts_with(&shown_consumed),
        "premise: rhei budget show reports {shown_consumed} consumed for {dimension}; got:\n{shown}"
    );
    assert_eq!(
        cell(&row, "Consumed at end", dimension),
        consumed_cell,
        "{dimension}: Consumed at end is the account after the run's last receipt, the \
         invocation that ended the run included, as rhei budget show reports it; row: {row:?}"
    );
    assert_eq!(
        cell(&row, "Remaining", dimension),
        remaining,
        "{dimension}: Remaining is the account's remainder once the run has exited, not \
         the bound less the reserve held at the last admission; row: {row:?}"
    );
    assert_eq!(
        cell(&row, "Outstanding at end", dimension),
        outstanding,
        "{dimension}: the row carries what is still outstanding, so consumed plus \
         outstanding plus remaining is the bound; row: {row:?}"
    );
}

/// The worst case the issue names: a one-ticket run, the common shape of a
/// supervised step, whose report said the day was not spent at all.
// §FS-rhei-run-report.3.1 §FS-rhei-budgets.6.2
#[test]
fn a_one_ticket_runs_bounds_end_row_carries_its_only_invocation() {
    let (dir, plan, machine) = setup_end_row("bounds-end-one", UNMEASURABLE_TARGET, "mock", 1);

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    let shown = budget_show(&plan, Some(SPEND_DAY));
    assert_success(&shown);

    assert_end_row(
        &dir,
        &shown.stdout,
        "project spend",
        "$20.00 (1 unmeasurable (charged at $20.00 each))",
        "$380.00",
        "$0.00",
    );
    assert_end_row(&dir, &shown.stdout, "project invocations", "1", "199", "0");
}

/// Two tickets: the second admission already carries the first settle, so the
/// end row was one invocation short rather than empty.
// §FS-rhei-run-report.3.1 §FS-rhei-budgets.6.2
#[test]
fn a_two_ticket_runs_bounds_end_row_carries_the_second_invocation_too() {
    let (dir, plan, machine) = setup_end_row("bounds-end-two", UNMEASURABLE_TARGET, "mock", 2);

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    let shown = budget_show(&plan, Some(SPEND_DAY));
    assert_success(&shown);

    assert_end_row(
        &dir,
        &shown.stdout,
        "project spend",
        "$40.00 (2 unmeasurable (charged at $20.00 each))",
        "$360.00",
        "$0.00",
    );
    assert_end_row(&dir, &shown.stdout, "project invocations", "2", "198", "0");
}

/// The control that rules out the fallback path: a measured, priced invocation
/// settles below its reserve, so a row read at admission is wrong twice — it
/// misses the spend, and its Remaining is the bound less the reserve.
// §FS-rhei-run-report.3.1 §FS-rhei-budgets.6.2
#[test]
fn a_priced_runs_bounds_end_row_carries_the_settled_amount_and_its_remainder() {
    let (dir, plan, machine) = setup_end_row("bounds-end-priced", PRICED_TARGET, "claude-code", 1);

    assert_success(&run_plan(&plan, &machine, Some(SPEND_DAY)));
    let shown = budget_show(&plan, Some(SPEND_DAY));
    assert_success(&shown);

    assert_end_row(&dir, &shown.stdout, "project spend", PRICED_INVOCATION, "$397.00", "$0.00");
    assert_end_row(&dir, &shown.stdout, "project invocations", "1", "199", "0");
}

/// The stream says the same as the report: its last `budget_snapshot` follows
/// the run's last settle and carries the account that settle left, so a reader
/// of the tail — and the TUI and dashboard fed by the same event — sees what
/// the run spent. §FS-rhei-run-json.2.1 §FS-rhei-budgets.9
#[test]
fn the_json_streams_last_budget_snapshot_follows_the_last_settle() {
    let (_dir, plan, machine) = setup_end_row("bounds-end-json", UNMEASURABLE_TARGET, "mock", 1);

    let result =
        run_at("run", &plan, &machine, Some(SPEND_DAY), &["--no-tui", "--no-callbacks", "--json"]);
    assert_success(&result);
    let records: Vec<serde_json::Value> = result
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("every --json stdout line is a record"))
        .collect();
    let last = |kind: &str| records.iter().rposition(|r| r["event"] == kind);
    let released = last("slot_released").expect("the run released its one worker");
    let snapshot = last("budget_snapshot").expect("the run emitted a budget_snapshot");
    assert!(
        snapshot > released,
        "the last budget_snapshot (record {}) comes after the last settle, which follows \
         the last slot_released (record {}); records:\n{}",
        records[snapshot]["seq"],
        records[released]["seq"],
        result.stdout
    );

    let bound = |dimension: &str| {
        records[snapshot]["bounds"]
            .as_array()
            .and_then(|bounds| bounds.iter().find(|b| b["dimension"] == dimension))
            .unwrap_or_else(|| panic!("the last budget_snapshot has no {dimension} entry"))
            .clone()
    };
    let spend = bound("project spend");
    assert_eq!(
        (&spend["consumed"], &spend["outstanding"]),
        (&serde_json::json!(20_000_000u64), &serde_json::json!(0u64)),
        "the snapshot carries the settled $20.00 in micro-units and nothing outstanding: {spend}"
    );
    let invocations = bound("project invocations");
    assert_eq!(
        (&invocations["consumed"], &invocations["outstanding"]),
        (&serde_json::json!(1u64), &serde_json::json!(0u64)),
        "the snapshot counts the one invocation the run made: {invocations}"
    );
}
