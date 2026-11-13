//! `rhei reset --task` under a supervisor: one it leaves out is held, so its
//! next visit briefs the reopened steps again, and one it reopens returns to
//! its authored state (agent-grounds/rhei#679).

// §FS-rhei-reset.2.5 §FS-rhei-reset.2.5.3 §FS-rhei-supervision.3.3 §FS-rhei-supervision.6

use std::fs;
use std::path::PathBuf;

use super::supervision_tests::{setup_supervision, supervision_machine};
use super::*;

/// A pre-authored chain: `implement`, then `review`, then `fix`.
const CHAIN: &str = "# Rhei: Fix

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Ticket
**State:** supervising

#### Task 1.1: Implement
**State:** fix

#### Task 1.2: Review
**State:** review
**Prior:** Task 1.1

#### Task 1.3: Fix
**State:** fix
**Prior:** Task 1.2
";

/// The chain with every step finished and the supervisor released.
fn finished_chain(name: &str) -> (TestDir, PathBuf, PathBuf) {
    let (dir, plan, machine) =
        setup_supervision(name, CHAIN, &supervision_machine("child-terminal", "completed"), "");
    let step = |args: &[&str]| {
        let run = run_cli(args[0], &plan, &machine, &args[1..]);
        assert_success(&run);
    };
    step(&["complete", "--task", "1.1", "--result", "implemented"]);
    fs::create_dir_all(dir.join("runtime/review")).expect("create the review dir");
    fs::write(dir.join("runtime/review/plan.1.2.md"), "findings\n").expect("findings");
    step(&["complete", "--task", "1.2", "--result", "reviewed"]);
    step(&["complete", "--task", "1.3", "--result", "fixed"]);
    step(&[
        "transition",
        "--task",
        "1",
        "--from",
        "supervising",
        "--to",
        "supervising",
        "--release-subtree",
    ]);
    (dir, plan, machine)
}

/// A released supervisor the scope leaves out is held, with a reset checkpoint
/// for each reopened step, and its state line is left as it was.
#[test]
fn a_supervisor_the_scope_leaves_out_is_held_with_reset_checkpoints() {
    let (dir, plan, machine) = finished_chain("reset-task-held-supervisor");
    let implement = fs::read_to_string(dir.join("runtime/results/plan.1.1.md")).expect("kept");

    let preview = run_cli("reset", &plan, &machine, &["--task", "plan.1.2", "--dry-run"]);
    assert_success(&preview);
    for line in [
        "  Task plan.1.2: completed → review",
        "  Task plan.1.3: completed → fix   (prior: plan.1.2)",
        "Would hold Task plan.1, which supervises them, so its next visit briefs them again.",
        "Kept 2 task(s) as they stand, with their results and logs: plan.1, plan.1.1",
    ] {
        assert!(preview.stdout.lines().any(|l| l == line), "{line}\nin:\n{}", preview.stdout);
    }

    let reset = run_cli("reset", &plan, &machine, &["--task", "plan.1.2", "--yes"]);
    assert_success(&reset);
    assert!(reset.stdout.contains("Held Task plan.1, which supervises them"), "{}", reset.stdout);

    let text = fs::read_to_string(&plan).expect("read the plan");
    assert!(text.contains("**State:** supervising-2"), "the supervisor's line stays:\n{text}");
    assert!(text.contains("phase: held"), "the supervisor is held:\n{text}");
    assert!(text.contains("reset: true"), "each checkpoint is marked a reset:\n{text}");
    assert!(text.contains("visit: 0"), "a reset is no counted visit:\n{text}");
    assert_task_state(&plan, &machine, "1.1", "completed");
    assert_eq!(
        fs::read_to_string(dir.join("runtime/results/plan.1.1.md")).expect("kept"),
        implement
    );
    assert_task_state(&plan, &machine, "1.2", "review");
    assert_task_state(&plan, &machine, "1.3", "fix");

    // §FS-rhei-supervision.3.4: the held supervisor is the ticket to work, and
    // its prompt tells the reset from a worker's regression.
    let next = run_cli("next", &plan, &machine, &["--task", "plan.1", "--peek", "--json"]);
    assert_success(&next);
    let payload: serde_json::Value = serde_json::from_str(&next.stdout).expect("json");
    let checkpoints = payload["checkpoints"].as_str().expect("a checkpoint section");
    assert!(
        checkpoints
            .contains("### Task plan.1.2: Review \u{2014} completed \u{2192} review (reset)"),
        "got: {checkpoints}"
    );
    assert!(checkpoints.contains("rhei reset --task"), "got: {checkpoints}");
}

/// A finished supervisor over the reopened step is taken along, alone, back to
/// `supervising`: no supervision block is already held, so it is not held again.
#[test]
fn a_finished_supervisor_is_reopened_and_not_held_a_second_time() {
    let (_dir, plan, machine) = finished_chain("reset-task-finished-supervisor");
    assert_success(&run_cli("complete", &plan, &machine, &["--task", "1", "--result", "shipped"]));

    let preview = run_cli("reset", &plan, &machine, &["--task", "plan.1.2", "--dry-run"]);
    assert_success(&preview);
    for line in [
        "  Task plan.1: completed → supervising   (finished parent of plan.1.2)",
        "  Task plan.1.2: completed → review",
        "  Task plan.1.3: completed → fix   (prior: plan.1.2)",
        "Kept 1 task(s) as they stand, with their results and logs: plan.1.1",
    ] {
        assert!(preview.stdout.lines().any(|l| l == line), "{line}\nin:\n{}", preview.stdout);
    }
    assert!(!preview.stdout.contains("Would hold"), "not held twice:\n{}", preview.stdout);
}
