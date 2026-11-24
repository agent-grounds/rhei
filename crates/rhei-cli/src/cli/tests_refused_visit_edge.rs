// When a spent or finished visit's records say the run tried its edge: the
// record that ended last proves the work, chosen among this visit's records only.
//
// Its own part because the end-to-end fan-out test can order its arms a second
// apart but cannot make two of them end in the same second.

// §AR-source-file-size.3 §FS-rhei-errors.1.5.2

mod refused_visit_edge {
    use super::super::*;

    const TASK: &str = "plan.1";
    const STATE: &str = "draft";
    const MOVES: u64 = 2;

    fn record(moves: u64, ended: &str, code: i32) -> SpawnRecord {
        SpawnRecord {
            task: TASK.into(),
            state: STATE.into(),
            moves,
            attempt: 1,
            charged: 1,
            grants: Vec::new(),
            attempt_charged: true,
            kind: "agent".into(),
            worker: "mock".into(),
            log: PathBuf::from("runtime/logs/x.log"),
            started: "2026-11-24T10:00:00Z".into(),
            ended: ended.into(),
            duration: "1s".into(),
            code: Some(code),
            ending: "exited".into(),
            unavailable: 0,
            killed: 0,
            signal: None,
            reverted: None,
        }
    }

    fn met(records: &[SpawnRecord]) -> bool {
        last_to_end_met(records, TASK, STATE, MOVES)
    }

    // §FS-rhei-errors.1.5.2: the edge was tried only when the success ended last.
    #[test]
    fn only_a_success_that_ended_last_says_the_edge_was_tried() {
        let failed = record(MOVES, "2026-11-24T10:00:05Z", 3);
        let finished = record(MOVES, "2026-11-24T10:00:04Z", 0);
        assert!(!met(&[failed.clone(), finished.clone()]), "a failure ended last");
        let finished = record(MOVES, "2026-11-24T10:00:06Z", 0);
        assert!(met(&[failed, finished]), "the success ended last");
    }

    // §FS-rhei-errors.1.5.2: a later record of another visit does not hide this one's.
    #[test]
    fn records_of_earlier_visits_are_dropped_before_the_last_is_chosen() {
        let finished = record(MOVES, "2026-11-24T10:00:04Z", 0);
        let earlier_visit = record(MOVES - 1, "2026-11-24T10:00:09Z", 3);
        assert!(met(&[earlier_visit.clone(), finished]));
        assert!(!met(&[earlier_visit]), "no record of this visit proves anything");
    }

    // §FS-rhei-errors.1.5.2: within one second the order the run judged them in is unknown.
    #[test]
    fn a_tie_counts_only_when_every_tied_record_proves_its_work() {
        let at = "2026-11-24T10:00:05Z";
        assert!(!met(&[record(MOVES, at, 0), record(MOVES, at, 3)]));
        assert!(met(&[record(MOVES, at, 0), record(MOVES, at, 0)]));
    }

    // §FS-rhei-run.3.7.4: a reverted last attempt met nothing.
    #[test]
    fn a_reverted_last_record_proves_nothing() {
        let mut finished = record(MOVES, "2026-11-24T10:00:05Z", 0);
        assert!(met(std::slice::from_ref(&finished)), "one worker, one record");
        finished.reverted = Some(RevertedEdit {
            location: "plan.rhei.md:3".into(),
            message: "broke the plan".into(),
            text: PathBuf::from("runtime/reverted/plan.1.md"),
        });
        assert!(!met(&[finished]));
    }
}
