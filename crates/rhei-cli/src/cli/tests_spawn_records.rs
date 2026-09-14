// The persistence under the attempt number: what makes two spawns one visit,
// what makes the next one a new visit, and what a spawn spends of its budget.
//
// Its own part because the end-to-end tests exercise this through `rhei run`,
// where a wrong answer shows up as a wrong log name three layers away.

// §AR-source-file-size.3 §FS-rhei-agents.8.4 §FS-rhei-agents.3.2.3

mod spawn_records {
    use super::super::*;

    /// One ticket's ledger, written where `ticket_move_count` reads it.
    fn ledger(root: &std::path::Path, lines: &str) {
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime).expect("runtime dir");
        fs::write(runtime.join("state-transitions.log"), lines).expect("ledger");
    }

    fn ended(plan: &SpawnPlan, ending: &str, code: i32) {
        plan.record_spawn(SpawnEnding {
            task_id: "plan.1",
            state_name: "implement",
            kind: "agent",
            worker: "mock",
            started: "2026-08-29T10:00:00Z",
            ended: "2026-08-29T10:00:01Z",
            duration: "1s",
            code: Some(code),
            ending,
        });
    }

    fn plan_for(root: &std::path::Path) -> SpawnPlan {
        plan_spawn_attempt(&root.join("runtime"), root, "plan.1", "implement", None)
    }

    /// Two spawns with the ticket standing still are two attempts at one visit:
    /// the second gets its own transcript rather than truncating the first, and
    /// it carries the first forward as what it is retrying.
    // §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
    #[test]
    fn a_second_spawn_without_a_move_is_the_second_attempt_of_one_visit() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger(dir.path(), "plan.1 draft@implement\n");

        let first = plan_for(dir.path());
        assert_eq!(first.attempt, 1);
        assert!(first.previous.is_none());
        assert!(first.log.ends_with("task-plan.1-implement.log"));
        ended(&first, "exited", 0);

        let second = plan_for(dir.path());
        assert_eq!(second.attempt, 2);
        assert!(second.log.ends_with("task-plan.1-implement-attempt2.log"));
        assert_eq!(
            second.previous.as_ref().map(SpawnRecord::ending_sentence).as_deref(),
            Some("exited 0 without meeting this state's completion condition")
        );
    }

    /// The ticket moving is what ends a visit. Everything about the next spawn
    /// starts over: the plain log name, no previous attempt to narrate, and a
    /// budget that has not been spent.
    // §FS-rhei-agents.8.1 §FS-rhei-agents.3.2.3
    #[test]
    fn a_move_starts_a_new_visit_with_a_fresh_name_and_budget() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger(dir.path(), "plan.1 draft@implement\n");
        let first = plan_for(dir.path());
        ended(&first, "exited", 0);
        let second = plan_for(dir.path());
        ended(&second, "exited", 0);
        assert_eq!(
            plan_for(dir.path()).budget_spent(AttemptBudget::Visit(2)),
            Some(2),
            "two recorded attempts spend a budget of two"
        );

        // The ticket leaves and comes back: one more ledger line either way.
        ledger(dir.path(), "plan.1 draft@implement\nplan.1 implement@review\nplan.1 review@implement\n");

        let after = plan_for(dir.path());
        assert_eq!(after.attempt, 1, "a fresh entry is not a third attempt at the last one");
        assert!(after.previous.is_none(), "and has nothing to narrate as a retry");
        assert!(after.log.ends_with("task-plan.1-implement.log"));
        assert!(
            after.budget_spent(AttemptBudget::Visit(2)).is_none(),
            "the budget came back with the visit"
        );
    }

    /// An interrupted invocation keeps its transcript — it ran — but the run
    /// ended it, so it is not an attempt the ticket spent. Without this, two
    /// Ctrl-Cs would halt a ticket that never had an attempt of its own.
    // §FS-rhei-run.3.2 §FS-rhei-agents.3.2.3
    #[test]
    fn an_interrupted_spawn_takes_an_attempt_log_but_not_a_budgeted_attempt() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger(dir.path(), "plan.1 draft@implement\n");

        let first = plan_for(dir.path());
        ended(&first, "interrupted", -1);
        let second = plan_for(dir.path());
        assert_eq!(second.attempt, 2, "the interrupted transcript is kept beside the retry");
        assert!(
            second.budget_spent(AttemptBudget::Visit(1)).is_none(),
            "but it did not spend the visit's only attempt"
        );

        ended(&second, "interrupted", -1);
        let third = plan_for(dir.path());
        assert!(
            third.budget_spent(AttemptBudget::Visit(1)).is_none(),
            "and neither did the next interruption"
        );

        ended(&third, "timed out", -1);
        let fourth = plan_for(dir.path());
        assert_eq!(
            fourth.budget_spent(AttemptBudget::Visit(1)),
            Some(1),
            "a timeout is the ticket's own attempt, and spends it"
        );
        assert_eq!(
            fourth.previous.as_ref().map(SpawnRecord::ending_sentence).as_deref(),
            Some("timed out after 1s")
        );
    }

    /// A state's account of its own worker is matched on the record's fields,
    /// never on the file name it happens to have: `review` and `review-fix`
    /// share a prefix, and one used to answer with the other's transcript.
    // §FS-rhei-agents.8.4
    #[test]
    fn a_state_never_answers_with_a_prefix_siblings_worker() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        let fix = SpawnPlan {
            log: runtime.join("logs").join("task-plan.1-review-fix.log"),
            record: spawn_record_path(&runtime, "plan.1", "review-fix", None),
            moves: 0,
            attempt: 1,
            accounting: None,
            charged: 0,
            previous: None,
        };
        fix.record_spawn(SpawnEnding {
            task_id: "plan.1",
            state_name: "review-fix",
            kind: "agent",
            worker: "mock",
            started: "2026-08-29T10:00:00Z",
            ended: "2026-08-29T10:00:01Z",
            duration: "1s",
            code: Some(0),
            ending: "exited",
        });

        assert!(
            newest_spawn_record_for_state(&runtime, "plan.1", "review").is_none(),
            "'review' had no worker of its own, whatever its neighbour is called"
        );
        assert_eq!(
            newest_spawn_record_for_state(&runtime, "plan.1", "review-fix")
                .map(|record| record.worker),
            Some("mock".to_string())
        );
    }
}
