//! A ticket halted by a spent bound gets a row of its own in `## Attention`.
//!
//! Its blocker names the dimension and the effective bound with its source. Its
//! next action is the one remedy that raises the limiter, or `renews automatically`
//! where a window limited it. The generic stall row told the reader to inspect logs
//! that do not explain the stop and to cancel a ticket that needed nothing but the
//! next day (agent-grounds/rhei#587).
//!
//! The next action is compared with the run's own `budget_halt` record, because the
//! two surfaces must not disagree: the row reads the record's remedy rather than
//! resolving one of its own. Which remedy is right belongs to the resolver, so the
//! tests here pin only as much of the text as says the row carries the limiter's.

// §FS-rhei-run-report.3.1 §FS-rhei-budgets.8 §FS-rhei-budgets.3.4 §FS-rhei-run.3.4

use std::fs;
use std::path::Path;
use std::process::Command;

use super::budget_support::*;
use super::*;

const NOON: &str = "2026-09-24T12:00:00Z";
const RENEWAL: &str = "2026-09-25T00:00:00Z";

fn report(root: &Path) -> String {
    fs::read_to_string(root.join("runtime/run-report.md")).expect("read the run report")
}

/// The body of the report's `## <heading>` section, empty where it has none.
fn section(report: &str, heading: &str) -> String {
    let marker = format!("## {heading}\n");
    let Some(start) = report.find(&marker) else { return String::new() };
    let body = &report[start + marker.len()..];
    body.split("\n## ").next().unwrap_or_default().to_owned()
}

/// The blocker and the next action of the ticket's `## Attention` row.
fn attention_row(root: &Path, task: &str) -> (String, String) {
    let report = report(root);
    let attention = section(&report, "Attention");
    let row = attention
        .lines()
        .find(|line| line.starts_with(&format!("| {task} |")))
        .unwrap_or_else(|| panic!("no {task} row in ## Attention:\n{report}"));
    let cells: Vec<&str> = row.trim().trim_matches('|').split(" | ").map(str::trim).collect();
    assert_eq!(cells.len(), 4, "Task, State, Reason, Next action: {row}");
    (cells[2].to_owned(), cells[3].to_owned())
}

/// The run's last `budget_halt` record: the premise that a bound stopped it, and
/// the remedy the row has to agree with.
fn halt_record(root: &Path) -> serde_json::Value {
    fs::read_to_string(root.join("runtime/events.jsonl"))
        .expect("read the run's event stream")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON event"))
        .rfind(|event| event["event"] == "budget_halt")
        .expect("a run stopped by a bound records budget_halt")
}

/// What every bound kind owes: stopped by this dimension, in Attention and not
/// Waiting, no stall advice anywhere in the report, and `## Bounds` still listing it.
fn assert_owed_by_every_kind(root: &Path, dimension: &str, result: &CliRun) {
    assert!(!result.status.success(), "a run ending with a halted ticket exits non-zero");
    assert_eq!(halt_record(root)["dimension"], dimension, "the run stopped on another bound");

    let report = report(root);
    let (blocker, _) = attention_row(root, "plan.1");
    assert!(blocker.contains(dimension), "the blocker names {dimension:?}: {blocker:?}");
    assert!(
        !section(&report, "Waiting").contains("plan.1"),
        "a bound-halted ticket is in Attention, not Waiting:\n{report}"
    );
    for stall in ["stalled in non-terminal state", "inspect logs", "mark the task cancelled"] {
        assert!(!report.contains(stall), "the report still gives {stall:?}:\n{report}");
    }
    let halt = format!("- `plan.1` was stopped by its {dimension} bound");
    assert!(section(&report, "Bounds").contains(&halt), "## Bounds lists the halt:\n{report}");
}

/// A window-limited row: the renewal instant is the blocker's deadline, and the
/// ticket needs nothing from anyone.
fn assert_renews_by_itself(root: &Path, bound: &str) {
    let (blocker, next) = attention_row(root, "plan.1");
    assert!(blocker.contains(bound), "the blocker names the bound {bound:?}: {blocker:?}");
    assert!(blocker.contains(RENEWAL), "the blocker names when it renews: {blocker:?}");
    assert_eq!(next, "renews automatically");
}

/// `rhei budget init`, which [`run_at`] cannot express: `budget` takes a subcommand
/// where that helper puts the plan.
fn budget_init_invocations(plan: &Path, invocations: &str) {
    let root = plan.parent().expect("plan has a parent");
    let mut cmd: Command = rhei_command(home_for(root));
    cmd.arg("budget").arg("init").arg(plan);
    cmd.arg("--invocations").arg(invocations).arg("--reason").arg("a lifetime allowance");
    cmd.env("RHEI_BUDGET_NOW", NOON);
    assert_success(&CliRun::from(&cmd.output().expect("rhei budget init should run")));
}

/// A count: the settings key is the one remedy, and the row carries the record's.
#[test]
fn a_travel_halt_names_the_bound_and_the_settings_remedy() {
    let (dir, plan, machine) =
        setup("budget-attention-travel", PING_PONG_MACHINE, r#", "transition_limit": 4"#);

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_spawn_count(&dir, 4, "four travel units buys four moves");
    assert_owed_by_every_kind(&dir, "ticket travel", &result);
    let (blocker, next) = attention_row(&dir, "plan.1");
    assert!(blocker.contains("4 (machine)"), "the bound and its source: {blocker:?}");
    assert!(next.contains("defaults.transition_limit"), "the limiter's key: {next:?}");
    assert_eq!(next, halt_record(&dir)["remedy"].as_str().expect("a remedy"));
}

/// A count too, so the next action is the record's remedy: the audited command, as
/// the account's allowance is what limits. §FS-rhei-budgets.8
#[test]
fn a_lifetime_halt_names_the_bound_and_the_remedy_of_its_record() {
    let (dir, plan, machine) = setup("budget-attention-lifetime", PING_PONG_MACHINE, "");
    budget_init_invocations(&plan, "2");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_spawn_count(&dir, 2, "the allowance buys two invocations");
    assert_owed_by_every_kind(&dir, "project invocations", &result);
    let record = halt_record(&dir);
    assert_eq!(record["mode"], "lifetime", "the allowance limited, not a window");
    let (blocker, next) = attention_row(&dir, "plan.1");
    assert!(blocker.contains("2 (account)"), "the bound and its source: {blocker:?}");
    assert!(next.contains("rhei budget adjust"), "the audited command: {next:?}");
    assert_eq!(next, record["remedy"].as_str().expect("a remedy"));
}

/// The ticket needs only the next day, and following the stall row's advice to
/// cancel it would have ended it.
#[test]
fn a_window_halt_names_the_renewal_and_renews_automatically() {
    let (dir, plan, machine) =
        setup("budget-attention-window", PING_PONG_MACHINE, r#", "invocations_per_day": 2"#);

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_spawn_count(&dir, 2, "the day's allowance buys two invocations");
    assert_owed_by_every_kind(&dir, "project invocations", &result);
    assert_renews_by_itself(&dir, "2 (machine)");
}

/// Spend is bounded by a window always, so its row always renews. Twenty
/// unmeasurable invocations at the $20.00 reserve reach the built-in $400.00.
// §FS-rhei-budgets.3.4
#[test]
fn a_spend_halt_names_the_renewal_and_renews_automatically() {
    let (dir, plan, machine) = setup("budget-attention-spend", PING_PONG_MACHINE, "");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_spawn_count(&dir, 20, "twenty reserves of $20.00 reach $400.00");
    assert_owed_by_every_kind(&dir, "project spend", &result);
    assert_renews_by_itself(&dir, "$400.00 (built_in)");
}

/// The same bound met by the next run: the run spawns nothing, so the row read
/// `not scheduled … rerun to pick it up`, the one remedy that provably changes
/// nothing, and the Transition Ledger and Task Final States repeated its reason.
#[test]
fn a_run_that_starts_on_a_spent_bound_names_it_rather_than_never_scheduled() {
    for (prefix, extra, spawned, dimension, next) in [
        (
            "budget-attention-again-window",
            r#", "invocations_per_day": 2"#,
            2,
            "project invocations",
            Some("renews automatically"),
        ),
        ("budget-attention-again-travel", r#", "transition_limit": 4"#, 4, "ticket travel", None),
    ] {
        let (dir, plan, machine) = setup(prefix, PING_PONG_MACHINE, extra);
        run_plan(&plan, &machine, Some(NOON));
        assert_spawn_count(&dir, spawned, "the first run spends the bound");

        let again = run_plan(&plan, &machine, Some(NOON));

        assert_spawn_count(&dir, spawned, "the second run spends nothing");
        assert!(!again.status.success(), "the ticket is still halted");
        assert_eq!(halt_record(&dir)["dimension"], dimension, "the same bound refused it");

        let report = report(&dir);
        for wrong in ["not scheduled before", "rerun to pick it up"] {
            assert!(!report.contains(wrong), "the report still gives {wrong:?}:\n{report}");
        }
        let (blocker, row_next) = attention_row(&dir, "plan.1");
        assert!(blocker.contains(dimension), "the blocker names {dimension:?}: {blocker:?}");
        let remedy = halt_record(&dir)["remedy"].as_str().expect("a remedy").to_owned();
        assert_eq!(row_next, next.map_or(remedy, str::to_owned));
        let halt = format!("- `plan.1` was stopped by its {dimension} bound");
        assert!(section(&report, "Bounds").contains(&halt), "## Bounds lists the halt:\n{report}");
        let ledger = section(&report, "Transition Ledger");
        let blocked = ledger.lines().find(|line| line.contains("| blocked |"));
        assert!(
            blocked.is_some_and(|line| line.contains(dimension)),
            "the ledger explains it the way Attention does:\n{report}"
        );
    }
}
