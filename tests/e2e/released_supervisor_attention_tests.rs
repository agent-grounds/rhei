//! A released supervisor whose child stopped is a parent waiting on its subtree.
//!
//! It has no stop of its own and its chain resumes by itself, so a row telling the
//! reader to inspect logs or cancel it throws away a healthy chain, and counting it
//! inflates `could not advance` by one (agent-grounds/rhei#596). The same two stops
//! under an ordinary parent are the control for what that reading looks like.
//!
//! The guards are the supervisors that do have a stop of their own and keep a row.
//! Three more live beside what they are about: stranded in
//! `reset_remedy_messages_tests.rs`, refused its own wake-up in
//! `budget_halt_attention_tests.rs`, and parked at a gate in
//! `supervision_barrier_tests.rs`.

// §FS-rhei-run-report.3.1 §FS-rhei-run-report.3.2 §FS-rhei-run-report.2 §FS-rhei-run-report.4
// §FS-rhei-supervision.3.1

use std::fs;
use std::path::{Path, PathBuf};

use super::budget_halt_attention_tests::{report, section, SUPERVISED_PLAN};
use super::budget_support::*;
use super::*;

const NOON: &str = "2026-09-24T12:00:00Z";

/// `invocations_per_day` 3 goes on the supervisor, its first child and its second
/// visit, so the window refuses the second child and nothing else.
const THREE_A_DAY: &str = r#", "invocations_per_day": 3"#;

/// The ordinary parent spawns nothing, so one invocation reaches the same refusal.
const ONE_A_DAY: &str = r#", "invocations_per_day": 1"#;

/// The filed reproducer's machine: `work` owes a file, and `parent` is the control.
const MACHINE: &str = r#"name: released-supervisor
version: 1
states:
  supervising:
    initial: true
    execute_on: child-terminal
    description: Supervise
    agent: mock
    visits: 12
  parent:
    description: An ordinary parent, the control
    agent: mock
  work:
    description: Work
    agent: mock
    outputs: [{ name: work, path: "runtime/{task_id}.work.md" }]
  completed: { description: Done, final: true }
  cancelled: { description: Stop, final: true }
transitions:
  - { from: supervising, to: completed, condition: 'openDescendants < 1', description: Done }
  - { from: supervising, to: supervising, description: Release }
  - { from: supervising, to: cancelled, description: Stop }
  - { from: parent, to: completed, description: Done }
  - { from: parent, to: cancelled, description: Stop }
  - { from: work, to: completed, description: Done }
  - { from: work, to: cancelled, description: Stop }
"#;

/// Writes what its state owes. A spawn named in `{SILENT}` writes nothing and one
/// named in `{FAILING}` exits 3, each as `<task>` or `<task>@<visit>`.
const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
names = (task, '{}@{}'.format(task, env('RHEI_VISIT_COUNT')))
append(root / 'runtime' / 'spawn-count.log', '{} {}\n'.format(task, state))
if any(name in '{FAILING}'.split() for name in names):
    raise SystemExit(3)
if not any(name in '{SILENT}'.split() for name in names):
    if state == 'work':
        write(root / 'runtime' / '{}.work.md'.format(task), 'done\n')
    result('done\n')
"#;

const ORDINARY_PLAN: &str = r#"# Rhei: Chain

## Tasks

### Task 1: Parent
**State:** parent

#### Task 1.1: First
**State:** work

#### Task 1.2: Second
**State:** work
**Prior:** 1.1

#### Task 1.3: Third
**State:** work
**Prior:** 1.2
"#;

/// A released supervisor whose worker still holds its visit, over a live child.
const CLAIMED_RELEASED_PLAN: &str = r#"# Rhei: Claimed

---
metadata:
  tasks:
    1:
      stateVisits:
        supervising: 1
      supervision:
        phase: released
---

## Tasks

### Task 1: Parent
**State:** supervising
**Assignee:** alice

#### Task 1.1: First
**State:** work
"#;

fn workspace(
    prefix: &str,
    tasks: &str,
    silent: &str,
    failing: &str,
    extra: &str,
) -> (TestDir, PathBuf, PathBuf) {
    let agent = AGENT.replace("{SILENT}", silent).replace("{FAILING}", failing);
    let (dir, plan, machine) = setup_with_agent(prefix, MACHINE, &agent, extra);
    write_fixture_file(&dir, "plan.rhei.md", tasks);
    (dir, plan, machine)
}

/// Every ticket the run's `budget_halt` events name, each once.
fn refused(root: &Path) -> Vec<String> {
    let events = fs::read_to_string(root.join("runtime/events.jsonl")).unwrap_or_default();
    let mut tasks: Vec<String> = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON event"))
        .filter(|event| event["event"] == "budget_halt")
        .map(|event| event["task"].as_str().expect("a task").to_owned())
        .collect();
    tasks.sort();
    tasks.dedup();
    tasks
}

fn attention_row(report: &str, task: &str) -> Option<String> {
    let attention = section(report, "Attention");
    attention.lines().find(|line| line.starts_with(&format!("| {task} |"))).map(str::to_owned)
}

fn tree_line(report: &str, task: &str) -> String {
    let tree = section(report, "Task Final States");
    let tick = format!("`{task}`");
    tree.lines().find(|line| line.contains(&tick)).unwrap_or_default().trim().to_owned()
}

fn could_not_advance(report: &str) -> String {
    let row = report.lines().find(|line| line.starts_with("| could not advance |"));
    row.unwrap_or("no `could not advance` row").to_owned()
}

/// The situation under test, so a setup that went wrong cannot read as the defect.
fn assert_released_with_a_live_subtree(plan: &Path, result: &CliRun) {
    let plan = fs::read_to_string(plan).expect("read the plan");
    assert!(plan.contains("phase: released"), "plan.1 is recorded as released:\n{plan}");
    assert_eq!(result.status.code(), Some(1), "the exit code is not what this change moves");
}

/// Everything §FS-rhei-run-report.3.1 says of a parent held open only by its subtree,
/// collected so one failure shows every surface still reading it the wrong way.
fn assert_reads_as_waiting_on_its_subtree(root: &Path) {
    let report = report(root);
    let mut wrong = Vec::new();
    if let Some(row) = attention_row(&report, "plan.1") {
        wrong.push(format!("plan.1 has an Attention row: {row}"));
    }
    if section(&report, "Waiting").lines().any(|line| line.starts_with("| plan.1 |")) {
        wrong.push("plan.1 has a Waiting row".to_owned());
    }
    let count = could_not_advance(&report);
    if count != "| could not advance | 2 |" {
        wrong.push(format!("expected `could not advance` of 2: {count}"));
    }
    let ledger = section(&report, "Transition Ledger");
    let blocked = |line: &&str| line.starts_with("| plan.1 |") && line.contains("| blocked |");
    if let Some(row) = ledger.lines().find(blocked) {
        wrong.push(format!("plan.1 has a blocked ledger row: {row}"));
    }
    let tree = tree_line(&report, "plan.1");
    if !tree.contains('⏸') || !tree.contains("waiting on open descendant Task plan.1.2") {
        wrong.push(format!("plan.1 does not read as waiting in the tree: {tree}"));
    }
    assert!(wrong.is_empty(), "{}\n\n{report}", wrong.join("\n"));
}

/// The child's row is about what stopped: the bound, and that it renews by itself.
fn assert_the_child_names_the_bound(root: &Path) {
    let report = report(root);
    let row = attention_row(&report, "plan.1.2").expect("the refused child keeps its row");
    assert!(row.contains("project invocations bound of 3 (machine)"), "{row}");
    assert!(row.contains("renews automatically"), "{row}");
}

#[test]
fn a_released_supervisor_over_a_child_a_bound_refused_waits_on_its_subtree() {
    let (dir, plan, machine) =
        workspace("released-supervisor-bound", SUPERVISED_PLAN, "", "", THREE_A_DAY);

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_eq!(spawns(&dir), ["plan.1 supervising", "plan.1.1 work", "plan.1 supervising"]);
    assert_eq!(refused(&dir), ["plan.1.2"], "the bound refused the child alone");
    assert_released_with_a_live_subtree(&plan, &result);
    assert_the_child_names_the_bound(&dir);
    assert_reads_as_waiting_on_its_subtree(&dir);
}

/// The same command again on the spent window. Nothing ran, so the row read `not
/// scheduled … rerun to pick it up` where the first run's read `stalled`.
#[test]
fn a_released_supervisor_rerun_on_the_spent_window_waits_on_its_subtree() {
    let (dir, plan, machine) =
        workspace("released-supervisor-rerun", SUPERVISED_PLAN, "", "", THREE_A_DAY);
    run_plan(&plan, &machine, Some(NOON));
    assert_spawn_count(&dir, 3, "the first run spends the window");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_spawn_count(&dir, 3, "the second run spends nothing");
    assert_eq!(refused(&dir), ["plan.1.2"], "the bound refused the child alone");
    assert_released_with_a_live_subtree(&plan, &result);
    assert_the_child_names_the_bound(&dir);
    assert_reads_as_waiting_on_its_subtree(&dir);
}

/// No bound anywhere: what misreads the supervisor is the subtree, not the bound.
#[test]
fn a_released_supervisor_over_a_child_that_wrote_nothing_waits_on_its_subtree() {
    let (dir, plan, machine) =
        workspace("released-supervisor-missing", SUPERVISED_PLAN, "plan.1.2", "", "");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert!(spawns(&dir).contains(&"plan.1.2 work".to_owned()), "plan.1.2 ran");
    assert!(!dir.join("runtime/plan.1.2.work.md").exists(), "and wrote nothing");
    assert!(refused(&dir).is_empty(), "no bound stopped this run");
    assert_released_with_a_live_subtree(&plan, &result);
    let report = report(&dir);
    let child = attention_row(&report, "plan.1.2").expect("the silent child keeps its row");
    assert!(child.contains("worker exited 0 without work"), "{child}");
    assert_reads_as_waiting_on_its_subtree(&dir);
}

/// What "waiting on its subtree" means, from the parent that has always read so.
#[test]
fn an_ordinary_parent_over_the_same_stops_reads_as_waiting_on_its_subtree() {
    for (prefix, silent, extra) in [
        ("released-supervisor-control-bound", "", ONE_A_DAY),
        ("released-supervisor-control-missing", "plan.1.2", ""),
    ] {
        let (dir, plan, machine) = workspace(prefix, ORDINARY_PLAN, silent, "", extra);

        let result = run_plan(&plan, &machine, Some(NOON));

        assert_eq!(result.status.code(), Some(1), "{prefix}");
        let report = report(&dir);
        assert_eq!(attention_row(&report, "plan.1"), None, "{report}");
        assert_eq!(could_not_advance(&report), "| could not advance | 2 |", "{report}");
        let tree = tree_line(&report, "plan.1");
        assert!(tree.contains('⏸'), "{tree}");
        assert!(tree.contains("waiting on open descendant Task plan.1.2"), "{tree}");
    }
}

/// A supervisor owed a visit is the ready ticket. Its descendants read `held by
/// supervisor` back at it, so a wait of its own would leave Attention empty.
fn assert_keeps_its_row_over_its_held_descendants(root: &Path, plan: &Path, phase: Option<&str>) {
    let report = report(root);
    let row = attention_row(&report, "plan.1");
    assert!(row.is_some(), "plan.1 keeps its row:\n{report}");
    let waiting = section(&report, "Waiting");
    let held = |task: &str| {
        waiting.lines().any(|line| {
            line.starts_with(&format!("| {task} |"))
                && line.contains("held by supervisor Task plan.1")
        })
    };
    assert!(held("plan.1.2"), "plan.1.2 is held by it:\n{report}");
    let plan = fs::read_to_string(plan).expect("read the plan");
    match phase {
        Some(phase) => assert!(plan.contains(phase), "expected {phase:?}:\n{plan}"),
        None => assert!(!plan.contains("supervision:"), "no recorded phase:\n{plan}"),
    }
}

/// The checkpoint held the subtree again and the visit it earned failed.
// §FS-rhei-supervision.3.5
#[test]
fn a_held_supervisor_whose_visit_failed_keeps_its_row() {
    let (dir, plan, machine) =
        workspace("released-supervisor-held", SUPERVISED_PLAN, "", "plan.1@2", "");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert!(!result.status.success(), "{}", result.stderr);
    assert_eq!(spawns(&dir), ["plan.1 supervising", "plan.1.1 work", "plan.1 supervising"]);
    assert_keeps_its_row_over_its_held_descendants(&dir, &plan, Some("phase: held"));
}

/// The first visit failed, so no block was ever recorded.
// §FS-rhei-supervision.3.5
#[test]
fn a_supervisor_with_no_recorded_phase_whose_visit_failed_keeps_its_row() {
    let (dir, plan, machine) =
        workspace("released-supervisor-no-phase", SUPERVISED_PLAN, "", "plan.1", "");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert!(!result.status.success(), "{}", result.stderr);
    assert_eq!(spawns(&dir), ["plan.1 supervising"]);
    assert_keeps_its_row_over_its_held_descendants(&dir, &plan, None);
}

/// The claim rung comes before the subtree: a claim holds the descendants whatever
/// phase is recorded, so only `rhei release` starts anything here, and the row is the
/// only thing left in Attention.
// §FS-rhei-supervision.3.4
#[test]
fn a_released_supervisor_with_a_claim_keeps_the_claims_row() {
    let (dir, plan, machine) =
        workspace("released-supervisor-claimed", CLAIMED_RELEASED_PLAN, "", "", "");

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_released_with_a_live_subtree(&plan, &result);
    assert_spawn_count(&dir, 0, "nothing is schedulable under the claim");
    let report = report(&dir);
    let row = attention_row(&report, "plan.1").unwrap_or_default();
    assert!(row.contains("claimed by alice"), "{report}");
    assert!(row.contains("rhei release"), "{report}");
    assert_eq!(could_not_advance(&report), "| could not advance | 1 |", "{report}");
    assert!(section(&report, "Waiting").contains("| plan.1.1 |"), "{report}");
}
