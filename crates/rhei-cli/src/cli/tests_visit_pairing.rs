// The pairing rule under an in-place target edit: which orphaned spawn record
// may answer for an invocation that has none of its own, and when none may.
//
// Its own part because the end-to-end tests reach this through two real runs
// and an edited plan, where a wrong pairing shows up only as a spawn count.

// §AR-source-file-size.3 §FS-rhei-agent-visit-pairing

mod visit_pairing {
    use super::super::*;

    const TASK: &str = "plan.1";
    const STATE: &str = "work";
    const MOVES: u64 = 3;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("runtime/spawns/task-{TASK}-{STATE}-{name}.json"))
    }

    fn names(name: &str) -> Vec<PathBuf> {
        vec![path(name)]
    }

    fn record(state: &str, moves: u64, ending: &str, code: Option<i32>) -> SpawnRecord {
        SpawnRecord {
            task: TASK.into(),
            state: state.into(),
            moves,
            attempt: 1,
            charged: 1,
            attempt_charged: true,
            kind: "agent".into(),
            worker: "mock".into(),
            log: PathBuf::from("runtime/logs/x.log"),
            started: "2026-10-04T10:00:00Z".into(),
            ended: "2026-10-04T10:00:01Z".into(),
            duration: "1s".into(),
            code,
            ending: ending.into(),
            reverted: None,
        }
    }

    fn finished(name: &str) -> (PathBuf, SpawnRecord) {
        (path(name), record(STATE, MOVES, "exited", Some(0)))
    }

    fn pair(
        own_names: &[Vec<PathBuf>],
        own_ran: &[bool],
        records: &[(PathBuf, SpawnRecord)],
    ) -> VisitPairing {
        pair_orphaned_record(own_names, own_ran, records, TASK, STATE, MOVES)
    }

    // §FS-rhei-agent-visit-pairing.3: one orphan and one recordless invocation pair.
    #[test]
    fn one_orphan_pairs_with_the_one_recordless_invocation() {
        let pairing = pair(&[names("m-y")], &[false], &[finished("m-x")]);
        assert_eq!(pairing.paired, Some((path("m-y"), path("m-x"))));
        assert!(pairing.ambiguous.is_empty());
        assert!(pairing.ran_this_visit);
    }

    // §FS-rhei-agent-visit-pairing.5: two orphans for one invocation is a guess, and is warned about.
    #[test]
    fn two_orphans_pair_nothing() {
        let pairing = pair(&[names("c")], &[false], &[finished("a"), finished("b")]);
        assert_eq!(pairing.paired, None);
        assert_eq!(pairing.ambiguous, [path("a"), path("b")]);
    }

    // §FS-rhei-agent-visit-pairing.5: one orphan for two recordless invocations is a guess too.
    #[test]
    fn two_recordless_invocations_pair_nothing() {
        let pairing = pair(&[names("c"), names("d")], &[false, false], &[finished("a")]);
        assert_eq!(pairing.paired, None);
        assert_eq!(pairing.ambiguous, [path("a")]);
        assert_eq!(pairing.unrecorded, [path("c"), path("d")], "the warning names both targets");
    }

    // §FS-rhei-agent-visit-pairing.5: a removed member's orphan and an added member pass silently.
    #[test]
    fn nothing_to_choose_between_is_not_ambiguous() {
        let removed = pair(&[names("a")], &[true], &[finished("a"), finished("b")]);
        assert_eq!(removed, VisitPairing { ran_this_visit: true, ..VisitPairing::default() });
        let added = pair(&[names("a"), names("b")], &[true, false], &[finished("a")]);
        assert_eq!(added, VisitPairing { ran_this_visit: true, ..VisitPairing::default() });
    }

    // §FS-rhei-agent-visit-pairing.3: only successful work of the current visit can stand in.
    #[test]
    fn a_stale_or_failed_orphan_pairs_nothing() {
        let stale = (path("m-x"), record(STATE, MOVES - 1, "exited", Some(0)));
        let pairing = pair(&[names("m-y")], &[false], &[stale]);
        assert_eq!(pairing, VisitPairing::default(), "an earlier visit is not this one");

        let failed = (path("m-x"), record(STATE, MOVES, "exited", Some(1)));
        let pairing = pair(&[names("m-y")], &[false], &[failed]);
        assert_eq!(pairing.paired, None);
        assert!(pairing.ambiguous.is_empty());
        assert!(pairing.ran_this_visit, "a failed record still says the state ran this visit");
    }

    // §FS-rhei-agent-visit-pairing.3: `review` never collects `review-fix`'s records by name prefix.
    #[test]
    fn a_record_matching_only_by_file_name_prefix_does_not_count() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        let spawns = spawn_records_dir(&runtime);
        fs::create_dir_all(&spawns).expect("spawns dir");
        let mut other = record("review-fix", MOVES, "exited", Some(0));
        other.task = TASK.into();
        fs::write(
            spawns.join(format!("task-{TASK}-review-fix-mock-mock-m-x.json")),
            serde_json::to_string(&other).expect("json"),
        )
        .expect("write review-fix's record");

        let records = spawn_records_for_state(&runtime, TASK, "review");
        assert!(records.is_empty(), "review-fix's record is not review's: {records:?}");
        let pairing = pair_orphaned_record(
            &[vec![spawn_record_path(&runtime, TASK, "review", Some("mock-mock-m-y"))]],
            &[false],
            &records,
            TASK,
            "review",
            MOVES,
        );
        assert_eq!(pairing, VisitPairing::default());
    }

    // §FS-rhei-agent-visit-pairing.2: an own record of this visit that failed or still runs decides alone.
    #[test]
    fn an_own_failed_or_running_record_is_a_record_of_its_own() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        fs::create_dir_all(spawn_records_dir(&runtime)).expect("spawns dir");
        let write = |slug: &str, record: &SpawnRecord| {
            let path = spawn_record_path(&runtime, TASK, STATE, Some(slug));
            fs::write(&path, serde_json::to_string(record).expect("json")).expect("write record");
            path
        };
        write("mock-mock-x", &record(STATE, MOVES, "exited", Some(0)));
        for (ending, code) in [("exited", Some(1)), ("running", None)] {
            let own = vec![write("mock-mock-d", &record(STATE, MOVES, ending, code))];
            let ran = own_record_this_visit(&own, TASK, STATE, MOVES);
            assert!(ran, "d's own {ending} record is of this visit");
            let records = spawn_records_for_state(&runtime, TASK, STATE);
            let pairing = pair_orphaned_record(&[own], &[ran], &records, TASK, STATE, MOVES);
            assert_eq!(
                pairing,
                VisitPairing { ran_this_visit: true, ..VisitPairing::default() },
                "x's orphan must not answer for d, whose own record {ending}"
            );
        }
        let own = vec![write("mock-mock-d", &record(STATE, MOVES - 1, "exited", Some(1)))];
        assert!(!own_record_this_visit(&own, TASK, STATE, MOVES), "an earlier visit is not this one");
    }

    // §FS-rhei-agent-visit-pairing.1: every name on an own-name list is disowned, not only the one that answered.
    #[test]
    fn a_name_on_an_own_name_list_is_never_an_orphan() {
        let a_names = vec![path("a"), path("a-fallback")];
        let pairing =
            pair(&[a_names, names("b")], &[true, false], &[finished("a"), finished("a-fallback")]);
        assert_eq!(pairing.paired, None, "a's fallback record must not answer for b");
        assert!(pairing.ambiguous.is_empty());
    }
}
