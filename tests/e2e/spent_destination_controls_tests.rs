//! Attribution controls: an exhausted destination is not enough by itself.
// §FS-rhei-errors.1.5.1 §FS-rhei-run-report.2

use super::spent_destination_support::{text, Worker, World, PATHS, REFUSAL};
use super::*;

// Raise only the destination limit for the probe: its earlier budget refusal
// otherwise masks the independently false guard. §FS-rhei-errors.1.5.1
fn assert_false_guard(world: &World, condition: &str) {
    world.change_machine("visits: 2", "visits: 3");
    for id in ["1", "2"] {
        let refused = world.cli(&[
            "transition",
            &format!("index.{id}"),
            "--from",
            "audit",
            "--to",
            "check-review",
        ]);
        assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
        assert!(
            text(&refused).contains(&format!("condition `{condition}` evaluated to false")),
            "the guard must independently refuse the edge: {}",
            text(&refused)
        );
    }
    world.change_machine("visits: 3", "visits: 2");
}

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
            "description: Audit written, condition: 1 < 0",
        );
        let later = world.run();
        assert_eq!(later.status.code(), Some(1));
        assert!(!text(&later).contains(REFUSAL), "{}", text(&later));
        for id in ["1", "2"] {
            assert!(!world.row(id).contains("visit budget"), "{}", world.row(id));
        }
        assert_false_guard(&world, "1 < 0");
    }
}

/// The worker changes a custom operand after entry selection has loaded it.
/// Both parallel workers reach the third audit before one updates the shared
/// metadata document; neither exits until that update is visible.
/// §FS-rhei-errors.1.5.1
#[test]
fn issue_764_worker_updated_guard_uses_completion_context() {
    for (worker, parallel) in PATHS {
        let world = World::new(worker, parallel, false);
        world.change_machine(
            "description: Audit written",
            "description: Audit written, condition: allowed == 1",
        );
        fs::write(
            world.dir.join("index.rhei.md"),
            "# Rhei: Spent destination\n\n---\nmetadata:\n  tasks:\n    1:\n      allowed: 1\n    2:\n      allowed: 1\n---\n",
        )
        .unwrap();
        let update = r#"def wait_for(paths):
    deadline = time.monotonic() + 60
    while not all(path.exists() for path in paths):
        if time.monotonic() >= deadline:
            raise RuntimeError('completion-guard barrier timed out')
        time.sleep(0.01)

if seen == 2:
    if parallel:
        write(root / 'runtime' / (task + '-guard-ready'), 'ready\n')
        wait_for([root / 'runtime' / ('index.' + id + '-guard-ready') for id in ['1', '2']])
    if not parallel or task == 'index.1':
        index = root / 'index.rhei.md'
        before = index.read_text(encoding='utf-8')
        after = before
        for id in (['1', '2'] if parallel else [task.rsplit('.', 1)[-1]]):
            old = '    ' + id + ':\n      allowed: 1\n'
            assert old in after
            after = after.replace(old, old.replace('allowed: 1', 'allowed: 0'))
        pending = root / 'guard-update.pending'
        write(pending, after)
        os.replace(pending, index)
        if parallel:
            write(root / 'runtime' / 'guard-updated', 'updated\n')
    if parallel:
        wait_for([root / 'runtime' / 'guard-updated'])
"#;
        fs::write(
            world.dir.join("audit.py"),
            format!(
                "{}parallel = {}\n{update}",
                world.read("audit.py"),
                if parallel > 1 { "True" } else { "False" }
            ),
        )
        .unwrap();

        let spending = world.run();
        world.assert_spent(&spending, true);
        assert!(!text(&spending).contains(REFUSAL), "{}", text(&spending));
        let logged_warnings: Vec<String> = world
            .read("runtime/events.jsonl")
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|event| event["event"] == "message" && event["level"] == "warn")
            .filter_map(|event| event["text"].as_str().map(str::to_owned))
            .collect();
        for id in ["1", "2"] {
            let warning = format!(
                "warning: {} exited 0 but task index.{id} did not advance from 'audit'",
                worker.name()
            );
            assert!(
                text(&spending).lines().any(|line| line.trim() == warning),
                "{}",
                text(&spending)
            );
            assert!(
                logged_warnings.iter().any(|line| line.trim() == warning),
                "{logged_warnings:?}"
            );
            assert!(!world.row(id).contains("visit budget"), "{}", world.row(id));
        }
        assert_eq!(world.read("index.rhei.md").matches("allowed: 0").count(), 2);
        assert!(
            logged_warnings.iter().all(|line| !line.contains("visit budget")),
            "{logged_warnings:?}"
        );
        assert_false_guard(&world, "allowed == 1");
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
