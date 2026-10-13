// The recovery handler and run agree on runtime and own-record precedence.
// §FS-rhei-retry.2 §FS-rhei-retry.3
mod retry_discovery {
    use super::super::*;

    const MACHINE: &str = r#"name: retry-runtime
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
  done:
    description: Done
    final: true
transitions:
  - {from: work, to: done}
  - {from: program, to: done}
"#;

    struct Fixture {
        _home: super::TempHome,
        root: tempfile::TempDir,
        member: PathBuf,
        machine: PathBuf,
        state: &'static str,
    }

    impl Fixture {
        fn new(state: &'static str) -> Self {
            let root = tempfile::tempdir().unwrap();
            let member = root.path().join("member");
            fs::create_dir_all(member.join("tasks")).unwrap();
            fs::write(root.path().join("index.panta.md"), "# Panta: Retry\n").unwrap();
            fs::write(member.join("index.rhei.md"), "# Rhei: Member\n").unwrap();
            fs::write(
                member.join("tasks/01-work.md"),
                format!("### Task 1: Work\n**State:** {state}\n"),
            )
            .unwrap();
            let machine = root.path().join("states.yaml");
            fs::write(&machine, MACHINE).unwrap();
            Self { _home: super::TempHome::new(), root, member, machine, state }
        }

        fn runtimes(&self) -> (PathBuf, PathBuf) {
            let project = self.root.path().join("runtime");
            let member = self.member.join("runtime");
            if self.state == "program" {
                (member, project)
            } else {
                (project, member)
            }
        }

        fn charge(&self, runtime: &Path, number: LogNumber, ended: &str) -> PathBuf {
            let plan =
                plan_spawn_attempt(runtime, &self.member, "member.1", self.state, None, number);
            plan.record_spawn(SpawnEnding {
                task_id: "member.1",
                state_name: self.state,
                kind: if self.state == "program" { "program" } else { "agent" },
                worker: "mock",
                started: "2026-10-13T10:00:00Z",
                ended,
                duration: "1s",
                code: Some(1),
                ending: "exited",
            });
            plan.record
        }

        fn retry(&self) -> MietteResult<()> {
            retry_command_with_worker(self.root.path(), &[], Some(&self.machine), "member.1", false)
        }

        fn discover(&self, runtime: &Path) -> Vec<(PathBuf, SpawnRecord, Option<String>)> {
            let loaded = load_plan(self.root.path()).unwrap();
            let machine = super::machine_with_states(MACHINE);
            let task = find_task_by_id(&loaded.rhei.tasks, &parse_task_id("member.1")).unwrap();
            retry_active_records(
                &loaded,
                &machine,
                task,
                self.state,
                &RheiSettings::default(),
                &self.member,
                runtime,
            )
            .unwrap()
        }
    }

    #[test]
    fn retry_handler_uses_active_runtime_regardless_of_dormant_recency_or_capacity() {
        for state in ["work", "program"] {
            for dormant_capacity in [false, true] {
                let fixture = Fixture::new(state);
                let (active, dormant) = fixture.runtimes();
                let number = LogNumber::Entry(1);
                for _ in 0..2 {
                    fixture.charge(&active, number, "2026-10-13T10:00:01Z");
                    fixture.charge(&dormant, number, "2099-10-13T10:00:01Z");
                }
                let active_path = spawn_record_path(&active, "member.1", state, None);
                let dormant_path = spawn_record_path(&dormant, "member.1", state, None);
                if dormant_capacity {
                    grant_retry_record(
                        &dormant_path,
                        "member.1",
                        state,
                        0,
                        AttemptGrant {
                            at: "2099-10-13T10:01:00Z".into(),
                            os_user: "operator".into(),
                            attempts: 2,
                        },
                    )
                    .unwrap();
                }
                let dormant_before = fs::read(&dormant_path).unwrap();
                fixture.retry().unwrap();
                let granted = read_spawn_record(&active_path).unwrap();
                assert_eq!(granted.moves, 0);
                assert_eq!(granted.attempt, 2);
                assert_eq!(granted.charged, 2);
                assert_eq!(granted.grants.len(), 1);
                assert_eq!(granted.grants[0].attempts, 2);
                let next =
                    plan_spawn_attempt(&active, &fixture.member, "member.1", state, None, number);
                assert_eq!(next.attempt, 3);
                assert_eq!(next.effective_allowance(2), 4);
                assert!(next.budget_spent(AttemptBudget::Visit(2)).is_none());
                let active_before = fs::read(&active_path).unwrap();
                assert!(fixture.retry().unwrap_err().to_string().contains("2 attempts left"));
                assert_eq!(fs::read(&active_path).unwrap(), active_before);
                assert_eq!(fs::read(&dormant_path).unwrap(), dormant_before);
            }
        }
    }

    #[test]
    fn retry_missing_active_record_does_not_refill_a_dormant_runtime() {
        for state in ["work", "program"] {
            let fixture = Fixture::new(state);
            let (active, dormant) = fixture.runtimes();
            let path = fixture.charge(&dormant, LogNumber::Entry(1), "2099-10-13T10:00:01Z");
            fixture.charge(&dormant, LogNumber::Entry(1), "2099-10-13T10:00:01Z");
            let before = fs::read(&path).unwrap();
            assert!(fixture.retry().unwrap_err().to_string().contains("no spawn record"));
            assert_eq!(fs::read(&path).unwrap(), before);
            assert!(!spawn_record_path(&active, "member.1", state, None).exists());
        }
    }

    #[test]
    fn retry_discovery_matches_run_own_record_and_legacy_fallback_precedence() {
        let fixture = Fixture::new("work");
        let (runtime, _) = fixture.runtimes();
        fs::create_dir_all(fixture.member.join("runtime")).unwrap();
        fs::write(fixture.member.join("runtime/state-transitions.log"), "member.1 work@work\n")
            .unwrap();
        let fallback = fixture.charge(&runtime, LogNumber::Entry(1), "2099-10-13T10:00:01Z");
        let own = fixture.charge(&runtime, LogNumber::Entry(2), "2026-10-13T10:00:01Z");
        let selected = fixture.discover(&runtime);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, own);
        let next = plan_spawn_attempt(
            &runtime,
            &fixture.member,
            "member.1",
            "work",
            None,
            LogNumber::Entry(2),
        );
        assert_eq!(next.previous.unwrap().ended, selected[0].1.ended);
        // A stale primary prevents fallback, just as it does for the run.
        update_spawn_record(&own, |stored| {
            let mut stored = stored.unwrap();
            stored.moves = 0;
            Ok(Some(stored))
        })
        .unwrap();
        assert!(fixture.discover(&runtime).is_empty());
        assert!(plan_spawn_attempt(
            &runtime,
            &fixture.member,
            "member.1",
            "work",
            None,
            LogNumber::Entry(2)
        )
        .previous
        .is_none());
        fs::remove_file(&own).unwrap();
        let selected = fixture.discover(&runtime);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, fallback);
        assert_eq!(selected[0].1.moves, 1);
        assert_eq!(
            plan_spawn_attempt(
                &runtime,
                &fixture.member,
                "member.1",
                "work",
                None,
                LogNumber::Entry(2)
            )
            .previous
            .unwrap()
            .ended,
            selected[0].1.ended
        );
    }
}
