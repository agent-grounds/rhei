// How often one run spawns a supervisor held for an empty visit: once, and
// again only after something advanced after the hold — never on the move that
// woke it, a poll rescheduling itself, or a deadline sleep. Sequential and
// pooled runs alike.

// §FS-rhei-run.3.6 §FS-rhei-supervision.3.6

use std::fs;
use std::path::{Path, PathBuf};

use super::supervision_tests::setup_supervision_with_agent;
use super::*;

/// The `grounded-ticket` supervisor's shape: `execute_on: child-terminal`, no
/// `outputs:`, a releasing self-loop and an `openDescendants < 1` exit, over a
/// child that needs nothing and a child waiting on a brief only it writes.
const MACHINE: &str = r#"name: empty-visit-release
version: 1
states:
  supervising:
    initial: true
    description: Supervise the subtree
    execute_on: child-terminal
    agent: mock
    agent_timeout: 30s
    visits: 12
    instructions: You supervise Task {task_id}.
  work:
    description: Work that needs no brief
    agent: mock
    agent_timeout: 30s
    concurrent: true
    instructions: Do the work.
  review:
    description: Work only the supervisor can brief
    agent: mock
    agent_timeout: 30s
    inputs:
      - name: brief
        path: runtime/supervise/{task_id}.md
    instructions: Review as briefed.
  waiting:
    description: Poll something outside the subtree
    agent: mock
    agent_timeout: 30s
    poll: { interval: 1s, max_attempts: 3 }
    instructions: Poll.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: work, to: completed, description: Worked }
  - { from: review, to: completed, description: Reviewed }
  - { from: waiting, to: waiting, description: Poll again }
  - { from: waiting, to: completed, description: Arrived }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// Logs `<task> <state> visit=<n> attempt=<n>`. The supervisor writes nothing
/// on any visit — what `grounded-ticket` tells it to do on every visit but the
/// last — so its second visit releases nothing.
const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID_LOCAL')
state = env('RHEI_STATE')
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {} visit={} attempt={}\n'.format(
    task, state, env('RHEI_VISIT_COUNT', '?'), env('RHEI_ATTEMPT', '?')))
if state != 'supervising':
    result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#;

/// The supervisor and its two children: case A's whole plan, and case B's
/// first ticket.
const SUPERVISOR_TICKET: &str = "### Task 1: Supervise
**State:** supervising

#### Task 1.1: Work
**State:** work

#### Task 1.2: Review
**State:** review
";

/// Case B adds a 1 s poll outside the subtree that waits on plain work.
const BESIDE_POLL: [(&str, &str); 2] = [
    ("02-watch.md", "### Task 2: Watch something else\n**State:** waiting\n**Prior:** Task 3\n"),
    ("03-prepare.md", "### Task 3: Prepare\n**State:** work\n"),
];

/// A workspace `plan/` with one ticket per file, so `--parallel 2` really runs
/// the worker pool rather than falling back to the sequential loop as it does
/// for tickets that share a plan file (§FS-rhei-run.2.5). Returns the
/// directory, the workspace and the machine.
fn workspace(prefix: &str, extra: &[(&str, &str)]) -> (TestDir, PathBuf, PathBuf) {
    let (dir, single, machine) = setup_supervision_with_agent(prefix, "", MACHINE, AGENT);
    fs::remove_file(single).expect("drop the single-file plan");
    let tasks = dir.join("plan/tasks");
    fs::create_dir_all(&tasks).expect("create workspace");
    fs::write(dir.join("plan/index.rhei.md"), "# Rhei: Empty supervising visit\n")
        .expect("write index");
    fs::write(tasks.join("01-supervise.md"), SUPERVISOR_TICKET).expect("write ticket");
    let settings = dir.join("plan/.agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings dir");
    fs::copy(dir.join(".agent-grounds/rhei/settings.json"), settings.join("settings.json"))
        .expect("copy settings");
    for (name, text) in extra {
        fs::write(tasks.join(name), text).expect("write ticket");
    }
    let plan = dir.join("plan");
    (dir, plan, machine)
}

/// The `<task> <state> visit=<n> attempt=<n>` lines the agent logged, in order.
fn spawns(plan: &Path) -> Vec<String> {
    fs::read_to_string(plan.join("runtime/logs/spawns.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

const HELD_VISIT: &str = "1 supervising visit=2 ";

fn run(plan: &Path, machine: &Path, parallel: Option<&str>) -> String {
    let mut args = vec!["--no-callbacks", "--no-tui"];
    if let Some(n) = parallel {
        args.extend(["--parallel", n]);
    }
    let out = run_cli("run", plan, machine, &args);
    let combined = format!("{}{}", out.stdout, out.stderr);
    assert!(!out.status.success(), "the run halts on the held supervisor; got:\n{combined}");
    combined
}

/// The run reached §3.6 at all: Task 1.1 woke the supervisor and the visit
/// after it was held as empty.
fn assert_reached_hold(log: &[String], output: &str) {
    assert!(
        log.iter().any(|line| line.starts_with("1.1 work ")),
        "Task 1.1 ran and woke the supervisor; spawns:\n{}\noutput:\n{output}",
        log.join("\n")
    );
    assert!(
        output.contains("holding Task ") && output.contains("the visit released nothing"),
        "the visit after it was held as empty; output:\n{output}"
    );
}

/// Case A: nothing moves after the hold, so the held visit is spawned once and
/// the run halts on the hold warning.
fn assert_supervisor_alone(prefix: &str, parallel: Option<&str>) {
    let (_dir, plan, machine) = workspace(prefix, &[]);
    let output = run(&plan, &machine, parallel);
    let log = spawns(&plan);
    assert_reached_hold(&log, &output);
    let held = log.iter().filter(|line| line.starts_with(HELD_VISIT)).count();
    assert_eq!(
        held,
        1,
        "the move that woke the supervisor is no advance after its hold, so the empty \
         visit is spawned once (§FS-rhei-run.3.6); spawns:\n{}\noutput:\n{output}",
        log.join("\n")
    );
}

/// Case B. The held visit is woken by Task 1.1's move and judged after it, so
/// the advances that may buy it another spawn are the moves outside the subtree
/// that `runtime/state-transitions.log` records after that one: Task 3
/// completing and Task 2 completing, whichever of them land later. A poll
/// rescheduling itself appends no entry (§FS-rhei-run.3 step 8). So the held
/// visit is spawned at most once plus once per such move — three when both land
/// after the hold, two when Task 3 finished first — however the run interleaved
/// its work, and every spawn past that bound was released by the move that woke
/// the supervisor, a poll's self-loop, or a deadline sleep.
fn assert_beside_poll(prefix: &str, parallel: Option<&str>) {
    let (_dir, plan, machine) = workspace(prefix, &BESIDE_POLL);
    let output = run(&plan, &machine, parallel);
    let log = spawns(&plan);
    assert_reached_hold(&log, &output);
    let moves = fs::read_to_string(plan.join("runtime/state-transitions.log"))
        .expect("read the transition ledger");
    let moves: Vec<&str> = moves.lines().collect();
    let woke = moves
        .iter()
        .position(|line| line.ends_with(" work@completed") && line.starts_with("plan.1.1 "))
        .unwrap_or_else(|| panic!("Task 1.1's move is in the ledger:\n{}", moves.join("\n")));
    let after_hold = moves[woke + 1..]
        .iter()
        .filter(|line| line.starts_with("plan.2 ") || line.starts_with("plan.3 "))
        .count();
    // A run that halts with the poll still waiting never asked the question:
    // the held supervisor does not end the run early either. §FS-rhei-run.5.1
    assert!(
        moves.contains(&"plan.2 waiting@completed"),
        "the run waits out Task 2's poll before it halts on the held supervisor\nspawns:\n{}\n\
         ledger:\n{}\noutput:\n{output}",
        log.join("\n"),
        moves.join("\n")
    );
    let held = log.iter().filter(|line| line.starts_with(HELD_VISIT)).count();
    assert!(
        held <= 1 + after_hold,
        "the held visit may be spawned once plus once per move outside the subtree after \
         its hold ({after_hold}), so at most {} times; it was spawned {held} times \
         (§FS-rhei-run.3.6)\nspawns:\n{}\nledger:\n{}\noutput:\n{output}",
        1 + after_hold,
        log.join("\n"),
        moves.join("\n")
    );
}

/// Case A, the ticket's first reproduction.
// §FS-rhei-run.3.6
#[test]
fn a_held_empty_visit_is_spawned_once_in_the_run_whose_move_woke_it() {
    assert_supervisor_alone("empty-visit-release-a", None);
}

/// The control: a fresh run over A's held plan spawns the supervisor once and
/// halts — the behaviour case A must match.
// §FS-rhei-supervision.3.6
#[test]
fn a_fresh_run_over_a_held_plan_spawns_the_supervisor_once() {
    let (_dir, plan, machine) = workspace("empty-visit-release-control", &[]);
    run(&plan, &machine, None);
    let before = spawns(&plan).iter().filter(|line| line.starts_with("1 supervising ")).count();
    let output = run(&plan, &machine, None);
    let log = spawns(&plan);
    let after = log.iter().filter(|line| line.starts_with("1 supervising ")).count();
    assert_eq!(after - before, 1, "spawns:\n{}\noutput:\n{output}", log.join("\n"));
    assert!(output.contains("the visit released nothing"), "output:\n{output}");
}

/// Case B, the ticket's second reproduction: a waiting poll outside the
/// subtree re-armed the hold on every attempt and every deadline sleep.
// §FS-rhei-run.3.6
#[test]
fn a_poll_outside_the_subtree_does_not_re_spawn_a_held_empty_visit() {
    assert_beside_poll("empty-visit-release-b", None);
}

/// Case A in the worker pool, which keeps its own refill-and-reset loop.
// §FS-rhei-run.3.6 §FS-rhei-run.5
#[test]
fn a_held_empty_visit_is_spawned_once_under_parallel_two() {
    assert_supervisor_alone("empty-visit-release-a-pool", Some("2"));
}

/// Case B in the worker pool.
// §FS-rhei-run.3.6 §FS-rhei-run.5
#[test]
fn a_poll_outside_the_subtree_does_not_re_spawn_a_held_empty_visit_under_parallel_two() {
    assert_beside_poll("empty-visit-release-b-pool", Some("2"));
}
