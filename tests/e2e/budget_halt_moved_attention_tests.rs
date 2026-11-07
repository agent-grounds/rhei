//! A bound's `## Attention` row stands only while the ticket is in the state the
//! bound refused it in (agent-grounds/rhei#603).
//!
//! A fan-out at `--parallel 2` on one invocation a day: one arm runs, the bound
//! refuses the other, and the running arm then moves the ticket. The refusal was
//! real, so `## Bounds` keeps it, but it says nothing about where the ticket is now.
//! The agent holds its arm until the refusal is on record, so the order these tests
//! need comes from the run itself rather than from a sleep, and they check it.

// §FS-rhei-run-report.3.1 §FS-rhei-run-json.2.1

use std::fs;
use std::path::{Path, PathBuf};

use super::budget_halt_attention_tests::{attention_row, halt_record, report, section};
use super::budget_support::*;
use super::*;

const NOON: &str = "2026-09-24T12:00:00Z";
const PAST_RENEWAL: &str = "2026-09-25T00:00:01Z";
const RENEWS: &str = "renews automatically";

/// `parked` has no agent or program, so nothing is configured to run it.
const PARKED_MACHINE: &str = r#"name: fan-out-parked
version: 1
states:
  work:
    initial: true
    description: Work
    all_targets: ["mock:mock:alpha", "mock:mock:beta"]
  parked: { description: Nobody runs this }
  done: { description: Done, final: true }
transitions:
  - { from: work, to: parked }
  - { from: parked, to: done }
"#;

/// `review` does have an agent, so the run attempts it once the ticket arrives.
const REVIEW_MACHINE: &str = r#"name: fan-out-review
version: 1
states:
  work:
    initial: true
    description: Work
    all_targets: ["mock:mock:alpha", "mock:mock:beta"]
  review:
    description: Review
    target: "mock:mock:alpha"
  done: { description: Done, final: true }
transitions:
  - { from: work, to: review }
  - { from: review, to: done }
"#;

/// A counted self-loop: the second visit's raw plan state is `work-2`.
const COUNTED_MACHINE: &str = r#"name: counted
version: 1
states:
  work:
    initial: true
    description: Work
    agent: mock
    visits: 5
    outputs:
      - name: work
        path: runtime/work.md
  cancelled: { description: Stop, final: true }
transitions:
  - { from: work, to: work, description: Again }
  - { from: work, to: cancelled, description: Stop }
"#;

/// Holds a `work` arm until the run has recorded the refusal, then ends the visit. A
/// refusal that never comes runs the arm out at the deadline, and
/// [`assert_refused_before_the_move`] then reports it.
const HOLDS_UNTIL_REFUSED: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawn-count.log', '{} {}\n'.format(env('RHEI_TASK_ID'), env('RHEI_STATE')))
events = root / 'runtime' / 'events.jsonl'
deadline = time.time() + 20
while env('RHEI_STATE') == 'work' and time.time() < deadline:
    if events.exists() and 'budget_halt' in events.read_text(encoding='utf-8'):
        break
    time.sleep(0.02)
result('done\n')
"#;

/// A fan-out plan on `machine`, with no `defaults.agent`: a state with no target
/// of its own has nothing to run it.
fn fan_out(prefix: &str, machine: &str, per_day: u32) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN.replace("Work", "T").as_str());
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let agent = write_python_agent(&dir, "mock-agent.py", HOLDS_UNTIL_REFUSED);
    write_machine_settings(
        &dir,
        &format!(
            r#"{{ "defaults": {{ "agent_timeout": "30s", "invocations_per_day": {per_day} }},
  "agents": {{ "mock": {{ "command": {}, "stdin_prompt": true }} }} }}"#,
            fixture_command(&agent)
        ),
    );
    (dir, plan, machine_path)
}

fn run_parallel(plan: &Path, machine: &Path, now: &str) -> CliRun {
    run_at("run", plan, machine, Some(now), &["--no-tui", "--no-callbacks", "--parallel", "2"])
}

fn events(root: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(root.join("runtime/events.jsonl"))
        .expect("read the run's event stream")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON event"))
        .collect()
}

/// The premise of every test here: the ticket was refused while its other arm was
/// running, so that arm's move came after the refusal and no later slot followed.
fn assert_refused_before_the_move(root: &Path) {
    let events = events(root);
    let position = |kind: &str| events.iter().position(|event| event["event"] == kind);
    let halt = position("budget_halt");
    let release = position("slot_released");
    assert!(
        halt.is_some() && release.is_some() && halt < release,
        "premise: the bound refuses one arm while the other runs, got halt at {halt:?} and \
         release at {release:?}:\n{events:#?}"
    );
    assert_eq!(events[halt.unwrap()]["task"], "plan.1", "the bound refused the ticket");
    let slots = events.iter().filter(|event| event["event"] == "slot_assigned").count();
    assert_eq!(slots, 1, "no later slot takes the halt back:\n{events:#?}");
}

fn state_of_plan(plan: &Path) -> String {
    let text = fs::read_to_string(plan).expect("read the plan");
    let line = text.lines().find(|line| line.starts_with("**State:**")).expect("a state line");
    line.trim_start_matches("**State:**").trim().to_owned()
}

fn assert_bounds_keeps_the_halt(root: &Path) {
    let report = report(root);
    let halt = "- `plan.1` was stopped by its project invocations bound";
    assert!(section(&report, "Bounds").contains(halt), "## Bounds lists the halt:\n{report}");
}

/// The issue's case. Its row promised a resumption, and the renewal run below shows
/// that none comes. §FS-rhei-run-report.3.1
#[test]
fn a_ticket_moved_into_a_state_with_no_agent_gets_the_classifiers_row_not_the_bounds() {
    let (dir, plan, machine) = fan_out("budget-moved-parked", PARKED_MACHINE, 1);

    let result = run_parallel(&plan, &machine, NOON);

    assert!(!result.status.success(), "the ticket cannot finish: {}", result.stderr);
    assert_spawn_count(&dir, 1, "the day's allowance buys one arm");
    assert_refused_before_the_move(&dir);
    assert_eq!(state_of_plan(&plan), "parked");
    assert_bounds_keeps_the_halt(&dir);
    let (blocker, next) = attention_row(&dir, "plan.1");
    assert_ne!(next, RENEWS, "no renewal resumes a ticket in `parked`: {blocker:?}");
    assert!(!blocker.contains("project invocations"), "the bound no longer holds it: {blocker:?}");
    let reading = format!("{blocker} | {next}");
    let classifier =
        ["stalled in non-terminal state parked", "not scheduled before the run halted"];
    assert!(
        classifier.iter().any(|words| reading.contains(words)),
        "the classifier's reading of the ticket: {reading:?}"
    );
    assert_eq!(halt_record(&dir)["state"], "work", "the state the refused arm was for");

    run_parallel(&plan, &machine, PAST_RENEWAL);
    assert_spawn_count(&dir, 1, "the renewal resumed nothing, as the row must not promise");
    assert_eq!(state_of_plan(&plan), "parked");
}

/// The same fan-out and move into a state that has an agent. The run does attempt
/// that state, and the bound refuses the ticket there too, so the latest halt's
/// state is the one the ticket is in. The row is true this time, and the renewal
/// does start the state, which is what the row promises. The row alone already reads
/// this way today; what is red is the record that lets the report tell the two apart.
/// §FS-rhei-run-report.3.1 §FS-rhei-run-json.2.1
#[test]
fn a_ticket_moved_then_refused_again_in_its_new_state_keeps_the_bounds_row() {
    let (dir, plan, machine) = fan_out("budget-moved-refused-again", REVIEW_MACHINE, 1);

    let result = run_parallel(&plan, &machine, NOON);

    assert!(!result.status.success(), "the bound holds the ticket: {}", result.stderr);
    assert_spawn_count(&dir, 1, "the day's allowance buys one arm");
    assert_refused_before_the_move(&dir);
    assert_eq!(state_of_plan(&plan), "review");
    let states: Vec<_> = events(&dir)
        .into_iter()
        .filter(|event| event["event"] == "budget_halt")
        .map(|event| event["state"].clone())
        .collect();
    assert_eq!(states, ["work", "review"], "one refusal in each state, in order");
    let (blocker, next) = attention_row(&dir, "plan.1");
    assert!(blocker.contains("project invocations"), "the bound holds it: {blocker:?}");
    assert_eq!(next, RENEWS);

    run_parallel(&plan, &machine, PAST_RENEWAL);
    assert_eq!(spawns(&dir), ["plan.1 work", "plan.1 review"], "the renewal starts the state");
}

/// The record itself, on the stream a reader routes on and on the durable log, from
/// the serial refusal site. The refused visit is a counted one, and the record names
/// the state, not the plan's raw `work-2`.
/// §FS-rhei-run-json.2.1
#[test]
fn a_budget_halt_names_the_state_the_refused_spawn_was_for() {
    let (dir, plan, machine) =
        setup("budget-halt-state", COUNTED_MACHINE, r#", "invocations_per_day": 1"#);

    let args = ["--no-tui", "--no-callbacks", "--json"];
    let result = run_at("run", &plan, &machine, Some(NOON), &args);

    assert_spawn_count(&dir, 1, "the day's allowance buys one invocation");
    assert_eq!(state_of_plan(&plan), "work-2", "premise: the counted visit is the one refused");
    let streamed = result
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON record"))
        .find(|record| record["event"] == "budget_halt")
        .expect("the stream carries the halt");
    assert_eq!(streamed["state"], "work", "--json: {streamed:#}");
    assert_eq!(halt_record(&dir)["state"], "work", "events.jsonl");
    let (_, next) = attention_row(&dir, "plan.1");
    assert_eq!(next, RENEWS, "a counted state is the state the bound refused it in");
}

/// The issue's control: a ticket the bound still holds keeps the row, so the fix
/// cannot simply drop it. Green today, and it has to stay so.
#[test]
fn a_ticket_refused_where_it_stands_keeps_the_bounds_row() {
    let (dir, plan, machine) = setup_with_agent(
        "budget-moved-held",
        FINISHING_MACHINE,
        FINISHING_AGENT,
        r#", "invocations_per_day": 1"#,
    );
    let two_tasks = "# Rhei: H\n\n## Tasks\n\n### Task 1: A\n**State:** work\n\n\
                     ### Task 2: B\n**State:** work\n";
    write_fixture_file(&dir, "plan.rhei.md", two_tasks);

    let result = run_plan(&plan, &machine, Some(NOON));

    assert!(!result.status.success(), "plan.2 is halted: {}", result.stderr);
    assert_spawn_count(&dir, 1, "the day's allowance buys one invocation");
    assert_eq!(halt_record(&dir)["task"], "plan.2");
    let (blocker, next) = attention_row(&dir, "plan.2");
    assert!(blocker.contains("project invocations"), "the bound holds it: {blocker:?}");
    assert_eq!(next, RENEWS);
}
