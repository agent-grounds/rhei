//! One ticket stopped by one bound is one line under `## Bounds`.
//!
//! A run retries a stalled ticket after a pass that moved something, so a spent
//! window refuses the same ticket on two passes. Both refusals are real and both
//! stay in the event stream, but they are one stop, and two identical lines read as
//! two stops or two tickets (agent-grounds/rhei#605). The halt's other surfaces are
//! in `budget_halt_attention_tests.rs`, and the halts that must stay apart are pinned
//! beside the report's unit tests, because no real run has built such a pair.

// §FS-rhei-run-report.3.1 §FS-rhei-run.3

use std::fs;
use std::path::Path;

use super::budget_halt_attention_tests::{report, section};
use super::budget_support::*;
use super::*;

const NOON: &str = "2026-09-24T12:00:00Z";

/// An ordinary parent spawns nothing, so the chain's first child spends the day's
/// one invocation and the window refuses its second.
const CHAIN: &str = r#"# Rhei: Chain

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

const CHAIN_MACHINE: &str = r#"name: budget-chain
version: 1
states:
  parent:
    initial: true
    description: An ordinary parent
    agent: mock
  work:
    description: Work
    agent: mock
    outputs: [{ name: work, path: "runtime/{task_id}.work.md" }]
  completed: { description: Done, final: true }
  cancelled: { description: Stop, final: true }
transitions:
  - { from: parent, to: completed, description: Done }
  - { from: parent, to: cancelled, description: Stop }
  - { from: work, to: completed, description: Done }
  - { from: work, to: cancelled, description: Stop }
"#;

const CHAIN_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
append(root / 'runtime' / 'spawn-count.log', '{} {}\n'.format(task, env('RHEI_STATE')))
write(root / 'runtime' / '{}.work.md'.format(task), 'done\n')
result('done\n')
"#;

/// The ticket each `budget_halt` event names, one entry per event.
fn refusals(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("runtime/events.jsonl"))
        .expect("read the run's event stream")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON event"))
        .filter(|event| event["event"] == "budget_halt")
        .map(|event| event["task"].as_str().expect("a task").to_owned())
        .collect()
}

/// The whole line and the two events: one line would pass a `contains` as two would,
/// and a fix that silenced the events or dropped the retry would pass without the second.
#[test]
fn a_ticket_one_bound_refused_on_two_passes_is_one_line_under_bounds() {
    let (dir, plan, machine) = setup_with_agent(
        "budget-bounds-one-line",
        CHAIN_MACHINE,
        CHAIN_AGENT,
        r#", "invocations_per_day": 1"#,
    );
    write_fixture_file(&dir, "plan.rhei.md", CHAIN);

    let result = run_plan(&plan, &machine, Some(NOON));

    assert_eq!(result.status.code(), Some(1), "a halted ticket exits 1: {}", result.stderr);
    let moved = format!("a pass moved something, then the window shut: {}", result.stderr);
    assert_eq!(spawns(&dir), ["plan.1.1 work"], "{moved}");
    assert_eq!(refusals(&dir), ["plan.1.2", "plan.1.2"], "every refusal stays an event");

    let report = report(&dir);
    let bounds = section(&report, "Bounds");
    let halts: Vec<&str> = bounds.lines().filter(|line| line.contains("was stopped by")).collect();
    assert_eq!(
        halts,
        ["- `plan.1.2` was stopped by its project invocations bound of 1 (machine); \
             it renews at 2026-09-25T00:00:00Z"],
        "one stop is one line:\n{report}"
    );
}
