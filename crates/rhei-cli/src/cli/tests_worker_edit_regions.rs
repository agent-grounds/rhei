// What a worker's region is, how a restore finds it again, and which of the
// three cases a reload's failure falls in — as text, below the run, so a wrong
// boundary shows up here rather than as a wrong revert three layers away. And
// the orders a stop and a worker's exit can meet in, which no run can force.

// §AR-source-file-size.3 §FS-rhei-run.3.7

mod worker_edit_regions {
    use super::super::*;

    const PLAN: &str = "\
# Plan

### Task 1: Raise coverage
**State:** cover

Raise it.

#### Task 1.1: A child kept inside
**State:** todo

### Task 2: Other
**State:** other
";

    const TASK_FILE: &str = "\
### Task 1: Raise coverage
**State:** cover

Raise it.
";

    fn region(file: &Path, text: &str, local_id: &str, live: usize) -> InFlightRegion {
        let snapshot = snapshot_region(text, local_id).expect("region");
        InFlightRegion { file: file.to_path_buf(), snapshot, live }
    }

    fn empty_registry() -> WorkerRegions {
        WorkerRegions {
            armed: true,
            root: None,
            in_flight: BTreeMap::new(),
            held: BTreeMap::new(),
            stopped: Default::default(),
            refusals: BTreeMap::new(),
        }
    }

    /// A region in a single-file plan ends at the next heading of its own
    /// depth, and a deeper child stays inside it. §FS-rhei-run.3.7.1
    #[test]
    fn a_region_ends_at_the_next_sibling_and_keeps_its_children() {
        let snapshot = snapshot_region(PLAN, "1").expect("Task 1 is declared once");
        assert_eq!(snapshot.heading, "### Task 1: Raise coverage");
        assert!(snapshot.text.contains("#### Task 1.1: A child kept inside"));
        assert!(!snapshot.text.contains("Task 2"));
        assert_eq!(snapshot.following.as_deref(), Some("### Task 2: Other"));

        let child = snapshot_region(PLAN, "1.1").expect("the child is a region too");
        assert_eq!(child.following.as_deref(), Some("### Task 2: Other"));
    }

    /// The last task's region, and a directory workspace's one-task file, run
    /// to the end of the file. §FS-rhei-run.3.7.1
    #[test]
    fn a_region_with_no_following_heading_ends_the_file() {
        let last = snapshot_region(PLAN, "2").expect("Task 2");
        assert_eq!(last.following, None);
        assert!(PLAN.ends_with(&last.text));

        let task_file = snapshot_region(TASK_FILE, "1").expect("the task file's task");
        assert_eq!(task_file.text, TASK_FILE);
        assert_eq!(task_file.following, None);
    }

    /// A heading declared twice has no region to snapshot. §FS-rhei-run.3.7.1
    #[test]
    fn a_duplicated_heading_has_no_region() {
        let twice = format!("{TASK_FILE}\n### Task 1: Raise coverage\n");
        assert_eq!(snapshot_region(&twice, "1"), None);
        assert_eq!(snapshot_region(PLAN, "9"), None);
    }

    /// The restore replaces the region and nothing else: a sibling's
    /// transition in the same file survives it. §FS-rhei-run.3.7.3
    #[test]
    fn a_restore_puts_back_one_region_and_keeps_every_other_byte() {
        let snapshot = snapshot_region(PLAN, "1").expect("Task 1");
        let edited = PLAN
            .replace("Raise it.\n", "Raise it.\n\n#### Visit 1 (cover)\n\nNotes.\n")
            .replace("**State:** other", "**State:** completed");
        let (restored, replaced) =
            restore_region(&edited, &snapshot).expect("both boundaries once");
        assert_eq!(restored, PLAN.replace("**State:** other", "**State:** completed"));
        assert!(replaced.contains("#### Visit 1 (cover)"));
        assert!(!replaced.contains("Task 2"));

        let file_snapshot = snapshot_region(TASK_FILE, "1").expect("task file");
        let appended = format!("{TASK_FILE}\n#### Visit 1 (cover)\n");
        let (restored, _) = restore_region(&appended, &file_snapshot).expect("end of file");
        assert_eq!(restored, TASK_FILE);
    }

    /// A restore that cannot find either boundary exactly once refuses rather
    /// than guesses, and says which. §FS-rhei-run.3.7.3
    #[test]
    fn a_restore_refuses_a_missing_or_duplicated_boundary() {
        let snapshot = snapshot_region(PLAN, "1").expect("Task 1");
        let renamed = PLAN.replace("### Task 1: Raise coverage", "### Task 1: Renamed");
        assert_eq!(region_span(&renamed, &snapshot), Err(RestoreRefusal::Heading { found: 0 }));
        let doubled = format!("{PLAN}### Task 1: Raise coverage\n");
        assert_eq!(region_span(&doubled, &snapshot), Err(RestoreRefusal::Heading { found: 2 }));

        let gone = PLAN.replace("### Task 2: Other", "### Task 2: Moved");
        assert_eq!(region_span(&gone, &snapshot), Err(RestoreRefusal::Following { found: 0 }));
        let twice = format!("{PLAN}### Task 2: Other\n");
        assert_eq!(region_span(&twice, &snapshot), Err(RestoreRefusal::Following { found: 2 }));
        assert!(RestoreRefusal::Following { found: 2 }.describe(&snapshot).contains("Task 2"));
    }

    /// A break is the exited worker's own, a running worker's, or outside every
    /// region — and an error with no location is outside. §FS-rhei-run.3.7.2
    #[test]
    fn a_break_is_classified_by_the_region_that_holds_its_line() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let file = dir.path().join("plan.rhei.md");
        fs::write(&file, PLAN).expect("plan");
        let at = |line| PlanBreak { file: file.clone(), line, message: "Malformed".into() };
        let own = region(&file, PLAN, "1", 0);
        let mut registry = empty_registry();
        registry.in_flight.insert("plan.2".into(), region(&file, PLAN, "2", 1));

        assert_eq!(registry.classify(Some(&own), Some(&at(5))), BreakClass::Own);
        assert_eq!(registry.classify(None, Some(&at(12))), BreakClass::Running("plan.2".into()));
        assert_eq!(registry.classify(None, Some(&at(1))), BreakClass::Outside);
        assert_eq!(registry.classify(Some(&own), None), BreakClass::Outside);

        registry.in_flight.get_mut("plan.2").expect("entry").live = 0;
        assert_eq!(registry.classify(None, Some(&at(12))), BreakClass::Outside);
    }

    /// The task a line belongs to is printed as the run prints it, under the
    /// owning rhei's id, in a workspace and in a single-file plan. §FS-rhei-run.3.7.6
    #[test]
    fn the_task_owning_a_line_is_named_as_the_run_names_it() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let ws = dir.path().join("ws");
        fs::create_dir_all(ws.join("tasks")).expect("tasks dir");
        fs::write(ws.join("index.rhei.md"), "# Plan\n").expect("index");
        let task_file = ws.join("tasks/03-later.md");
        fs::write(&task_file, TASK_FILE).expect("task file");
        assert_eq!(task_owning_line(&task_file, 4).as_deref(), Some("ws.1"));

        let plan = dir.path().join("plan.rhei.md");
        fs::write(&plan, PLAN).expect("plan");
        assert_eq!(task_owning_line(&plan, 9).as_deref(), Some("plan.1.1"));
        assert_eq!(task_owning_line(&plan, 12).as_deref(), Some("plan.2"));
        assert_eq!(task_owning_line(&plan, 1), None);
    }

    /// The outcomes a release reported, in order.
    #[derive(Default)]
    struct Released(std::sync::Mutex<Vec<rhei_tui::TaskOutcome>>);

    impl rhei_tui::EventSink for Released {
        fn emit(&self, event: rhei_tui::RunEvent) {
            if let rhei_tui::RunEvent::SlotReleased { outcome, .. } = event {
                self.0.lock().expect("released lock").push(outcome);
            }
        }
    }

    /// A worker whose exit 0 broke nothing of its own while another task's
    /// text breaks the plan, as in the parallel run CI lost: Task 1 in flight,
    /// Task 3's file broken, and a registry of the test's own, so no run in
    /// this process can reach it.
    struct StopRace {
        _dir: tempfile::TempDir,
        ws: PathBuf,
        regions: WorkerRegions,
        attempt: SpawnPlan,
        sink: Arc<Released>,
    }

    impl StopRace {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tmpdir");
            let ws = dir.path().join("ws");
            fs::create_dir_all(ws.join("tasks")).expect("tasks dir");
            fs::create_dir_all(ws.join("runtime/logs")).expect("logs dir");
            fs::write(ws.join("index.rhei.md"), "# Rhei: Stop race\n").expect("index");
            fs::write(ws.join("tasks/01-t.md"), TASK_FILE).expect("Task 1");
            let broken = "### Task 3: Later\n**State:** todo\n\nBody.\n\n#### Visit 1 (cover)\n";
            fs::write(ws.join("tasks/03-t.md"), broken).expect("Task 3");
            let attempt = spawn_plan_for_test(&ws.join("runtime/logs/task-ws.1-cover.log"));
            let stored = SpawnRecord {
                task: "ws.1".into(),
                state: "cover".into(),
                moves: 0,
                attempt: 1,
                charged: 1,
                attempt_charged: true,
                kind: "program".into(),
                worker: "note".into(),
                log: attempt.log.clone(),
                started: String::new(),
                ended: String::new(),
                duration: String::new(),
                code: Some(0),
                ending: "exited".into(),
                reverted: None,
            };
            let body = serde_json::to_string_pretty(&stored).expect("record");
            fs::write(&attempt.record, body).expect("spawn record");
            let mut regions = empty_registry();
            regions.root = Some(ws.clone());
            let file = ws.join("tasks/01-t.md");
            regions.in_flight.insert("ws.1".into(), region(&file, TASK_FILE, "1", 1));
            Self { _dir: dir, ws, regions, attempt, sink: Arc::default() }
        }

        fn lease(&self) -> WorkerRegionLease {
            WorkerRegionLease {
                task_id: Some("ws.1".into()),
                state: "cover".into(),
                input: self.ws.clone(),
                budget: AttemptBudget::Visit(2),
            }
        }

        /// The release the worker thread builds from its reaped exit 0.
        fn release(&self) -> PendingSlotRelease {
            let release = SlotRelease {
                slot: 0,
                task: "ws.1".into(),
                from: "cover".into(),
                to: "cover".into(),
                log_path: self.attempt.log.clone(),
                outcome: rhei_tui::TaskOutcome::Completed,
                finished_at: std::time::Instant::now(),
                wall_clock: std::time::SystemTime::now(),
                exit_code: Some(0),
                duration_ms: 1,
            };
            PendingSlotRelease::hold(self.sink.clone(), release)
        }

        /// The checkpoint's stop on Task 3's break.
        fn stop(&mut self) {
            let at = locate_plan_break(&self.ws).expect("Task 3's text breaks the plan");
            assert!(at.file.ends_with("tasks/03-t.md"), "{}", at.file.display());
            let _ = self.regions.stop_on(&at);
        }

        /// The release dropped as the abandoned channel drops it.
        fn drop_release(&mut self, mut release: PendingSlotRelease) {
            release.settle_held(|record| self.regions.held_at_stop(record));
            drop(release);
        }

        fn assert_interrupted_and_uncharged(&self) {
            let outcomes = self.sink.0.lock().expect("released lock").clone();
            assert_eq!(outcomes, vec![rhei_tui::TaskOutcome::Interrupted]);
            let stored = read_spawn_record(&self.attempt.record).expect("spawn record");
            assert_eq!((stored.charged, stored.attempt_charged), (0, false));
            assert_eq!(stored.ending, "interrupted");
            assert!(self.regions.held.is_empty(), "nothing is left held");
        }
    }

    /// The order CI lost: the worker is reaped, the checkpoint stops the run
    /// before the worker reaches the registry, and only then does it. It was in
    /// flight at the stop, so it is interrupted and uncharged, never completed.
    // §FS-rhei-run.3.7.6
    #[test]
    fn a_worker_reaped_before_the_stop_but_marked_after_it_is_interrupted() {
        let mut race = StopRace::new();
        let lease = race.lease();
        let mut release = race.release();

        race.stop();
        assert!(lease.exited_in(&mut race.regions, &race.attempt, &mut release).is_none());
        assert!(race.regions.held.is_empty(), "an exit after the stop is never held");
        race.drop_release(release);

        race.assert_interrupted_and_uncharged();
    }

    /// The opposite order: the worker reaches the registry first, its exit is
    /// held on the break, and the stop then leaves it unrouted. §FS-rhei-run.3.7.6
    #[test]
    fn a_worker_held_before_the_stop_is_interrupted() {
        let mut race = StopRace::new();
        let lease = race.lease();
        let mut release = race.release();

        assert!(lease.exited_in(&mut race.regions, &race.attempt, &mut release).is_none());
        assert!(race.regions.held.contains_key(&race.attempt.record), "held on the break");
        race.stop();
        race.drop_release(release);

        race.assert_interrupted_and_uncharged();
    }

    /// The reverted text sits beside the attempt's log and takes its name.
    /// §FS-rhei-run.3.7.7
    #[test]
    fn the_reverted_text_takes_the_attempt_logs_name() {
        let log = Path::new("runtime/logs/task-ws.1-cover-attempt2.log");
        assert_eq!(
            reverted_text_path(log),
            Path::new("runtime/logs/task-ws.1-cover-attempt2.reverted.md")
        );
    }
}
