//! Attribution controls: an exhausted destination is not enough by itself.
// §FS-rhei-errors.1.5.1 §FS-rhei-run-report.2

use super::spent_destination_support::{text, Worker, World, PATHS, REFUSAL};
use super::*;

#[test]
fn issue_764_applicable_alternative_still_advances() {
    for (worker, parallel) in PATHS {
        let world = World::new(worker, parallel, false);
        world.change_machine("    initial: true", "    initial: true\n    visits: 4");
        world.change_machine(
            "  - { from: \"*\", to: cancelled",
            "  - { from: audit, to: done, description: Alternative, condition: visitCount >= 3 }\n  - { from: \"*\", to: cancelled",
        );
        let advanced = world.run();
        assert_success(&advanced);
        assert!(!text(&advanced).contains(REFUSAL), "{}", text(&advanced));
        for id in ["1", "2"] {
            assert!(world.read(&format!("tasks/{id}.md")).contains("**State:** done"));
            assert_eq!(
                world.read(&format!("runtime/index.{id}-review-calls")),
                "success\nsuccess\n"
            );
            assert_eq!(
                world.read(&format!("runtime/index.{id}-audit-calls")),
                "success\nsuccess\nsuccess\n"
            );
        }
    }
}

#[test]
fn issue_764_independently_false_guard_is_not_a_spent_budget_halt() {
    for (worker, parallel) in PATHS {
        let world = World::new(worker, parallel, false);
        world.assert_spent(&world.run(), true);
        world.change_machine(
            "description: Audit written",
            "description: Audit written, condition: visitCount < 1",
        );
        let later = world.run();
        assert_eq!(later.status.code(), Some(1));
        assert!(!text(&later).contains(REFUSAL), "{}", text(&later));
        for id in ["1", "2"] {
            assert!(!world.row(id).contains("visit budget"), "{}", world.row(id));
        }
    }
}

#[test]
fn issue_764_spent_invocation_attempts_keep_their_retry_remedy() {
    for (worker, parallel) in PATHS {
        let world = World::new(worker, parallel, true);
        world.change_machine("    initial: true", "    initial: true\n    attempts: 1");
        world.assert_spent(&world.run(), false);
        let later = world.run();
        world.assert_spent(&later, false);
        for id in ["1", "2"] {
            let row = world.row(id);
            assert!(row.contains("1 attempts spent") && row.contains("rhei retry"), "{row}");
            assert!(!row.contains("visit budget"), "{row}");
        }
    }
}

#[test]
fn issue_764_false_guard_on_completion_is_not_attributed_to_visits() {
    for (worker, parallel) in PATHS {
        let world = World::new(worker, parallel, false);
        world.change_machine("    initial: true", "    initial: true\n    visits: 4");
        world.change_machine(
            "description: Audit written",
            "description: Audit written, condition: visitCount < 3",
        );
        let spending = world.run();
        assert_eq!(spending.status.code(), Some(1));
        assert!(!text(&spending).contains(REFUSAL), "{}", text(&spending));
        for id in ["1", "2"] {
            assert!(world.read(&format!("tasks/{id}.md")).contains("**State:** audit-3"));
            assert_eq!(
                world.read(&format!("runtime/index.{id}-review-calls")),
                "success\nsuccess\n"
            );
            assert!(!world.row(id).contains("visit budget"), "{}", world.row(id));
        }
    }
}

#[test]
fn issue_764_program_exit_mismatch_is_not_a_spent_budget_halt() {
    for parallel in [1, 2] {
        let world = World::new(Worker::Program, parallel, false);
        world.assert_spent(&world.run(), true);
        world.change_machine(
            "description: Audit written, exit_code: 0",
            "description: Audit written, exit_code: 1",
        );
        let later = world.run();
        assert_eq!(later.status.code(), Some(1));
        assert!(!text(&later).contains(REFUSAL), "{}", text(&later));
        for id in ["1", "2"] {
            assert!(!world.row(id).contains("visit budget"), "{}", world.row(id));
        }
    }
}

#[test]
fn issue_764_missing_program_outputs_are_not_attributed_to_visits() {
    for parallel in [1, 2] {
        let world = World::new(Worker::Program, parallel, true);
        let spending = world.run();
        world.assert_spent(&spending, false);
        assert!(!text(&spending).contains(REFUSAL), "{}", text(&spending));
        for id in ["1", "2"] {
            assert!(!world.row(id).contains("visit budget"), "{}", world.row(id));
        }
    }
}

#[test]
fn issue_764_missing_agent_outputs_keep_their_diagnostic_priority() {
    for parallel in [1, 2] {
        let world = World::new(Worker::Agent, parallel, true);
        let spending = world.run();
        world.assert_spent(&spending, false);
        assert!(text(&spending).contains("required outputs are missing"), "{}", text(&spending));
        assert!(!text(&spending).contains(REFUSAL), "{}", text(&spending));
        for id in ["1", "2"] {
            let row = world.row(id);
            assert!(
                row.contains("audit")
                    && (row.contains("worker exited 0 without")
                        || row.contains("attempts spent") && row.contains("still unmet")),
                "{row}"
            );
            assert!(!row.contains("visit budget"), "{row}");
        }
    }
}
