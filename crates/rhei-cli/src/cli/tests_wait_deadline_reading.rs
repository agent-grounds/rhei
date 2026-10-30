// One reading of the plan, one instant: a poll the ready scan held back as
// still in the future stays pending to every judgement drawn from that same
// reading — the deadline the run sleeps for, the halt-or-idle judgement, and
// the next attempt it reports — even when the clock's second turns between
// the scan and them (agent-grounds/rhei#542).
//
// Its own part because these hold the clock still and then move it, which no
// reading in `tests_wait_deadlines.rs` needs.

// §AR-source-file-size.3 §FS-rhei-run.5.1

mod wait_deadline_reading {
    use super::wait_deadlines::{record_reverted_attempt_of, POLL_MACHINE};
    use super::*;

    /// The second the pass reads its ready set in. Any instant serves; a fixed
    /// one keeps these tests off the wall clock entirely.
    const T: u64 = 1_800_000_000;

    /// Holds `current_unix_secs()` on this thread until dropped, so a test
    /// chooses the second each read lands in rather than racing for it.
    struct HeldClock;

    impl HeldClock {
        fn at(secs: u64) -> Self {
            HELD_UNIX_SECS.with(|held| held.set(Some(secs)));
            Self
        }

        fn tick_to(&self, secs: u64) {
            HELD_UNIX_SECS.with(|held| held.set(Some(secs)));
        }
    }

    impl Drop for HeldClock {
        fn drop(&mut self) {
            HELD_UNIX_SECS.with(|held| held.set(None));
        }
    }

    /// A single-file plan with one program poll whose next attempt is due at
    /// `T + 1`, its bound unspent, loaded the way a run loads it.
    struct PollPlan {
        dir: tempfile::TempDir,
        runtime: PathBuf,
        loaded: LoadedPlan,
        machines: ExecutionMachines,
    }

    impl PollPlan {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tmpdir");
            let plan_path = dir.path().join("plan.rhei.md");
            let plan = "# Rhei: Poll\n\n## Tasks\n\n### Task 1: One\n**State:** waiting\n";
            fs::write(&plan_path, plan).expect("write plan");
            let mut loaded = load_plan(&plan_path).expect("plan loads");
            let task = loaded.rhei.tasks[0].id.clone();
            loaded.rhei.metadata =
                Some(set_poll_next_attempt_metadata(None, &task, "waiting", T + 1, 1));
            let machine =
                rhei_validator::StateMachine::from_yaml_str(POLL_MACHINE).expect("machine");
            let machines = ExecutionMachines {
                set: rhei_validator::MachineSet::single(machine),
                default_callbacks: CallbackPaths {
                    plan_path,
                    state_machine_path: None,
                    working_dir: dir.path().to_path_buf(),
                },
                per_rhei_callbacks: BTreeMap::new(),
                member_local_runtimes: BTreeSet::new(),
                state_machine_override: None,
            };
            let runtime = dir.path().join("runtime");
            Self { dir, runtime, loaded, machines }
        }

        /// One reading of the plan, as the pass loop takes it before its ready
        /// scan.
        fn reading<'a>(
            &'a self,
            settings: &'a RheiSettings,
            opts: &'a RunOptions,
        ) -> SpawnSiteRoots<'a> {
            let (root, runtime) = (self.dir.path(), &self.runtime);
            SpawnSiteRoots::new(root, runtime, &self.loaded, &self.machines, Some(settings), opts)
        }

        /// The pass's ready scan over the reading `spawns` stands for, by the
        /// name the pass loop calls it.
        fn ready_ids(&self, spawns: &SpawnSiteRoots<'_>) -> Vec<String> {
            let ready = find_runnable_tasks_with_options(
                &self.loaded.rhei,
                &self.machines.set,
                spawns,
                &HashSet::new(),
            );
            ready.iter().map(|task| task.id.to_string()).collect()
        }

        /// The premise each test starts from: read in second `T`, the scan
        /// holds the poll back, because its attempt is due at `T + 1`.
        fn assert_scan_holds_the_poll(&self, spawns: &SpawnSiteRoots<'_>) {
            assert_eq!(
                self.ready_ids(spawns),
                Vec::<String>::new(),
                "premise: in second T the ready scan holds back a poll due at T + 1"
            );
        }
    }

    /// The ticket's run, in agent mode: the ready scan read second `T` and
    /// held the poll back, then the second turned before the empty-ready
    /// branch asked what to wait for. The deadline is the reading's, so the
    /// run sleeps for it rather than halting with attempt 2 owed, due and
    /// unspent. The next reading, in `T + 1`, takes the attempt up; and once
    /// the bound is spent the same reading waits for nothing, so the anchor
    /// keeps the spent-bound filter.
    // §FS-rhei-run.5.1 §FS-rhei-run.3.6 §FS-rhei-run.3.7.4
    #[test]
    fn a_poll_the_scan_held_is_still_waited_for_after_the_second_turns() {
        let clock = HeldClock::at(T);
        let plan = PollPlan::new();
        let (settings, opts) = (RheiSettings::default(), default_run_options());
        let spawns = plan.reading(&settings, &opts);
        plan.assert_scan_holds_the_poll(&spawns);
        let rhei = &plan.loaded.rhei;
        let query =
            || earliest_pending_agent_deadline(rhei, &plan.machines.set, &settings, &None, &spawns);
        assert_eq!(query(), Some(T + 1), "premise: read in second T, the poll is pending");

        clock.tick_to(T + 1);
        assert_eq!(
            query(),
            Some(T + 1),
            "the scan read second T and held the poll back, so the pass waits for T + 1 \
             however late it asks; a None here is the halt with the attempt owed"
        );

        let task = rhei.tasks[0].id.to_string();
        let next = plan.reading(&settings, &opts);
        assert_eq!(plan.ready_ids(&next), vec![task.clone()], "the next reading takes it up");

        let machine = plan.machines.set.for_task(&rhei.tasks[0].id);
        for _ in 1..=3 {
            record_reverted_attempt_of(machine, &task, plan.dir.path(), &plan.runtime);
        }
        assert_eq!(query(), None, "no attempt follows a spent bound, so none is waited for");
    }

    /// The callback-mode loop asks the same question of the same reading, and
    /// gets the same answer.
    // §FS-rhei-run.5.1
    #[test]
    fn a_poll_the_scan_held_is_still_waited_for_in_callback_mode_after_the_second_turns() {
        let clock = HeldClock::at(T);
        let plan = PollPlan::new();
        let (settings, opts) = (RheiSettings::default(), default_run_options());
        let spawns = plan.reading(&settings, &opts);
        plan.assert_scan_holds_the_poll(&spawns);
        let query = || {
            earliest_pending_poll_deadline(&plan.loaded.rhei, &plan.machines.set, &None, &spawns)
        };
        assert_eq!(query(), Some(T + 1), "premise: read in second T, the poll is pending");

        clock.tick_to(T + 1);
        assert_eq!(
            query(),
            Some(T + 1),
            "the scan read second T and held the poll back, so the callback loop waits for T + 1"
        );
    }

    /// Under `--until-idle` the same reading returns idle, waiting on the poll
    /// and naming `T + 1` as the next attempt, rather than halting while its
    /// own report says the poll resumes itself.
    // §FS-rhei-run.5.1 §FS-rhei-run.3 §FS-rhei-run-report.3.1
    #[test]
    fn a_poll_the_scan_held_is_still_the_idle_return_after_the_second_turns() {
        let clock = HeldClock::at(T);
        let plan = PollPlan::new();
        let settings = RheiSettings::default();
        let mut opts = default_run_options();
        opts.standalone.until_idle = true;
        let spawns = plan.reading(&settings, &opts);
        plan.assert_scan_holds_the_poll(&spawns);
        let verdict = || {
            idle_return(&plan.loaded.rhei, &plan.machines.set, &settings, &None, &spawns)
                .map(|idle| (idle.blocker, idle.waiting, idle.next_attempt_at))
        };
        let next_attempt =
            rhei_tui::format_rfc3339(std::time::UNIX_EPOCH + Duration::from_secs(T + 1));
        let waiting_on_the_poll = Some((rhei_tui::IdleBlocker::Poll, 1, Some(next_attempt)));
        assert_eq!(verdict(), waiting_on_the_poll, "premise: read in second T, the run is idle");

        clock.tick_to(T + 1);
        assert_eq!(
            verdict(),
            waiting_on_the_poll,
            "the scan read second T and held the poll back, so the run is idle on it, next \
             attempt T + 1; a None here is the halt"
        );
    }
}
