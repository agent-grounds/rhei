// Exercise command refusals against real plans and spawn records. §FS-rhei-retry.2

mod retry_handlers {
    use super::super::*;
    use super::retry_operations::{charge, grant, plan};

    const MACHINE: &str = r#"name: retry-handlers
version: 1
states:
  work:
    initial: true
    description: Work
    attempts: 2
  program:
    description: Program
    attempts: 2
    program: echo work
  waiting:
    description: Poll
    poll: {interval: 1s, max_attempts: 2}
  completed:
    description: Done
    final: true
transitions:
  - {from: work, to: completed}
  - {from: program, to: completed}
  - {from: waiting, to: waiting, condition: attempts < maxAttempts}
  - {from: waiting, to: completed, condition: attempts >= maxAttempts}
"#;

    struct Fixture {
        _home: super::TempHome,
        dir: tempfile::TempDir,
        input: PathBuf,
        machine: PathBuf,
    }

    impl Fixture {
        fn new(state: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let input = dir.path().join("plan.rhei.md");
            let machine = dir.path().join("states.yaml");
            fs::write(
                &input,
                format!("# Rhei: Retry\n\n## Tasks\n\n### Task 1: Work\n**State:** {state}\n"),
            )
            .unwrap();
            fs::write(&machine, MACHINE).unwrap();
            Self { _home: super::TempHome::new(), dir, input, machine }
        }

        fn invoke(&self, worker: bool) -> MietteResult<()> {
            retry_command_with_worker(&self.input, &[], Some(&self.machine), "plan.1", worker)
        }

        // Install the same durable identity that production admission reserves against.
        // §FS-rhei-budgets.6.1 §FS-rhei-retry.2
        fn install_identity(
            &self,
            account: &Account,
            journal: &mut Journal,
            audit: &Audit,
        ) -> String {
            let loaded = load_plan(&self.input).unwrap();
            let mut identity = ticket_budget_identity(&self.input, &loaded, "plan.1").unwrap();
            let ticket = account.ticket_identity(&identity.settle(journal, "plan.1").unwrap());
            journal.bind_ticket(&ticket, "plan.1", &self.input, audit).unwrap();
            identity.commit().unwrap();
            assert!(journal.identity_installed(&ticket));
            ticket
        }

        fn refuses(&self, worker: bool, expected: &str) {
            let before = self.contents();
            let error = self.invoke(worker).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
            assert_eq!(self.contents(), before);
        }

        fn contents(&self) -> BTreeMap<PathBuf, Vec<u8>> {
            let mut out = BTreeMap::new();
            out.insert(self.input.clone(), fs::read(&self.input).unwrap());
            fn collect(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
                let Ok(entries) = fs::read_dir(dir) else { return };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect(&path, out);
                    } else {
                        out.insert(path.clone(), fs::read(path).unwrap());
                    }
                }
            }
            collect(&self.dir.path().join("runtime"), &mut out);
            out
        }
    }

    #[test]
    fn retry_refuses_attempts_left() {
        let fixture = Fixture::new("work");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        fixture.refuses(false, "attempts left");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        grant(&plan(fixture.dir.path(), "work").record, "work").unwrap();
        fixture.refuses(false, "attempts left");
    }

    #[test]
    fn retry_refuses_poll_or_terminal() {
        for (state, expected) in [("waiting", "poll"), ("completed", "terminal")] {
            Fixture::new(state).refuses(false, expected);
        }
    }

    #[test]
    fn retry_refuses_missing_current_record() {
        let fixture = Fixture::new("work");
        fixture.refuses(false, "spawn record");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        fs::write(fixture.dir.path().join("runtime/state-transitions.log"), "plan.1 work@work\n")
            .unwrap();
        fixture.refuses(false, "spawn record");
    }

    #[test]
    fn retry_refuses_worker() {
        let fixture = Fixture::new("work");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        fixture.refuses(true, "worker");
    }

    #[test]
    fn retry_handler_grants_ordinary_agent_and_program_visits_without_moving() {
        for (state, kind) in [("work", "agent"), ("program", "program")] {
            let fixture = Fixture::new(state);
            charge(&plan(fixture.dir.path(), state), state, kind);
            charge(&plan(fixture.dir.path(), state), state, kind);
            let before = fs::read(&fixture.input).unwrap();
            fixture.invoke(false).unwrap();
            let next = plan(fixture.dir.path(), state);
            assert_eq!(next.attempt, 3);
            assert_eq!(next.charged, 2);
            assert!(next.budget_spent(AttemptBudget::Visit(2)).is_none());
            assert_eq!(next.previous.unwrap().grants.len(), 1);
            assert_eq!(fs::read(&fixture.input).unwrap(), before);
            assert!(!fixture.dir.path().join("runtime/state-transitions.log").exists());
        }
    }

    // §FS-rhei-retry.4 §FS-rhei-budgets.5.2: reset removes allowances, no refund.
    #[test]
    fn reset_clears_grants_without_refunding_the_invocation_account() {
        let fixture = Fixture::new("work");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        fixture.invoke(false).unwrap();
        let audit = budget_audit("test invocation").unwrap();
        let (account, mut journal) = Account::establish(fixture.dir.path(), &audit).unwrap();
        let ticket = fixture.install_identity(&account, &mut journal, &audit);
        let bounds = rhei_core::budget::EffectiveBounds {
            transition_limit: 10,
            invocations_per_day: 1,
            spend_per_day: 400_000_000,
        };
        let arms = [Arm { attempt_identity: "charged-before-grant", descendant_envelope: 1 }];
        let execution_root = fixture.dir.path().to_string_lossy();
        let request = AdmissionRequest {
            ticket_identity: &ticket,
            display_id: "plan.1",
            project_label: "retry",
            execution_root: &execution_root,
            arms: &arms,
            parent_reservation: None,
            travel: false,
            spend_reserve_micro: 0,
            spend_currency: "USD",
        };
        let group = journal.reserve(&request, bounds, &audit).unwrap();
        journal.record_start(&group.reservation_ids[0], true, &audit).unwrap();
        let before = journal.snapshot().unwrap().lifetime_invocations.consumed;
        assert_eq!(before, 1);
        drop(journal);
        reset_command(&fixture.input, Some(&fixture.machine), &[], false, true).unwrap();
        assert!(spawn_records_for_state(&fixture.dir.path().join("runtime"), "plan.1", "work")
            .is_empty());
        assert_eq!(plan(fixture.dir.path(), "work").effective_allowance(2), 2);
        assert_eq!(
            account.open(false).unwrap().snapshot().unwrap().lifetime_invocations.consumed,
            before
        );
    }

    // §FS-rhei-budgets.4.2: available visit attempts cannot buy account capacity.
    #[test]
    fn invocation_account_refuses_a_granted_attempt_without_consuming_it() {
        let fixture = Fixture::new("work");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        charge(&plan(fixture.dir.path(), "work"), "work", "agent");
        let audit = budget_audit("test admitted invocation").unwrap();
        let (account, mut journal) = Account::establish(fixture.dir.path(), &audit).unwrap();
        let ticket = fixture.install_identity(&account, &mut journal, &audit);
        let bounds = rhei_core::budget::EffectiveBounds {
            transition_limit: 10,
            invocations_per_day: 1,
            spend_per_day: 400_000_000,
        };
        let arms = [Arm { attempt_identity: "before-grant", descendant_envelope: 1 }];
        let execution_root = fixture.dir.path().to_string_lossy();
        let request = AdmissionRequest {
            ticket_identity: &ticket,
            display_id: "plan.1",
            project_label: "retry",
            execution_root: &execution_root,
            arms: &arms,
            parent_reservation: None,
            travel: false,
            spend_reserve_micro: 0,
            spend_currency: "USD",
        };
        let group = journal.reserve(&request, bounds, &audit).unwrap();
        journal.record_start(&group.reservation_ids[0], true, &audit).unwrap();
        drop(journal);
        fixture.invoke(false).unwrap();
        let next = plan(fixture.dir.path(), "work");
        assert!(next.budget_spent(AttemptBudget::Visit(2)).is_none());
        let record_bytes = fs::read(&next.record).unwrap();
        let loaded = load_plan(&fixture.input).unwrap();
        let machine = super::machine_with_states(MACHINE);
        let task = find_task_by_id(&loaded.rhei.tasks, &parse_task_id("plan.1")).unwrap();
        let mut settings = RheiSettings::default();
        settings.machine_bounds.invocations_per_day = Some(1);
        let admission = budget_admit_spawn(
            &fixture.input,
            &loaded,
            fixture.dir.path(),
            &machine,
            &settings,
            task,
            "plan.1",
            "USD",
            &begin_budget_run(),
        )
        .unwrap();
        assert!(matches!(admission, BudgetAdmission::Refused { .. }));
        assert_eq!(fs::read(&next.record).unwrap(), record_bytes);
        assert_eq!(
            account.open(false).unwrap().snapshot().unwrap().lifetime_invocations.consumed,
            1
        );
    }
}
