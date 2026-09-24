// Empty supervising visits end to end: a visit that exits 0 having released
// nothing is held for a rerun instead of spending the self-loop that would
// strand the run — and every visit that *did* release something still takes
// that edge exactly as it always did.

// §FS-rhei-supervision.3.6

use std::fs;
use std::path::Path;

use super::supervision_tests::{assert_state_anywhere, setup_supervision_with_agent, spawn_log};
use super::*;

/// The canonical machine of §FS-rhei-supervision.7 with `review` gated on the
/// brief its supervisor writes — §FS-rhei-supervision.5.2 permits exactly this,
/// and it is the shape the ticket reported.
fn brief_gated_machine() -> &'static str {
    r#"name: supervision-empty-visit
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
    outputs:
      - name: findings
        path: runtime/review/{task_id}.md
    instructions: Review as briefed.
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
  - { from: supervising, to: human-review, description: Budget exhausted, condition: visitCount >= visits }
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: review, to: completed, description: Findings written }
  - { from: "*", to: cancelled, description: Dropped }
"#
}

/// A supervisor that writes a brief only once `runtime/unblock` exists, so one
/// workspace can show the same visit empty and then not.
const CONDITIONAL_BRIEF_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {} {}\n'.format(task, state, visit))

if state == 'supervising':
    if (root / 'runtime' / 'unblock').exists():
        write(root / 'runtime' / 'supervise' / (task + '.1.md'), 'Review it.\n')
elif state == 'review':
    write(root / 'runtime' / 'review' / (task + '.md'), 'Findings from {}.\n'.format(task))

result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#;

const ONE_CHILD_PLAN: &str = r#"# Rhei: No-op supervising visit

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Review
**State:** review
"#;

/// The `**State:**` line of a task, read from the plan file itself.
///
/// The visit counter is the whole assertion here, and `rhei render` normalizes
/// `supervising-2` back to `supervising`; only the raw line can tell a spent
/// visit from an unspent one.
// §FS-rhei-supervision.3.6 §FS-rhei-transitions.2.3
fn raw_state_line(plan: &Path, heading: &str) -> String {
    let plan = fs::read_to_string(plan).expect("read plan");
    let after = plan.split(heading).nth(1).unwrap_or_else(|| panic!("plan has {heading}"));
    after
        .lines()
        .find_map(|line| line.strip_prefix("**State:** "))
        .expect("the task has a state line")
        .trim()
        .to_string()
}

/// The ticket: a brief-gated child, a supervisor that writes nothing, and the
/// self-loop that used to consume the only edge that could wake it again.
/// Run 2 proves the hold is rerunnable; run 2's brief proves the release still
/// happens the moment the subtree can move.
// §FS-rhei-supervision.3.6
#[test]
fn an_empty_supervising_visit_is_held_and_a_rerun_spawns_the_supervisor_again() {
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-empty-visit",
        ONE_CHILD_PLAN,
        brief_gated_machine(),
        CONDITIONAL_BRIEF_AGENT,
    );

    let first = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", first.stdout, first.stderr);
    assert!(!first.status.success(), "the run halts; got:\n{combined}");
    assert!(
        combined.contains("the visit released nothing"),
        "the halt names the empty visit; got:\n{combined}"
    );
    assert!(
        combined.contains("waits on brief"),
        "and the file its subtree is waiting for; got:\n{combined}"
    );
    // The visit is not spent: no self-loop fired, so `stateVisits` never moved
    // and the plan carries no `supervision` block at all.
    // §FS-rhei-supervision.3.6 §FS-rhei-supervision.3.5
    assert_eq!(raw_state_line(&plan_path, "### Task 1: Supervise"), "supervising");
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    assert!(!plan.contains("phase: released"), "the subtree was not released:\n{plan}");
    assert!(!plan.contains("stateVisits"), "the visit was not spent:\n{plan}");

    // A rerun spawns the same visit again — the property the ticket asks for,
    // and the one "rerun to pick it up" never delivered.
    fs::write(dir.join("runtime/unblock"), "go\n").expect("write unblock");
    let second = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&second);
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
        ],
        "the held visit is re-spawned, and once it writes the brief the subtree proceeds"
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
}

/// The hold has to stay answerable however long it lasts. The default attempt
/// budget is two, so a third and a fourth run prove the withheld edge gives its
/// attempt back; then writing the very file the warning named — no `rhei
/// reset`, no hand transition — makes the next run proceed.
// §FS-rhei-supervision.3.6 §FS-rhei-agents.3.2.3
#[test]
fn repeated_empty_visits_never_spend_the_attempt_budget() {
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-empty-visit-budget",
        ONE_CHILD_PLAN,
        brief_gated_machine(),
        CONDITIONAL_BRIEF_AGENT,
    );

    // Four runs is twice the default budget: on a charged visit the third would
    // already refuse to spawn. §FS-rhei-agents.3.2.3
    for run in 1..=4 {
        let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
        let combined = format!("{}{}", result.stdout, result.stderr);
        assert!(
            combined.contains("the visit released nothing"),
            "run {run} visits the supervisor and holds it; got:\n{combined}"
        );
        assert!(
            !combined.contains("attempts spent on this visit"),
            "run {run} never charges the visit's budget; got:\n{combined}"
        );
    }
    assert_eq!(
        spawn_log(&dir),
        vec!["plan.1 supervising 1".to_string(); 4],
        "one visit per run, and the visit is still the first"
    );

    // The remedy the warning names, and nothing else.
    let brief = dir.join("runtime/supervise/plan.1.1.md");
    fs::create_dir_all(brief.parent().expect("brief dir")).expect("create brief dir");
    fs::write(&brief, "Review it.\n").expect("write brief");
    let recovered = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&recovered);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
}

/// What a held visit leaves behind: nothing. The fixture carries everything a
/// fired self-loop would have rewritten — a spent visit count, a delivered
/// checkpoint, a `phase` — so the preservation §3.6 claims is asserted against
/// a plan that has something to lose. The `**Assignee:**` the release edge
/// drops (§3.4) is not among them and is not in the fixture: `rhei run`
/// schedules neither a claimed ticket (§FS-rhei-run.3) nor a supervisor whose
/// descendant is claimed (§3.1, the drain), so no visit this rule judges can
/// have one, and a field placed where it survives either way asserts nothing.
// §FS-rhei-supervision.3.6
#[test]
fn a_held_visit_leaves_the_plan_file_untouched() {
    const CARRIES_METADATA_PLAN: &str = r#"# Rhei: Second visit

---
structure:
  maxLevels: 3
metadata:
  tasks:
    1:
      stateVisits:
        supervising: 2
      supervision:
        phase: held
        checkpoints:
          - task: "1.2"
            from: review
            to: completed
            visit: 1
---

## Tasks

### Task 1: Supervise
**State:** supervising-2

#### Task 1.1: Review
**State:** review

#### Task 1.2: Review lexer
**State:** completed

> **Result:** [plan.1.2](runtime/results/plan.1.2.md)
"#;
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-empty-visit-metadata",
        CARRIES_METADATA_PLAN,
        brief_gated_machine(),
        CONDITIONAL_BRIEF_AGENT,
    );
    let result_path = dir.join("runtime/results/plan.1.2.md");
    fs::create_dir_all(result_path.parent().expect("results dir")).expect("create results dir");
    fs::write(&result_path, "## Result\n\nDone.\n").expect("write result");
    let before = fs::read_to_string(&plan_path).expect("read plan");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("the visit released nothing"), "the visit is held; got:\n{combined}");
    // But for the ticket's budget identity: the visit spawned, so its admission
    // reserved a unit against the project and the ticket earned one.
    // §FS-rhei-budgets.6.1
    super::budget_support::assert_plan_but_for_the_budget_identity(
        &fs::read_to_string(&plan_path).expect("read plan"),
        &before,
        "no transition fired, so the checkpoint and the visit count both stand",
    );
}

/// The same scenario through the worker pool: the parallel completion path
/// mirrors the sequential one, so the rule has to bite there identically.
// §FS-rhei-supervision.3.6 §FS-rhei-run.5
#[test]
fn an_empty_supervising_visit_is_held_under_parallel_too() {
    let (_dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-empty-visit-parallel",
        ONE_CHILD_PLAN,
        brief_gated_machine(),
        CONDITIONAL_BRIEF_AGENT,
    );

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--parallel", "4", "--no-callbacks", "--no-tui"],
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(!result.status.success(), "the run halts; got:\n{combined}");
    assert!(
        combined.contains("the visit released nothing"),
        "the halt names the empty visit; got:\n{combined}"
    );
    assert_eq!(raw_state_line(&plan_path, "### Task 1: Supervise"), "supervising");
}

/// The scope guard, and the more important half of the rule: a machine whose
/// children are not gated on briefs behaves exactly as it did before. The
/// supervisor writes nothing, and the subtree proceeds — because it can.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.6
#[test]
fn a_supervisor_that_writes_nothing_still_releases_a_subtree_that_can_move() {
    const CHAINED_PLAN: &str = r#"# Rhei: Observer

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Watch
**State:** supervising

#### Task 1.1: Review parser
**State:** review

#### Task 1.2: Review lexer
**State:** review
**Prior:** Task 1.1
"#;
    // The same machine without the brief `inputs:`, so nothing beneath the
    // supervisor ever needs it to act.
    let machine = brief_gated_machine().replace(
        "    inputs:\n      - name: brief\n        path: runtime/supervise/{task_id}.md\n",
        "",
    );
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-observer",
        CHAINED_PLAN,
        &machine,
        CONDITIONAL_BRIEF_AGENT,
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);
    assert!(
        !format!("{}{}", result.stdout, result.stderr).contains("released nothing"),
        "a visit that left the subtree able to move released something; got:\n{}",
        result.stdout
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.2", "completed");
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
            "plan.1.2 review 1".to_string(),
            "plan.1 supervising 3".to_string(),
        ],
        "hold \u{2192} visit \u{2192} release \u{2192} child, exactly as before the rule existed"
    );
}

/// A visit whose only act is a cancel released something: the engine does not
/// second-guess how a supervisor steers, only whether it did.
// §FS-rhei-supervision.3.6 rule 2
#[test]
fn a_visit_whose_only_act_is_a_cancel_counts_as_a_release() {
    const TWO_GATED_CHILDREN: &str = r#"# Rhei: Drop one

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Review parser
**State:** review

#### Task 1.2: Review lexer
**State:** review
"#;
    // The binary path is JSON-encoded rather than pasted: on Windows it is
    // full of backslashes, which a plain Python literal would read as escapes.
    let cancel_only = format!(
        r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
append(root / 'runtime' / 'logs' / 'spawns.log', '{{}} {{}} {{}}\n'.format(task, state, visit))

if state == 'supervising' and visit == '1':
    import subprocess

    subprocess.run(
        [
            {binary},
            '--state-machine',
            env('RHEI_STATE_MACHINE_PATH'),
            'transition',
            env('RHEI_PLAN_PATH'),
            '--task',
            task + '.1',
            '--from',
            'review',
            '--to',
            'cancelled',
            '--result',
            'made unnecessary',
            '--no-callbacks',
        ],
        check=True,
    )

result('## Result\n\nTask {{}} finished {{}}.\n'.format(task, state))
"#,
        binary = serde_json::to_string(&rhei_binary().display().to_string())
            .expect("binary path should serialize"),
    );
    let (_dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-cancel-only",
        TWO_GATED_CHILDREN,
        brief_gated_machine(),
        &cancel_only,
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert!(
        !format!("{}{}", result.stdout, result.stderr).contains("released nothing"),
        "the cancel is the visit's act, so the self-loop fires; got:\n{}",
        result.stdout
    );
    // The self-loop fired: the visit is spent and the subtree released.
    assert_eq!(raw_state_line(&plan_path, "### Task 1: Supervise"), "supervising-2");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "cancelled");
}

/// A workspace an older `rhei` already stranded: the supervisor is `released`
/// over a subtree nothing can move. The rule above cannot form one any more,
/// but the halt has to stop advising the rerun that provably does nothing.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
#[test]
fn a_released_supervisor_over_a_blocked_subtree_is_named_as_unwakeable() {
    const STRANDED_PLAN: &str = r#"# Rhei: Already stranded

---
structure:
  maxLevels: 3
metadata:
  tasks:
    1:
      stateVisits:
        supervising: 2
      supervision:
        phase: released
---

## Tasks

### Task 1: Supervise
**State:** supervising-2

#### Task 1.1: Review
**State:** review
"#;
    let (_dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-already-stranded",
        STRANDED_PLAN,
        brief_gated_machine(),
        CONDITIONAL_BRIEF_AGENT,
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(!result.status.success(), "the run halts; got:\n{combined}");
    assert!(
        combined.contains("released its subtree on a visit that changed nothing"),
        "the halt says the supervisor cannot be woken; got:\n{combined}"
    );
    assert!(
        combined.contains("unblock one of the tickets above and rerun"),
        "and names the remedies that work; got:\n{combined}"
    );
    assert!(
        !combined.contains("Task plan.1 (supervising): not scheduled"),
        "never the rerun advice that cannot help; got:\n{combined}"
    );
}

/// A descendant parked at a human gate is waiting, not stranded: the human owns
/// the next move, so an otherwise empty visit still releases.
// §FS-rhei-supervision.3.6 rule 3
#[test]
fn a_descendant_at_a_human_gate_counts_as_able_to_move() {
    const GATED_CHILD_PLAN: &str = r#"# Rhei: Waiting on a human

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising

#### Task 1.1: Decide
**State:** human-review
"#;
    let (_dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-gated-descendant",
        GATED_CHILD_PLAN,
        brief_gated_machine(),
        CONDITIONAL_BRIEF_AGENT,
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert!(
        !format!("{}{}", result.stdout, result.stderr).contains("released nothing"),
        "a human owns the next move, so the run is waiting rather than stranded; got:\n{}",
        result.stdout
    );
    assert_eq!(raw_state_line(&plan_path, "### Task 1: Supervise"), "supervising-2");
}
