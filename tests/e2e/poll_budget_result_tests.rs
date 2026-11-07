//! What a ticket's result says when a poll state's attempts run out. The spent
//! budget selected the edge and the last exit chose nothing, so the engine's
//! entry names the budget as `used/limit attempts` and then that exit, which may
//! still be a crash. Beside those cases sit the outcomes the budget did not end,
//! which keep the account they have.
//!
//! The figures are asserted as §FS-rhei-errors.1.5 words them. The rest of the
//! sentence is free to change, which is why nothing here compares a whole line.

// §FS-rhei-run.5.1 §FS-rhei-errors.1.5 §FS-rhei-states.3.3 §FS-rhei-run.3

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::worker_edit_revert_race_tests::{poll_machine, runs, workspace, POLL_CULPRIT};
use super::worker_edit_revert_support::state_in;
use super::*;

const POLL_STATE: &str = "awaiting-checks";

const PLAN: &str =
    "# Rhei: Poll\n\n## Tasks\n\n### Task 1: Wait for the checks\n**State:** awaiting-checks\n";

/// The answer "ask again", on the edge the issue's program takes it by.
const SELF_LOOP_ON_75: &str =
    "  - { from: awaiting-checks, to: awaiting-checks, exit_code: 75, description: Still running }";

/// The self-loop that asks nothing of the exit, the only kind an agent may have.
const SELF_LOOP_WHILE_BUDGET_LASTS: &str = "  - { from: awaiting-checks, to: awaiting-checks, \
     condition: pollAttempts < pollMaxAttempts, description: Still running }";

const FINISHED_ON_0: &str =
    "  - { from: awaiting-checks, to: completed, exit_code: 0, description: Finished }";

const BUDGET_EDGE: &str = "  - { from: awaiting-checks, to: completed, \
     condition: pollAttempts >= pollMaxAttempts, description: The poll budget ran out }";

const CATCH_ALL: &str =
    "  - { from: awaiting-checks, to: failed, exit_code: nonzero, description: Died }";

/// A machine around the poll state. `worker` is the keys that run something in
/// it. `checked` and `failed` are the non-terminal and terminal places a case may
/// route to besides `completed`.
fn poll_budget_machine(worker: &str, max_attempts: u32, edges: &[&str]) -> String {
    let edges = edges.join("\n");
    format!(
        r#"name: poll-budget-result
version: 1
states:
  awaiting-checks:
    initial: true
    description: Ask whether the checks are done
    {worker}
    poll: {{ interval: 0s, max_attempts: {max_attempts} }}
  checked: {{ gating: true, description: Routed here by the exit }}
  failed: {{ final: true, description: The attempt failed }}
  completed: {{ final: true, description: Done }}
transitions:
{edges}
  - {{ from: checked, to: completed, description: Looked at }}
"#
    )
}

/// `body` runs with `attempt` set to this attempt's 1-based number. A counter
/// beside the script survives the attempts, so one program can answer 75 and
/// then crash, and the count says how many attempts the run really spent.
fn counted_script(dir: &Path, body: &str) -> PathBuf {
    write_python_agent(
        dir,
        "worker.py",
        &format!(
            "attempts = pathlib.Path(__file__).parent / 'attempts.txt'\n\
             attempt = len(attempts.read_text().splitlines()) + 1 if attempts.exists() else 1\n\
             append(attempts, 'ran\\n')\n\
             {body}"
        ),
    )
}

fn attempts_spent(dir: &Path) -> usize {
    fs::read_to_string(dir.join("attempts.txt")).map(|text| text.lines().count()).unwrap_or(0)
}

fn program_worker(script: &Path, timeout: &str) -> String {
    format!("program:\n      command: {}\n    program_timeout: {timeout}", fixture_command(script))
}

/// Stand up a program poll in a single plan file and run it once.
fn run_program_poll(
    prefix: &str,
    body: &str,
    max_attempts: u32,
    edges: &[&str],
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let script = counted_script(&dir, body);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &poll_budget_machine(&program_worker(&script, "30s"), max_attempts, edges),
    );
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
    (dir, plan, machine)
}

/// `mock` resolved to a script. The settings home is resolved from the plan's
/// own directory, so it differs between a single file and a workspace.
fn write_mock_agent(dir: &Path, settings_root: &Path, body: &str) {
    let command = fixture_command(&counted_script(dir, body));
    let settings_dir = settings_root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let settings = format!(
        r#"{{ "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{ "mock": {{ "command": {command}, "timeout": "30s" }} }} }}"#
    );
    fs::write(settings_dir.join("settings.json"), settings).expect("write settings");
}

fn recorded_result(root: &Path, task_id: &str) -> String {
    fs::read_to_string(root.join(format!("runtime/results/{task_id}.md"))).unwrap_or_default()
}

/// The engine's entry in a result, which must be the only one it wrote.
fn engine_entry(recorded: &str) -> &str {
    let entries: Vec<&str> =
        recorded.lines().filter(|line| line.starts_with("`rhei run`:")).collect();
    assert_eq!(entries.len(), 1, "the engine owes one entry; the result was:\n{recorded}");
    entries[0]
}

fn assert_names_the_budget_and_the_exit(recorded: &str, used: u32, limit: u32, exit: i32) {
    let entry = engine_entry(recorded);
    let budget =
        format!("poll budget for state '{POLL_STATE}' is exhausted ({used}/{limit} attempts)");
    assert!(entry.contains(&budget), "the entry must name the spent budget, {budget:?}: {entry}");
    let last_exit = format!("exited {exit}");
    assert!(entry.contains(&last_exit), "the entry must name the last exit too: {entry}");
}

fn assert_names_no_budget(entry: &str) {
    for word in ["budget", "attempts", "exhausted"] {
        assert!(!entry.contains(word), "the budget did not end this outcome: {entry}");
    }
}

/// The ids of the tickets a worker-pool run spawned. Reaching the pool is a
/// property of the fixture, so a run that fell back to the sequential loop fails
/// here rather than testing that loop twice over.
fn pool_task_ids<'a>(stdout: &'a str, spawn_prefix: &str) -> BTreeSet<&'a str> {
    let spawns: Vec<&str> =
        stdout.lines().filter_map(|line| line.strip_prefix(spawn_prefix)).collect();
    assert!(!stdout.contains("Deferred"), "both tickets belong in one pass; got:\n{stdout}");
    for spawn in &spawns {
        assert!(spawn.ends_with("(parallel)"), "this test is for the pool; got:\n{spawn}");
    }
    let ids: BTreeSet<&str> =
        spawns.iter().filter_map(|spawn| spawn.split_once(": ")).map(|(id, _)| id).collect();
    assert_eq!(ids.len(), 2, "one id per ticket; got:\n{stdout}");
    ids
}

const TWO_TICKETS: [(&str, &str); 2] = [
    ("01-wait.md", "### Task 1: Wait for the checks\n**State:** awaiting-checks\n"),
    ("02-wait.md", "### Task 2: Wait for the checks as well\n**State:** awaiting-checks\n"),
];

/// The issue's own fixture: the program answers 75 on every attempt and writes
/// nothing, so the entry is the whole of the result and the only account of why
/// the ticket moved. Exit 75 was no failure and chose no edge.
// §FS-rhei-run.5.1 §FS-rhei-errors.1.5
#[test]
fn a_program_poll_answering_75_until_its_budget_is_spent_names_the_budget() {
    let (dir, plan, machine) = run_program_poll(
        "poll-budget-issue-fixture",
        "sys.exit(75)\n",
        3,
        &[SELF_LOOP_ON_75, FINISHED_ON_0, BUDGET_EDGE],
    );

    assert_eq!(attempts_spent(&dir), 3, "the fixture spends all of max_attempts");
    assert_task_state(&plan, &machine, "1", "completed");
    assert_names_the_budget_and_the_exit(&recorded_result(&dir, "plan.1"), 3, 3, 75);
}

/// The worker pool's program path is a separate call site.
// §FS-rhei-run.5.1 §FS-rhei-run.5
#[test]
fn the_worker_pool_names_the_budget_a_program_poll_spent() {
    let (dir, ws, machine) = create_workspace("poll-budget-pool", "# Rhei: Poll\n", &TWO_TICKETS);
    let script = write_python_agent(&dir, "worker.py", "sys.exit(75)\n");
    let text = poll_budget_machine(
        &format!("concurrent: true\n    {}", program_worker(&script, "30s")),
        2,
        &[SELF_LOOP_ON_75, BUDGET_EDGE],
    );
    fs::write(&machine, text).expect("state machine should be written");

    let args = ["--no-tui", "--no-callbacks", "--parallel", "2"];
    let result = run_cli("run", &ws, &machine, &args);
    assert_success(&result);

    let ids = pool_task_ids(&result.stdout, "Spawning program for Task ");
    for (task, id) in ["1", "2"].iter().zip(ids) {
        assert_task_state(&ws, &machine, task, "completed");
        assert_names_the_budget_and_the_exit(&recorded_result(&ws, id), 2, 2, 75);
    }
}

/// An attempt that really crashes on the way out spends the budget like any
/// other, and its exit is what lets a reader tell the crash from an exhausted
/// wait. So the entry names the budget and the exit that actually happened.
// §FS-rhei-run.5.1
#[test]
fn a_crash_on_the_last_attempt_names_the_budget_and_that_exit() {
    let (dir, plan, machine) = run_program_poll(
        "poll-budget-last-crash",
        "sys.exit([75, 75, 1][attempt - 1])\n",
        3,
        &[SELF_LOOP_ON_75, FINISHED_ON_0, BUDGET_EDGE],
    );

    assert_eq!(attempts_spent(&dir), 3, "the fixture spends all of max_attempts");
    assert_task_state(&plan, &machine, "1", "completed");
    assert_names_the_budget_and_the_exit(&recorded_result(&dir, "plan.1"), 3, 3, 1);
}

/// A program poll run to the end of its budget, read back: every attempt spent,
/// the edge taken, and an entry that names the budget and the last exit.
fn assert_the_budget_is_named_after(
    prefix: &str,
    body: &str,
    edges: &[&str],
    target: &str,
    exit: i32,
) {
    let (dir, plan, machine) = run_program_poll(prefix, body, 3, edges);

    assert_eq!(attempts_spent(&dir), 3, "the fixture spends all of max_attempts");
    assert_task_state(&plan, &machine, "1", target);
    assert_names_the_budget_and_the_exit(&recorded_result(&dir, "plan.1"), 3, 3, exit);
}

/// The shape commit 16f42c2ef1 calls the usual one: the exhaustion edge declares
/// the exact code its self-loop does. Whatever it declares, the budget chose it.
// §FS-rhei-run.5.1 §FS-rhei-programs.3.2
#[test]
fn an_exhaustion_edge_declaring_the_self_loops_code_names_the_budget() {
    let exhaustion =
        "  - { from: awaiting-checks, to: completed, exit_code: 75, description: Out of attempts }";
    assert_the_budget_is_named_after(
        "poll-budget-same-code",
        "sys.exit(75)\n",
        &[SELF_LOOP_ON_75, exhaustion],
        "completed",
        75,
    );
}

/// Once the self-loop is refused, a `"nonzero"` catch-all is the only edge left
/// to match. The spent budget left it there, so it is not the ordinary failure
/// the same edge reports while attempts remain.
// §FS-rhei-run.5.1 §FS-rhei-programs.3.2
#[test]
fn a_catch_all_the_spent_budget_left_names_the_budget() {
    assert_the_budget_is_named_after(
        "poll-budget-catch-all-spent",
        "sys.exit(75)\n",
        &[SELF_LOOP_ON_75, CATCH_ALL],
        "failed",
        75,
    );
}

/// No code is declared anywhere, so the fix cannot lean on a self-loop's
/// `exit_code:` to recognize the wait.
// §FS-rhei-run.5.1
#[test]
fn a_condition_only_self_loop_names_the_budget() {
    assert_the_budget_is_named_after(
        "poll-budget-condition-loop",
        "sys.exit([0, 0, 1][attempt - 1])\n",
        &[SELF_LOOP_WHILE_BUDGET_LASTS, BUDGET_EDGE],
        "completed",
        1,
    );
}

/// A last attempt whose exit has an exact edge of its own: the ticket takes that
/// edge, but the spent budget is what let the exit leave, so the budget is still
/// named beside the exit. The classification, not the edge's code, decides this.
// §FS-rhei-run.5.1 §FS-rhei-programs.3.2
#[test]
fn a_different_exact_edge_on_the_last_attempt_names_the_budget() {
    let route = "  - { from: awaiting-checks, to: checked, exit_code: 3, description: Routed }";
    assert_the_budget_is_named_after(
        "poll-budget-other-exact-edge",
        "sys.exit([75, 75, 3][attempt - 1])\n",
        &[SELF_LOOP_ON_75, route],
        "checked",
        3,
    );
}

/// The second copy of the rule, and the one the issue does not name. An agent's
/// first attempt finishes and takes the self-loop; its second fails, and the
/// budget it spent is the only thing that selects an edge for it.
// §FS-rhei-run.5.1 §FS-rhei-states.3.3
#[test]
fn an_agent_poll_that_spends_its_budget_names_it() {
    let dir = unique_temp_dir("poll-budget-agent");
    write_mock_agent(&dir, &dir, "sys.exit([0, 1][attempt - 1])\n");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &poll_budget_machine("agent: mock", 2, &[SELF_LOOP_WHILE_BUDGET_LASTS, BUDGET_EDGE]),
    );

    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));

    assert_eq!(attempts_spent(&dir), 2, "the fixture spends all of max_attempts");
    assert_task_state(&plan, &machine, "1", "completed");
    assert_names_the_budget_and_the_exit(&recorded_result(&dir, "plan.1"), 2, 2, 1);
}

/// The agent worker pool applies the selection in code of its own. One attempt
/// each keeps the two tickets from sharing the counter, and one attempt is the
/// whole budget.
// §FS-rhei-run.5.1 §FS-rhei-run.5
#[test]
fn the_worker_pool_names_the_budget_an_agent_poll_spent() {
    let (dir, ws, machine) =
        create_workspace("poll-budget-agent-pool", "# Rhei: Poll\n", &TWO_TICKETS);
    write_mock_agent(&dir, &ws, "sys.exit(1)\n");
    let text = poll_budget_machine(
        "concurrent: true\n    agent: mock",
        1,
        &[SELF_LOOP_WHILE_BUDGET_LASTS, BUDGET_EDGE],
    );
    fs::write(&machine, text).expect("state machine should be written");

    let args = ["--no-tui", "--no-callbacks", "--parallel", "2"];
    let result = run_cli("run", &ws, &machine, &args);
    assert_success(&result);

    let ids = pool_task_ids(&result.stdout, "Spawning agent 'mock' for Task ");
    for (task, id) in ["1", "2"].iter().zip(ids) {
        assert_task_state(&ws, &machine, task, "completed");
        assert_names_the_budget_and_the_exit(&recorded_result(&ws, id), 1, 1, 1);
    }
}

/// With attempts still in hand the budget ended nothing, so a poll state's
/// `"nonzero"` catch-all is the ordinary failure it is anywhere else.
///
/// This passes before the change as well as after: it guards the change.
// §FS-rhei-programs.3.2 §FS-rhei-states.3.3
#[test]
fn a_nonzero_catch_all_with_attempts_left_names_no_budget() {
    let (dir, plan, machine) = run_program_poll(
        "poll-budget-catch-all",
        "sys.exit(2)\n",
        3,
        &[SELF_LOOP_ON_75, CATCH_ALL, BUDGET_EDGE],
    );

    assert_eq!(attempts_spent(&dir), 1, "the first attempt takes the catch-all");
    assert_task_state(&plan, &machine, "1", "failed");
    let recorded = recorded_result(&dir, "plan.1");
    let entry = engine_entry(&recorded);
    assert!(entry.contains("exited 2"), "the entry names the failure: {entry}");
    assert_names_no_budget(entry);
}

/// An attempt that exceeded its timeout was ended by the watchdog, on the last
/// attempt as on any other, so it takes the timeout's route and the timeout's
/// account. One attempt makes it the last without racing the watchdog on an
/// earlier one.
///
/// These pass before the change as well as after: they guard the change.
// §FS-rhei-run.5.1 §FS-rhei-states.3.3 §FS-rhei-agents.7.5
#[test]
fn a_program_timeout_on_the_last_attempt_names_no_budget() {
    let timeout_edge =
        "  - { from: awaiting-checks, to: failed, timeout: 1s, description: Too slow }";
    let dir = unique_temp_dir("poll-budget-program-timeout");
    let script = counted_script(&dir, "time.sleep(20)\n");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &poll_budget_machine(
            &program_worker(&script, "1s"),
            1,
            &[SELF_LOOP_WHILE_BUDGET_LASTS, BUDGET_EDGE, timeout_edge],
        ),
    );

    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));

    assert_task_state(&plan, &machine, "1", "failed");
    let recorded = recorded_result(&dir, "plan.1");
    let entry = engine_entry(&recorded);
    assert!(entry.contains("timed out"), "the entry names the timeout: {entry}");
    assert_names_no_budget(entry);
}

// §FS-rhei-run.5.1 §FS-rhei-states.3.3 §FS-rhei-agents.7.5
#[test]
fn an_agent_timeout_on_the_last_attempt_names_no_budget() {
    let timeout_edge =
        "  - { from: awaiting-checks, to: failed, timeout: 1s, description: Too slow }";
    let dir = unique_temp_dir("poll-budget-agent-timeout");
    write_mock_agent(&dir, &dir, "time.sleep(20)\n");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &poll_budget_machine(
            "agent: mock\n    agent_timeout: 1s",
            1,
            &[SELF_LOOP_WHILE_BUDGET_LASTS, BUDGET_EDGE, timeout_edge],
        ),
    );

    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));

    assert_task_state(&plan, &machine, "1", "failed");
    let recorded = recorded_result(&dir, "plan.1");
    let entry = engine_entry(&recorded);
    assert!(entry.contains("timed out"), "the entry names the timeout: {entry}");
    assert_names_no_budget(entry);
}

/// A zero exit reported success, and the engine has nothing to explain even on
/// the attempt that spent the budget. The program's own words stand alone.
///
/// This passes before the change as well as after: it guards the change.
// §FS-rhei-run.3 §FS-rhei-states.3.3
#[test]
fn a_zero_last_attempt_writes_no_entry() {
    let (dir, plan, machine) = run_program_poll(
        "poll-budget-zero-last",
        "if attempt == 3:\n    result('the checks landed\\n')\n    sys.exit(0)\nsys.exit(75)\n",
        3,
        &[SELF_LOOP_ON_75, FINISHED_ON_0, BUDGET_EDGE],
    );

    assert_eq!(attempts_spent(&dir), 3, "the fixture spends all of max_attempts");
    assert_task_state(&plan, &machine, "1", "completed");
    let recorded = recorded_result(&dir, "plan.1");
    assert!(recorded.contains("the checks landed"), "the program's result stands: {recorded}");
    assert!(!recorded.contains("`rhei run`:"), "the engine adds nothing: {recorded}");
}

/// A program naming its own edge while attempts remain is a declared route, on
/// a poll state as on any other, and leaves the result to the program.
///
/// This passes before the change as well as after: it guards the change.
// §FS-rhei-programs.3.2
#[test]
fn an_exact_route_with_attempts_left_writes_no_entry() {
    let route = "  - { from: awaiting-checks, to: checked, exit_code: 3, description: Routed }";
    let (dir, plan, machine) = run_program_poll(
        "poll-budget-exact-route",
        "sys.exit(3)\n",
        3,
        &[SELF_LOOP_ON_75, route, BUDGET_EDGE],
    );

    assert_eq!(attempts_spent(&dir), 1, "the first attempt takes the route");
    assert_task_state(&plan, &machine, "1", "checked");
    let recorded = recorded_result(&dir, "plan.1");
    assert!(recorded.trim().is_empty(), "a declared route writes nothing: {recorded}");
}

/// Reverted attempts charge the same bound without firing the exhaustion edge,
/// so no edge carries an entry: the ticket stalls in its state with a result
/// that says nothing, and its halt is what names the spent bound.
///
/// This passes before the change as well as after: it guards the change.
// §FS-rhei-run.3.7.4 §FS-rhei-run.5.1
#[test]
fn attempts_a_revert_spent_fire_no_edge_and_write_no_entry() {
    let dir = unique_temp_dir("poll-budget-reverted");
    let script = write_python_agent(&dir, "poll.py", POLL_CULPRIT);
    let text = poll_machine(&fixture_command(&script)).replace("interval: 3s", "interval: 0s");
    let (ws, machine) =
        workspace(&dir, &[("01-t.md", "### Task 1: One\n**State:** waiting\n\nBody.\n")], &text);

    let mut output = String::new();
    for _ in 0..4 {
        let ran = run_cli("run", &ws, &machine, &["--no-tui", "--no-callbacks"]);
        output.push_str(&format!("{}{}\n---\n", ran.stdout, ran.stderr));
    }

    assert_eq!(runs(&dir, "poll-runs.txt"), 3, "spawns stop at max_attempts\n{output}");
    assert!(
        output.contains("halting Task ws.1 in state 'waiting': 3 attempts spent"),
        "the fixture reached the spent bound\n{output}"
    );
    let task = fs::read_to_string(ws.join("tasks/01-t.md")).expect("read task file");
    assert_eq!(state_in(&task), "waiting", "no edge fires\n{output}");
    let recorded = recorded_result(&ws, "ws.1");
    assert!(recorded.trim().is_empty(), "no edge carried an entry: {recorded}");
}
