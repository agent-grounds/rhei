//! Controls for issue #770: the remedies the explanation states really move
//! the ticket, and a missing destination input is named only when it is what
//! refused the edge completion selected. These pass before the fix and after.
// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2

use super::refused_input_support::{text, Outputs, Worker, World};
use super::*;

fn assert_completed(world: &World, result: &CliRun) {
    assert_success(result);
    assert_eq!(world.state(), "completed", "{}", text(result));
    assert_eq!(world.calls("read"), 1, "'read' ran once, on the written note");
}

/// Write that file and rerun: the reused visit takes its edge without a spawn.
#[test]
fn issue_770_writing_the_input_releases_a_reused_visit() {
    let world = World::new(Worker::Agent, Outputs::Declared);
    world.assert_refused(&world.run());
    world.assert_refused(&world.run());
    world.write_note();
    assert_completed(&world, &world.run());
    assert_eq!(world.calls("draft"), 1, "the finished visit was reused");
}

/// Write that file and rerun: with attempts left, the rerun spawns the worker
/// again, as a state that owes no reusable output must be, and it advances.
#[test]
fn issue_770_writing_the_input_releases_a_visit_with_attempts_left() {
    for worker in [Worker::Agent, Worker::Program] {
        let world = World::new(worker, Outputs::Bare);
        world.assert_refused(&world.run());
        world.write_note();
        assert_completed(&world, &world.run());
        assert_eq!(world.calls("draft"), 2, "{worker:?}: attempt 2 ran and advanced");
    }
}

/// Once the attempts are spent, writing the file alone does not move the
/// ticket; writing it, then `rhei retry`, then a rerun does.
#[test]
fn issue_770_spent_visit_needs_the_input_then_a_retry() {
    let world = World::new(Worker::Agent, Outputs::Bare);
    for _ in 0..3 {
        world.assert_refused(&world.run());
    }
    assert_eq!(world.calls("draft"), 2);
    world.write_note();
    let unreleased = world.run();
    assert_eq!(unreleased.status.code(), Some(1), "{}", text(&unreleased));
    assert_eq!(world.state(), "draft", "the file alone leaves the spent visit halted");
    assert_eq!(world.calls("draft"), 2);
    assert_success(&world.cli(&["retry", "plan.1"]));
    assert_completed(&world, &world.run());
    assert_eq!(world.calls("draft"), 3, "the granted attempt ran and advanced");
}

/// Selection is unchanged: a later edge does not advance in place of the one
/// the destination refused. §FS-rhei-errors.1.5.2
#[test]
fn issue_770_a_later_edge_does_not_advance_in_place_of_the_refused_one() {
    let world = World::new(Worker::Agent, Outputs::Declared);
    world.change_machine(
        "  - { from: read, to: completed",
        "  - { from: draft, to: completed, description: Skip reading }\n  - { from: read, to: completed",
    );
    let refused = world.run();
    world.assert_refused(&refused);
    assert!(text(&refused).contains("cannot enter state read"), "{}", text(&refused));
}

/// An edge refused by its own guard is not refused by the input behind it.
#[test]
fn issue_770_false_guard_is_not_attributed_to_the_input() {
    let world = World::new(Worker::Agent, Outputs::Declared);
    world.change_machine("description: Drafted", "description: Drafted, condition: 1 < 0");
    for run in ["finishing run", "later run"] {
        let halted = world.run();
        world.assert_refused(&halted);
        assert!(!text(&halted).contains("note ("), "{run}: {}", text(&halted));
        let (reason, next) = world.row();
        assert!(!reason.contains("note"), "{run}: {reason} | {next}");
        assert!(!reason.contains("'read'"), "{run}: {reason} | {next}");
    }
}

/// Missing required outputs keep their priority: the worker owes its result,
/// so its completion condition is unmet and the input is not the reason.
#[test]
fn issue_770_missing_outputs_keep_their_priority() {
    let world = World::new(Worker::Agent, Outputs::Declared);
    let script = world.read("worker.py");
    let write = "write(root / 'runtime' / 'results'";
    assert!(script.contains(write));
    fs::write(
        world.dir.join("worker.py"),
        script.replace(write, &format!("if env('RHEI_STATE') != 'draft':\n    {write}")),
    )
    .unwrap();
    for (run, owed) in
        [("first run", "without result ("), ("spending run", "still unmet: result (")]
    {
        let halted = world.run();
        assert_eq!(halted.status.code(), Some(1), "{}", text(&halted));
        assert_eq!(world.state(), "draft");
        let (reason, next) = world.row();
        assert!(reason.contains(owed), "{run}: {reason} | {next}");
        assert!(!reason.contains("note"), "{run}: {reason} | {next}");
    }
}
