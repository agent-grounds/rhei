// Rule 3 of the empty-visit test, clause by clause: what "the subtree can still
// move" means for a descendant that is waiting on a poll's clock or on work
// outside its supervisor's subtree, and why neither excuses a `inputs:` file
// that only the supervisor writes.
//
// Its own part because these are the *scope* of the rule rather than the rule:
// each case is a pair — one where the descendant really can move and the visit
// releases, one where it cannot and the visit is held — and the pairs are what
// keep the clause from being read as an unconditional yes.

// §FS-rhei-supervision.3.6

use std::fs;

use super::supervision_tests::{setup_supervision_with_agent, spawn_log};
use super::*;

/// A supervisor over a child that gates on a brief, a `human-review` gate for a
/// task outside the subtree to sit in, and a `waiting` state that polls.
fn rule_three_machine() -> &'static str {
    r#"name: supervision-release-rule
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
  review:
    description: Review
    agent: mock
    agent_timeout: 30s
    inputs:
      - name: brief
        path: runtime/supervise/{task_id}.md
    instructions: Review as briefed.
  waiting:
    description: Poll for the brief
    agent: mock
    agent_timeout: 30s
    poll: { interval: 1s, max_attempts: 2 }
    inputs:
      - name: brief
        path: runtime/supervise/{task_id}.md
    instructions: Poll.
  human-review:
    description: A human decides
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: review, to: completed, description: Reviewed }
  - { from: waiting, to: waiting, description: Poll again }
  - { from: waiting, to: completed, description: Arrived }
  - { from: human-review, to: completed, description: Decided }
  - { from: "*", to: cancelled, description: Dropped }
"#
}

/// A supervisor that writes nothing at all, so only the release test decides
/// what its visit meant.
const SILENT_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {} {}\n'.format(task, state, visit))

result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#;

/// A child blocked on a prior outside the subtree *and* on the brief its
/// supervisor never wrote. The outside prior says the supervisor is not the
/// only thing in the child's way; it does not say the child could run, and
/// reading it that way released the subtree and stranded the run — the ticket's
/// own failure, through a different door.
// §FS-rhei-supervision.3.6 rule 3
#[test]
fn an_outside_prior_does_not_excuse_an_input_only_the_supervisor_writes() {
    const OUTSIDE_PRIOR_PLAN: &str = r#"# Rhei: Outside prior

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Review
**State:** review
**Prior:** Task 2

### Task 2: Decide
**State:** human-review
"#;
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-outside-prior",
        OUTSIDE_PRIOR_PLAN,
        rule_three_machine(),
        SILENT_AGENT,
    );
    let before = fs::read_to_string(&plan_path).expect("read plan");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("the visit released nothing"),
        "the visit is held, not released; got:\n{combined}"
    );
    assert!(
        combined.contains("waits on brief"),
        "and names the file the child is waiting for; got:\n{combined}"
    );
    // But for the ticket's budget identity: the visit spawned, so its admission
    // reserved a unit against the project and the ticket earned one.
    // §FS-rhei-budgets.6.1
    super::budget_support::assert_plan_but_for_the_budget_identity(
        &fs::read_to_string(&plan_path).expect("read plan"),
        &before,
        "a held visit rewrites nothing",
    );
    assert_eq!(spawn_log(&dir), vec!["plan.1 supervising 1".to_string()]);
}

/// The other half of the same clause: with the brief on disk, the outside prior
/// really is the only thing in the child's way, and that is someone else's
/// work. The visit releases exactly as it did before this rule existed.
// §FS-rhei-supervision.3.6 rule 3
#[test]
fn an_outside_prior_still_releases_when_the_inputs_are_there() {
    const OUTSIDE_PRIOR_PLAN: &str = r#"# Rhei: Outside prior, brief written

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Review
**State:** review
**Prior:** Task 2

### Task 2: Decide
**State:** human-review
"#;
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-outside-prior-briefed",
        OUTSIDE_PRIOR_PLAN,
        rule_three_machine(),
        SILENT_AGENT,
    );
    let brief = dir.join("runtime/supervise/plan.1.1.md");
    fs::create_dir_all(brief.parent().expect("brief dir")).expect("create brief dir");
    fs::write(&brief, "Review it.\n").expect("write brief");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("released nothing"),
        "the child waits on other work, not on its supervisor; got:\n{combined}"
    );
    assert!(
        combined.contains("'supervising' -> 'supervising'"),
        "so the release self-loop fires; got:\n{combined}"
    );
}

/// A `poll:` state schedules its own next attempt, which says when the child
/// runs and nothing about whether it can. With the brief missing, the poll
/// would spin to its own exhaustion having done nothing — and reading it as
/// "can move" released the supervisor and put the run back on the ticket's
/// original "rerun to pick it up" advice.
// §FS-rhei-supervision.3.6 rule 3
#[test]
fn a_poll_state_does_not_excuse_an_input_only_the_supervisor_writes() {
    const POLLING_CHILD_PLAN: &str = r#"# Rhei: Polling child

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Wait
**State:** waiting
"#;
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-poll-child",
        POLLING_CHILD_PLAN,
        rule_three_machine(),
        SILENT_AGENT,
    );
    let before = fs::read_to_string(&plan_path).expect("read plan");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("the visit released nothing"),
        "the visit is held, not released; got:\n{combined}"
    );
    assert!(
        !combined.contains("rerun to pick it up"),
        "and the run never falls back to the advice that cannot help; got:\n{combined}"
    );
    // But for the ticket's budget identity: the visit spawned, so its admission
    // reserved a unit against the project and the ticket earned one.
    // §FS-rhei-budgets.6.1
    super::budget_support::assert_plan_but_for_the_budget_identity(
        &fs::read_to_string(&plan_path).expect("read plan"),
        &before,
        "a held visit rewrites nothing",
    );
    assert_eq!(spawn_log(&dir), vec!["plan.1 supervising 1".to_string()]);
}

/// And with its inputs on disk a polling child is genuinely waiting on its
/// clock, so the visit releases and the poll gets to run.
// §FS-rhei-supervision.3.6 rule 3
#[test]
fn a_poll_state_still_releases_when_the_inputs_are_there() {
    const POLLING_CHILD_PLAN: &str = r#"# Rhei: Polling child, brief written

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Wait
**State:** waiting
"#;
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-poll-child-briefed",
        POLLING_CHILD_PLAN,
        rule_three_machine(),
        SILENT_AGENT,
    );
    let brief = dir.join("runtime/supervise/plan.1.1.md");
    fs::create_dir_all(brief.parent().expect("brief dir")).expect("create brief dir");
    fs::write(&brief, "Poll for it.\n").expect("write brief");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("released nothing"),
        "the child waits on its poll interval, not on its supervisor; got:\n{combined}"
    );
    assert!(
        spawn_log(&dir).iter().any(|line| line.starts_with("plan.1.1 waiting")),
        "so the polling child runs; got:\n{:?}",
        spawn_log(&dir)
    );
}

/// Every ticket the hold names carries a reason, and a prior is one of them. A
/// descendant whose own `inputs:` are all on disk is blocked by nothing the
/// closing "unblock what they wait for" can be read against unless the run says
/// what it is — so it names the `**Prior:**` rule 3 blamed, in the shape every
/// other blocked-on row uses.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
#[test]
fn a_descendant_blocked_only_by_a_prior_is_named_with_that_prior() {
    const PRIOR_BLOCKED_PLAN: &str = r#"# Rhei: Blocked behind a sibling

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Review
**State:** review

#### Task 1.2: Wait
**State:** waiting
**Prior:** Task 1.1
"#;
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-prior-blocked-descendant",
        PRIOR_BLOCKED_PLAN,
        rule_three_machine(),
        SILENT_AGENT,
    );
    // Task 1.2 has everything it declares; only its sibling stands in its way.
    let brief = dir.join("runtime/supervise/plan.1.2.md");
    fs::create_dir_all(brief.parent().expect("brief dir")).expect("create brief dir");
    fs::write(&brief, "Poll for it.\n").expect("write brief");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("Task plan.1.1 (review) waits on brief"),
        "the descendant missing a file is named with the file; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.1.2 (waiting) waits on Task plan.1.1 (review)"),
        "and the one missing none is named with the prior that blocks it; got:\n{combined}"
    );
}
