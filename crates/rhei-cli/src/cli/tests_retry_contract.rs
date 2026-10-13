// The on-disk grant contract, exercised through existing production readers
// and charging. JSON injection is test scaffolding for the not-yet-existing
// operator grant interface, not an implementation of that interface.

// §FS-rhei-retry.2 §FS-rhei-agents.8.4 §FS-rhei-agents.3.2.3

mod retry_contract {
    use super::super::*;

    fn plan_for(root: &Path) -> SpawnPlan {
        plan_spawn_attempt(&root.join("runtime"), root, "plan.1", "work", None, LogNumber::Entry(1))
    }

    fn charge(plan: &SpawnPlan) {
        plan.record_spawn(SpawnEnding {
            task_id: "plan.1", state_name: "work", kind: "agent", worker: "mock",
            started: "2026-10-13T10:00:00Z", ended: "2026-10-13T10:00:01Z",
            duration: "1s", code: Some(1), ending: "exited",
        });
    }

    fn inject_grant(path: &Path) -> serde_json::Value {
        let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        value["grants"] = serde_json::json!([{"at": "2026-10-13T10:01:00Z", "os_user": "operator", "attempts": 2}]);
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        value["grants"].clone()
    }

    #[test]
    fn granted_budget_is_read_from_record_without_changing_attempt_or_visit() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan_for(dir.path()));
        charge(&plan_for(dir.path()));
        let exhausted = plan_for(dir.path());
        assert_eq!(exhausted.budget_spent(AttemptBudget::Visit(2)), Some(2));
        inject_grant(&exhausted.record);
        let granted = plan_for(dir.path());
        assert_eq!(granted.attempt, 3);
        assert_eq!(granted.moves, exhausted.moves);
        assert_eq!(granted.charged, 2);
        assert_eq!(granted.budget_spent(AttemptBudget::Visit(2)), None, "grant admits the third attempt");
        assert!(matches!(granted.retry_outlook(AttemptBudget::Visit(2)), RetryOutlook::AttemptsLeft));
    }

    /// Deterministic interleaving: plan a charge, commit a grant, then finish
    /// that charge. Both audited grant and charged counter must survive. The
    /// real grant API/lock seam must replace injection when it exists.
    // §FS-rhei-retry.2 §FS-rhei-agents.8.4
    #[test]
    fn interleaved_grant_and_charge_preserve_both_in_a_readable_record() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan_for(dir.path()));
        charge(&plan_for(dir.path()));
        let pending_charge = plan_for(dir.path());
        let grants = inject_grant(&pending_charge.record);
        charge(&pending_charge);
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&pending_charge.record).unwrap()).expect("whole readable record");
        assert_eq!(value["charged"], 3, "charge survives");
        assert_eq!(value["grants"], grants, "grant survives a charge planned before it");
    }

    #[test]
    fn legacy_record_without_grants_has_only_configured_budget() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan_for(dir.path()));
        charge(&plan_for(dir.path()));
        assert_eq!(plan_for(dir.path()).budget_spent(AttemptBudget::Visit(2)), Some(2));
    }

    #[test]
    fn grant_allows_exactly_one_more_configured_budget() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan_for(dir.path()));
        charge(&plan_for(dir.path()));
        inject_grant(&plan_for(dir.path()).record);
        for charged in 2..4 {
            let next = plan_for(dir.path());
            assert_eq!(next.charged, charged);
            assert_eq!(next.budget_spent(AttemptBudget::Visit(2)), None);
            charge(&next);
        }
        let spent = plan_for(dir.path());
        assert_eq!(spent.attempt, 5);
        assert!(spent.budget_spent(AttemptBudget::Visit(2)).is_some(), "four charged attempts exhaust configured two plus granted two");
    }

    #[test]
    fn grants_from_an_older_visit_do_not_raise_the_current_visit_budget() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan_for(dir.path()));
        charge(&plan_for(dir.path()));
        inject_grant(&plan_for(dir.path()).record);
        fs::write(dir.path().join("runtime/state-transitions.log"), "plan.1 work@work\n").unwrap();
        let new_visit = plan_for(dir.path());
        assert_eq!(new_visit.attempt, 1);
        assert_eq!(new_visit.charged, 0);
        assert!(new_visit.previous.is_none());
        charge(&new_visit);
        charge(&plan_for(dir.path()));
        assert_eq!(plan_for(dir.path()).budget_spent(AttemptBudget::Visit(2)), Some(2));
    }

    // §FS-rhei-retry.3 §FS-rhei-agents.5.2.1
    #[test]
    fn ordinary_halt_preserves_existing_ending_and_appends_retry_command() {
        let line = budget_spent_halt_line("plan.1", "work", 2, "result missing");
        assert!(line.contains("The ticket stays in 'work'."));
        assert!(line.ends_with("To give this visit 2 more attempts, run: rhei retry plan.1."), "{line}");
    }

    // §FS-rhei-retry.3
    #[test]
    fn poll_budget_still_uses_poll_exhaustion_without_visit_retry_advice() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan_for(dir.path()));
        charge(&plan_for(dir.path()));
        let poll = plan_for(dir.path());
        assert_eq!(poll.budget_spent(AttemptBudget::Poll { max_attempts: 2 }), None);
        assert!(matches!(poll.retry_outlook(AttemptBudget::Poll { max_attempts: 2 }), RetryOutlook::AttemptsLeft));
        assert!(!poll.respawn_note("plan.1", "waiting", AttemptBudget::Poll { max_attempts: 2 }).unwrap().contains("rhei retry"));
    }
}
