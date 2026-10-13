// Production record transactions, including deterministic lock contention.
// §FS-rhei-retry.2 §FS-rhei-agents.8.4

mod retry_operations {
    use super::super::*;

    pub(super) fn plan(root: &Path, state: &str) -> SpawnPlan {
        plan_spawn_attempt(&root.join("runtime"), root, "plan.1", state, None, LogNumber::Entry(1))
    }

    pub(super) fn charge(plan: &SpawnPlan, state: &str, kind: &str) {
        plan.record_spawn(SpawnEnding {
            task_id: "plan.1",
            state_name: state,
            kind,
            worker: "mock",
            started: "2026-10-13T10:00:00Z",
            ended: "2026-10-13T10:00:01Z",
            duration: "1s",
            code: Some(1),
            ending: "exited",
        });
    }

    pub(super) fn grant(path: &Path, state: &str) -> MietteResult<SpawnRecord> {
        grant_retry_record(
            path,
            "plan.1",
            state,
            0,
            AttemptGrant {
                at: "2026-10-13T10:01:00Z".into(),
                os_user: "operator".into(),
                attempts: 2,
            },
        )
    }

    // §FS-rhei-agents.8.4: charge replaces malformed JSON; grants cannot trust it.
    #[test]
    fn completed_charge_replaces_malformed_record_and_accounts_for_the_next_log() {
        let dir = tempfile::tempdir().unwrap();
        let first = plan(dir.path(), "work");
        fs::create_dir_all(first.record.parent().unwrap()).unwrap();
        fs::write(&first.record, "{broken").unwrap();
        assert!(grant(&first.record, "work").is_err());
        assert_eq!(fs::read_to_string(&first.record).unwrap(), "{broken");
        fs::create_dir_all(first.log.parent().unwrap()).unwrap();
        fs::write(&first.log, "completed worker transcript").unwrap();
        charge(&first, "work", "program");
        let stored = read_spawn_record(&first.record).unwrap();
        assert_eq!(stored.attempt, 1);
        assert_eq!(stored.charged, 1);
        assert!(stored.grants.is_empty());
        let next = plan(dir.path(), "work");
        assert_eq!(next.attempt, 2);
        assert_eq!(next.charged, 1);
        assert!(next.unaccounted_log_refusal("plan.1").is_none());
        charge(&next, "work", "program");
        assert_eq!(plan(dir.path(), "work").budget_spent(AttemptBudget::Visit(2)), Some(2));
    }

    // §FS-rhei-agents.8.4: these are real grant/charge operations, not injected JSON.
    #[test]
    fn retry_grant_and_attempt_charge_share_lock_and_atomic_record_write() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan(dir.path(), "work"), "work", "agent");
        charge(&plan(dir.path(), "work"), "work", "agent");
        let pending = plan(dir.path(), "work");
        let path = pending.record.clone();
        let old = fs::read(&path).unwrap();
        let (staged_tx, staged_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let grant_path = path.clone();
        let granter = std::thread::spawn(move || {
            SPAWN_RECORD_REPLACE_OBSERVER.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move |path, staged| {
                    let runtime = path.parent().unwrap().parent().unwrap();
                    let published = spawn_records_for_state(runtime, "plan.1", "work");
                    assert_eq!(published.len(), 1, "the staging file is not a record");
                    assert!(published[0].1.grants.is_empty(), "the grant is not published yet");
                    staged_tx
                        .send((fs::read(path).unwrap(), fs::read_to_string(staged).unwrap()))
                        .unwrap();
                    resume_rx.recv_timeout(Duration::from_secs(30)).unwrap();
                }))
            });
            grant(&grant_path, "work").unwrap()
        });
        let (before, staged) = staged_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        assert_eq!(before, old, "staging cannot expose partial JSON");
        let staged: serde_json::Value = serde_json::from_str(&staged).unwrap();
        assert_eq!(staged["grants"][0]["attempts"], 2);
        assert_eq!(staged["charged"], 2);

        let (lock_tx, lock_rx) = mpsc::channel();
        let (charge_tx, charge_rx) = mpsc::channel();
        let charger = std::thread::spawn(move || {
            set_plan_lock_observer(lock_tx);
            SPAWN_RECORD_REPLACE_OBSERVER.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move |path, staged| {
                    charge_tx
                        .send((
                            fs::read_to_string(path).unwrap(),
                            fs::read_to_string(staged).unwrap(),
                        ))
                        .unwrap();
                }))
            });
            charge(&pending, "work", "agent");
        });
        assert_eq!(
            lock_rx.recv_timeout(Duration::from_secs(30)).unwrap(),
            PlanLockEvent::Contended,
            "charge must wait for the grant's lock"
        );
        assert_eq!(fs::read(&path).unwrap(), old);
        resume_tx.send(()).unwrap();
        let granted = granter.join().unwrap();
        assert_eq!(granted.attempt, 2);
        assert_eq!(lock_rx.recv_timeout(Duration::from_secs(30)).unwrap(), PlanLockEvent::Acquired);
        charger.join().unwrap();
        let (before_charge, staged_charge) =
            charge_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        let before_charge: serde_json::Value = serde_json::from_str(&before_charge).unwrap();
        let staged_charge: serde_json::Value = serde_json::from_str(&staged_charge).unwrap();
        assert_eq!(before_charge, staged, "reader at charge replacement sees the whole grant");
        assert_eq!(staged_charge["charged"], 3);
        assert_eq!(staged_charge["grants"], staged["grants"]);
        let after: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(after, staged_charge, "after replacement reader sees the whole charge");
    }

    // §FS-rhei-retry.2: the same grant applies to both worker kinds, repeatedly.
    #[test]
    fn ordinary_agent_and_program_grants_sum_only_when_exhausted() {
        for kind in ["agent", "program"] {
            let dir = tempfile::tempdir().unwrap();
            for total in [2, 4, 6] {
                while plan(dir.path(), "work").charged < total {
                    charge(&plan(dir.path(), "work"), "work", kind);
                }
                let spent = plan(dir.path(), "work");
                assert_eq!(spent.budget_spent(AttemptBudget::Visit(2)), Some(total));
                let grant = grant(&spent.record, "work").unwrap();
                assert_eq!(grant.allowance(2), total + 2);
                assert_eq!(grant.kind, kind);
                assert_eq!(grant.attempt, total);
                assert_eq!(plan(dir.path(), "work").attempt, total + 1);
                assert!(plan(dir.path(), "work").budget_spent(AttemptBudget::Visit(2)).is_none());
                let bytes = fs::read(&spent.record).unwrap();
                assert!(self::grant(&spent.record, "work")
                    .unwrap_err()
                    .to_string()
                    .contains("attempts left"));
                assert_eq!(fs::read(&spent.record).unwrap(), bytes);
            }
        }
    }

    // §FS-rhei-agents.3.2.3 §FS-rhei-supervision.3.6: non-charges retain grants.
    #[test]
    fn withheld_and_interrupted_attempts_preserve_grants() {
        let dir = tempfile::tempdir().unwrap();
        charge(&plan(dir.path(), "work"), "work", "agent");
        charge(&plan(dir.path(), "work"), "work", "agent");
        let granted = grant(&plan(dir.path(), "work").record, "work").unwrap();
        let third = plan(dir.path(), "work");
        charge(&third, "work", "agent");
        uncharge_withheld_visit(&third.record);
        let after = read_spawn_record(&third.record).unwrap();
        assert_eq!(after.charged, 2);
        assert_eq!(after.grants.len(), granted.grants.len());
        charge(&plan(dir.path(), "work"), "work", "agent");
        uncharge_stopped_attempt(&third.record);
        let after = read_spawn_record(&third.record).unwrap();
        assert_eq!(after.charged, 2);
        assert_eq!(after.grants.len(), 1);
    }
}
