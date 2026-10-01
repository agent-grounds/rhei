// The rest of the barrier under `rhei run`: what a supervisor may do to its
// subtree during a visit, how a nested one behaves, what `--parallel` drains,
// and which descendant a checkpoint names. What the manual surfaces say about
// a held ticket lives next door in `supervision_surfaces_tests.rs`.

// §FS-rhei-supervision

use std::fs;

use super::supervision_tests::{
    assert_state_anywhere, prompt_for, setup_supervision, spawn_log, supervision_machine,
    REVIEW_FIX_PLAN,
};
use super::*;

pub const TWO_CHILD_PLAN: &str = r#"# Rhei: Harden

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Harden the parser
**State:** supervising

#### Task 1.1: Review parser
**State:** review

#### Task 1.2: Fix findings
**State:** fix
"#;

/// §FS-rhei-supervision.2.1 and §6: a supervisor cancels a step during its own
/// visit; that move is its own doing, so it is not a checkpoint, and the plan
/// it left behind is what `openDescendants` reads on the way out.
#[test]
fn a_step_the_supervisor_cancels_is_not_reported_back_to_it() {
    // The binary path is JSON-encoded rather than pasted: on Windows it is
    // full of backslashes, which a plain Python literal would read as escapes.
    let cancel = r#"    if visit == '1':
        import subprocess

        subprocess.run(
            [
                RHEI_BIN,
                '--state-machine',
                env('RHEI_STATE_MACHINE_PATH'),
                'transition',
                env('RHEI_PLAN_PATH'),
                '--task',
                task + '.2',
                '--from',
                'fix',
                '--to',
                'cancelled',
                '--result',
                'made unnecessary',
                '--supervisor',
                task,
                '--no-callbacks',
            ],
            check=True,
        )"#
    .replace(
        "RHEI_BIN",
        &serde_json::to_string(&rhei_binary().to_string_lossy()).expect("binary path json"),
    );
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-cancel",
        TWO_CHILD_PLAN,
        &supervision_machine("descendant-terminal", "completed"),
        &cancel,
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    assert_state_anywhere(&plan_path, &machine_path, "1.2", "cancelled");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
        ],
        "the cancelled step never runs, and the supervisor is woken once for the one that did"
    );

    let second = prompt_for(&dir, "plan.1", "supervising", 2);
    assert!(second.contains("### Task plan.1.1:"), "got:\n{second}");
    assert!(
        !second.contains("### Task plan.1.2:"),
        "the supervisor's own cancel is not news for it; got:\n{second}"
    );
}

/// §FS-rhei-supervision.2.2: a checkpoint reaches exactly one task — the
/// nearest supervising ancestor — and the inner supervisor's own terminal exit
/// is what the outer one hears.
#[test]
fn a_nested_supervisor_hears_its_own_subtree_and_reports_upward_when_it_finishes() {
    let plan = r#"# Rhei: Nested

---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Top
**State:** supervising

#### Task 1.1: Middle
**State:** supervising

##### Task 1.1.1: Leaf
**State:** review
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-nested",
        plan,
        &supervision_machine("descendant-terminal", "completed"),
        "",
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1.1", "completed");
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 supervising 1".to_string(),
            "plan.1.1.1 review 1".to_string(),
            "plan.1.1 supervising 2".to_string(),
            "plan.1 supervising 2".to_string(),
        ],
        "the outer supervisor releases the inner one, which releases the leaf"
    );

    let inner = prompt_for(&dir, "plan.1.1", "supervising", 2);
    assert!(
        inner.contains("### Task plan.1.1.1:"),
        "the leaf checkpoints its nearest; got:\n{inner}"
    );
    let outer = prompt_for(&dir, "plan.1", "supervising", 2);
    assert!(
        outer.contains("### Task plan.1.1: Middle \u{2014} supervising \u{2192} completed"),
        "the outer supervisor hears only the inner one's own exit; got:\n{outer}"
    );
    assert!(
        !outer.contains("### Task plan.1.1.1:"),
        "an ancestor farther up sees nothing of the leaf; got:\n{outer}"
    );
}

/// §FS-rhei-supervision.3.1: under `--parallel` a checkpoint is a drain —
/// siblings already running finish, nothing new starts, and the supervisor sees
/// every checkpoint they produced in one visit.
#[test]
fn a_checkpoint_drains_the_parallel_siblings_before_the_supervisor_runs() {
    let machine = supervision_machine("descendant-terminal", "completed")
        .replace("  review:\n", "  review:\n    concurrent: true\n")
        .replace("  fix:\n", "  fix:\n    concurrent: true\n");
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-parallel", TWO_CHILD_PLAN, &machine, "");

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-callbacks", "--no-tui", "--parallel", "2"],
    );
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "completed");
    let log = spawn_log(&dir);
    assert_eq!(log.first().map(String::as_str), Some("plan.1 supervising 1"));
    assert_eq!(log.last().map(String::as_str), Some("plan.1 supervising 2"));
    assert_eq!(log.len(), 4, "both children run inside the one release; got:\n{log:?}");

    // §FS-rhei-supervision.3.3: the checkpoints accumulate and one visit
    // consumes them all.
    let second = prompt_for(&dir, "plan.1", "supervising", 2);
    assert!(second.contains("### Task plan.1.1:"), "got:\n{second}");
    assert!(second.contains("### Task plan.1.2:"), "got:\n{second}");
}

/// A checkpoint names one descendant exactly, and spells it the way
/// `## Child Tasks` and `rhei transition` do.
///
/// The ids here collide on the tail — the sibling `1.2` beside the nested
/// `1.1.2` — which is the case a suffix match resolves to the wrong task and
/// then shows the supervisor that task's title and result.
// §FS-rhei-supervision.5.1
#[test]
fn a_checkpoint_names_the_descendant_that_moved_not_one_whose_id_ends_the_same() {
    let plan = r#"# Rhei: Nested

---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Outer
**State:** supervising

#### Task 1.1: Inner supervisor
**State:** supervising

##### Task 1.1.1: A
**State:** fix

##### Task 1.1.2: B
**State:** fix
**Prior:** Task 1.1.1

#### Task 1.2: Sibling
**State:** fix
**Prior:** Task 1.1
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-tail-collision",
        plan,
        &supervision_machine("descendant-terminal", "completed"),
        "",
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    let outer = prompt_for(&dir, "plan.1", "supervising", 3);
    assert!(
        outer.contains("### Task plan.1.2: Sibling \u{2014} fix \u{2192} completed (visit 1)"),
        "the checkpoint is the sibling, titled and qualified as such; got:\n{outer}"
    );
    assert!(
        outer.contains("Task plan.1.2 finished fix."),
        "and it carries the sibling's own result; got:\n{outer}"
    );
    // Everything above `## Plan History` is this task's own inputs, checkpoints
    // included; the history below it names the finished children of this task
    // on purpose, so the collision check stops there. §FS-rhei-memory.3.2
    let inputs = outer.split("\n## Plan History").next().unwrap_or(outer.as_str());
    assert!(
        !inputs.contains("Task plan.1.1.2 finished fix."),
        "the tail-colliding cousin's result is not the sibling's; got:\n{outer}"
    );
}

/// A supervisor that leaves for a human gate keeps its subtree held.
///
/// Before: the `supervision` block was dropped on any non-self-loop exit, so
/// exhausting the visit budget into a gating state silently un-supervised the
/// subtree — the remaining children ran to completion with nobody watching,
/// which is the opposite of what the budget is for.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.2 §FS-rhei-supervision.3.3
#[test]
fn a_supervisor_parked_at_a_human_gate_still_holds_its_subtree() {
    let machine = supervision_machine("descendant-terminal", "completed")
        .replace("    visits: 12\n", "    visits: 1\n");
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-gate-hold", REVIEW_FIX_PLAN, &machine, "");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert!(
        result.stderr.contains(
            "Task plan.1 left supervision for human gate 'human-review'; its subtree stays \
             held until a human moves it"
        ),
        "the one transition where the barrier outlives the state says so:\n{}",
        result.stderr
    );

    // The children never ran: the budget ran out, so nothing beneath the
    // supervisor may move until a human decides.
    assert_task_state(&plan_path, &machine_path, "1", "human-review");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "review");
    assert_state_anywhere(&plan_path, &machine_path, "1.2", "fix");
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    assert!(plan.contains("phase: held"), "the block survives the move:\n{plan}");

    // §FS-rhei-supervision.3.4: and the refusal names the human, not a visit
    // that is never coming.
    let held = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert!(!held.status.success(), "a held descendant is still not claimable");
    assert!(
        held.stderr.contains("is at a human gate and still holds this subtree"),
        "got:\n{}",
        held.stderr
    );

    // §FS-rhei-run-report.3.1: the held ticket explains itself under Waiting,
    // and the row a person must act on is the gate, alone in Attention.
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    let attention = report
        .split("## Attention")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("an Attention section");
    assert!(attention.contains("| plan.1 |"), "got:\n{attention}");
    assert!(
        !attention.contains("| plan.1.1 |") && !attention.contains("| plan.1.2 |"),
        "held tickets do not dilute Attention:\n{attention}"
    );
    let waiting = report
        .split("## Waiting")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("a Waiting section");
    assert!(
        waiting.contains("| plan.1.1 |") && waiting.contains("held by supervisor Task plan.1"),
        "got:\n{waiting}"
    );
}

/// agent-grounds/rhei#122: a visit's subprocess exiting non-zero must not release
/// the subtree, burn the visit, or take a forward edge — in particular not
/// `openDescendants < 1` -> `completed`, which this scenario makes available
/// the moment the visit starts (its only child is already terminal). A
/// second `rhei run` on the halted workspace re-spawns the same visit, and
/// once that visit succeeds the retained checkpoint is consumed normally.
// §FS-rhei-supervision.3.5 §FS-rhei-programs.3.2
#[test]
fn a_failed_visit_keeps_the_hold_and_a_rerun_respawns_it() {
    let plan = r#"# Rhei: Failed Visit Rerun

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Harden the parser
**State:** supervising

#### Task 1.1: Review parser
**State:** review
"#;
    // Visit 2 is the child-terminal wake for 1.1; its first spawn fails, its
    // second (the rerun) succeeds — the marker file tells them apart, since a
    // failed visit is not supposed to advance `RHEI_VISIT_COUNT`.
    let fail_first_spawn_of_visit_two = r#"    if visit == '2':
        marker = root / 'runtime' / 'visit-2-attempts'
        attempts = (int(marker.read_text()) if marker.exists() else 0) + 1
        marker.write_text(str(attempts))
        if attempts == 1:
            sys.exit(1)"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-failed-visit-rerun",
        plan,
        &supervision_machine("child-terminal", "completed"),
        fail_first_spawn_of_visit_two,
    );

    let first = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert!(
        !first.status.success(),
        "no exit_code transition matches the failure, so the run halts; got:\nstdout:\n{}\nstderr:\n{}",
        first.stdout,
        first.stderr
    );
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
        ],
        "the failed visit is spawned once and nothing fires past it"
    );
    assert_task_state(&plan_path, &machine_path, "1", "supervising-2");

    let plan_after_failure = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        plan_after_failure.contains("phase: held"),
        "a failed visit is not the release edge:\n{plan_after_failure}"
    );
    assert!(
        !plan_after_failure.contains("phase: released"),
        "the release edge must not have fired:\n{plan_after_failure}"
    );
    assert!(
        plan_after_failure.contains("supervising: 2"),
        "the failed visit did not burn a visit's worth of stateVisits:\n{plan_after_failure}"
    );
    // The whole entry, not two independent substrings, so a corrupted or
    // wrong `task:`/`visit:` field elsewhere in the block would also fail
    // this assertion. §FS-rhei-supervision.3.3
    assert!(
        plan_after_failure.contains(
            "checkpoints:\n        - task: '1.1'\n          from: review\n          to: completed\n          visit: 1"
        ),
        "the checkpoint delivered before the failure survives it verbatim:\n{plan_after_failure}"
    );

    // The ticket's headline: a second `rhei run` re-spawns the same visit
    // rather than reporting nothing left to schedule.
    let second = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&second);
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
            "plan.1 supervising 2".to_string(),
        ],
        "the rerun re-spawns visit 2, not a fresh visit 3"
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    // The re-spawned visit's own prompt still carries the checkpoint that
    // was retained across the failure. §FS-rhei-supervision.5.1
    let retry_prompt = prompt_for(&dir, "plan.1", "supervising", 2);
    assert!(
        retry_prompt.contains(
            "### Task plan.1.1: Review parser \u{2014} review \u{2192} completed (visit 1)"
        ),
        "got:\n{retry_prompt}"
    );
}
