// The release edge reached without a visit: when a supervising state's declared
// `outputs:` are already on disk, `rhei run` has no invocation left to spawn and
// advances the ticket straight away. That advance fires the same self-loop, so
// it answers the same release test.
//
// Its own part because the door is the *absence* of a visit rather than the end
// of one, and because a held visit is what leaves those outputs behind: without
// this the fix set its own trap, holding on one run and releasing over the same
// blocked subtree on the next.

// §FS-rhei-supervision.3.6

use std::fs;

use super::supervision_tests::{assert_state_anywhere, setup_supervision_with_agent, spawn_log};
use super::*;

/// A brief-gated child under a supervisor whose own state declares an output.
/// The output is what routes the second run past the spawn.
fn declared_outputs_machine() -> &'static str {
    r#"name: supervision-no-spawn-release
version: 1
states:
  supervising:
    initial: true
    description: Supervise the subtree
    execute_on: child-terminal
    agent: mock
    agent_timeout: 30s
    visits: 12
    outputs:
      - name: note
        path: runtime/notes/{task_id}.md
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
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: review, to: completed, description: Findings written }
  - { from: "*", to: cancelled, description: Dropped }
"#
}

const ONE_CHILD_PLAN: &str = r#"# Rhei: Supervisor with declared outputs

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

/// A supervisor that meets its own completion condition and does nothing else:
/// it writes the note its state declares, and never the brief its child needs.
const NOTE_ONLY_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {} {}\n'.format(task, state, visit))

if state == 'supervising':
    write(root / 'runtime' / 'notes' / (task + '.md'), 'Noted.\n')
elif state == 'review':
    write(root / 'runtime' / 'review' / (task + '.md'), 'Findings from {}.\n'.format(task))

result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#;

/// The trap the hold used to set for itself. Run 1 spawns a visit, which writes
/// the state's declared output and nothing the child needs, so the visit is
/// held. Run 2 is the same command with no flags: the output is now on disk, so
/// no invocation is left to spawn and the ticket advances without a visit at
/// all. That advance is the release self-loop, and it must be held too — a
/// second run that released here would put `phase: released` over a subtree
/// still waiting on a brief nobody will write, which is the ticket's own end
/// state. Writing the brief is what lets the third run through.
// §FS-rhei-supervision.3.6
#[test]
fn an_advance_that_spawns_nothing_still_answers_the_release_test() {
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-no-spawn-release",
        ONE_CHILD_PLAN,
        declared_outputs_machine(),
        NOTE_ONLY_AGENT,
    );
    let before = fs::read_to_string(&plan_path).expect("read plan");
    let args = ["--no-callbacks", "--no-tui"];

    let first = run_cli("run", &plan_path, &machine_path, &args);
    let first_out = format!("{}{}", first.stdout, first.stderr);
    assert!(
        first_out.contains("the visit released nothing"),
        "the visit is held; got:\n{first_out}"
    );
    assert!(
        dir.join("runtime/notes/plan.1.md").exists(),
        "and it left the state's declared output behind, which is what routes run 2"
    );

    let second = run_cli("run", &plan_path, &machine_path, &args);
    let second_out = format!("{}{}", second.stdout, second.stderr);
    assert!(
        second_out.contains("the visit released nothing"),
        "the advance that spawns nothing is held on the same terms; got:\n{second_out}"
    );
    assert!(
        !second_out.contains("'supervising' \u{2192} 'supervising'"),
        "so the release self-loop never fires; got:\n{second_out}"
    );
    // But for the ticket's budget identity, which run 1's spawn earned by
    // reserving a unit against the project; run 2 spawned nothing and so wrote
    // nothing of its own. §FS-rhei-budgets.6.1
    super::budget_support::assert_plan_but_for_the_budget_identity(
        &fs::read_to_string(&plan_path).expect("read plan"),
        &before,
        "and two held runs rewrite nothing: no spent visit, no `phase: released`",
    );
    assert_eq!(
        spawn_log(&dir),
        vec!["plan.1 supervising 1".to_string()],
        "run 2 spawned nothing at all — the hold is the only reason it did not advance"
    );

    // And what a dry run says about the same advance is what the run does with
    // it, not the transition it will not make. §FS-rhei-run.4
    let dry_args = ["--no-callbacks", "--no-tui", "--dry-run"];
    let dry = run_cli("run", &plan_path, &machine_path, &dry_args);
    let dry_out = format!("{}{}", dry.stdout, dry.stderr);
    assert!(
        dry_out.contains("withheld: Task plan.1  supervising -> supervising"),
        "the dry run names the withheld edge; got:\n{dry_out}"
    );
    assert!(
        !dry_out.contains("would transition: Task plan.1"),
        "and does not promise the transition; got:\n{dry_out}"
    );

    fs::write(write_brief_dir(&dir).join("plan.1.1.md"), "Review the parser.\n")
        .expect("write brief");
    let third = run_cli("run", &plan_path, &machine_path, &args);
    let third_out = format!("{}{}", third.stdout, third.stderr);
    assert!(
        third_out.contains("'supervising' \u{2192} 'supervising'"),
        "with the child unblocked the same advance releases; got:\n{third_out}"
    );
    assert!(
        spawn_log(&dir).contains(&"plan.1.1 review 1".to_string()),
        "and the child finally runs; got:\n{:?}",
        spawn_log(&dir)
    );
    assert_success(&third);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
}

/// The guard on the other side: an advance that spawns nothing is not held for
/// spawning nothing. With the brief already there the subtree can move, so the
/// self-loop fires exactly as it did before this rule reached this path.
// §FS-rhei-supervision.3.6
#[test]
fn an_advance_that_spawns_nothing_releases_when_the_subtree_can_move() {
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-no-spawn-release-open",
        ONE_CHILD_PLAN,
        declared_outputs_machine(),
        NOTE_ONLY_AGENT,
    );
    // Both files up front: the note routes the run past the spawn, the brief
    // means the child can move the moment the barrier lifts.
    fs::write(write_notes_dir(&dir).join("plan.1.md"), "Noted.\n").expect("write note");
    fs::write(write_brief_dir(&dir).join("plan.1.1.md"), "Review the parser.\n")
        .expect("write brief");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("released nothing"),
        "the subtree can move, so nothing is withheld; got:\n{combined}"
    );
    assert!(
        combined.contains("'supervising' \u{2192} 'supervising'"),
        "the release self-loop fires; got:\n{combined}"
    );
    assert_eq!(
        spawn_log(&dir).first().map(String::as_str),
        Some("plan.1.1 review 1"),
        "and the release came of an advance that spawned nothing: the child ran first"
    );
}

/// `runtime/supervise/`, created.
fn write_brief_dir(dir: &TestDir) -> std::path::PathBuf {
    let path = dir.join("runtime/supervise");
    fs::create_dir_all(&path).expect("create brief dir");
    path
}

/// `runtime/notes/`, created.
fn write_notes_dir(dir: &TestDir) -> std::path::PathBuf {
    let path = dir.join("runtime/notes");
    fs::create_dir_all(&path).expect("create notes dir");
    path
}
