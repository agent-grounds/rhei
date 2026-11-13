// Arrivals the ledger never recorded — a `**State:**` line edited by hand — and
// what the entry number and the refusal of a taken log name make of them
// (agent-grounds/rhei#667, a gap in the entry numbers of agent-grounds/rhei#309).
//
// Apart from the entry-number tests because each case here is one ledger shape
// the first-line rule miscounted, and the refusal it led to.

// §AR-source-file-size.3 §FS-rhei-agents.8.1 §FS-rhei-agents.8.4

mod unrecorded_arrivals {
    use super::super::*;

    /// One ledger at `root/runtime/state-transitions.log`.
    fn ledger_at(root: &std::path::Path, lines: &str) {
        let runtime = root.join("runtime");
        fs::create_dir_all(&runtime).expect("runtime dir");
        fs::write(runtime.join("state-transitions.log"), lines).expect("ledger");
    }

    /// The entry number of `plan.1` in `state`, with one root for both ledgers.
    fn entry(root: &std::path::Path, state: &str) -> Result<u64, String> {
        ticket_entry_number(root, &root.join("runtime"), "plan.1", state)
    }

    /// The reported ledger: the ticket was edited from `park` into `gate`, left
    /// it, and came back over a real edge. Its departure from `gate` with no
    /// arrival before it reveals the first stay, so this one is entry 2.
    // §FS-rhei-agents.8.1
    #[test]
    fn a_departure_with_no_arrival_before_it_counts_as_an_arrival() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.1 prepare@park\nplan.1 gate@check\nplan.1 check@gate\n");
        assert_eq!(entry(dir.path(), "gate"), Ok(2), "edited in, left, came back");
        assert_eq!(entry(dir.path(), "check"), Ok(1), "arrived once, every move recorded");

        ledger_at(
            dir.path(),
            "plan.1 a@gate\nplan.1 gate@b\nplan.1 b@c\nplan.1 gate@d\nplan.1 d@gate\n",
        );
        assert_eq!(entry(dir.path(), "gate"), Ok(3), "two recorded arrivals and one revealed");
    }

    /// A ticket edited by hand into a state it was in before is on a stay its
    /// last ledger line does not land in: that stay is an arrival too.
    // §FS-rhei-agents.8.1
    #[test]
    #[ignore = "red until #667: the current stay is not counted when the last ledger line does not \
                land in its state; implement removes this"]
    fn the_current_stay_counts_when_no_line_lands_in_it() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.1 a@b\nplan.1 b@c\n");
        assert_eq!(entry(dir.path(), "b"), Ok(2), "arrived once, then edited back into it");
        assert_eq!(entry(dir.path(), "a"), Ok(2), "placed there, left, edited back into it");
    }

    /// The first-line rule is the case the general one starts from, and a
    /// ledger that records every move numbers exactly as before.
    // §FS-rhei-agents.8.1
    #[test]
    fn a_ledger_that_records_every_arrival_numbers_as_before() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.1 prepare@park\nplan.1 park@gate\nplan.1 gate@check\nplan.1 check@gate\n");
        assert_eq!(entry(dir.path(), "gate"), Ok(2));
        ledger_at(dir.path(), "plan.1 gate@check\nplan.1 check@gate\n");
        assert_eq!(entry(dir.path(), "gate"), Ok(2), "placed there, then one arrival");
        ledger_at(dir.path(), "plan.2 x@gate\nplan.1 a@b\nplan.2 gate@y\nplan.1 b@gate\n");
        assert_eq!(entry(dir.path(), "gate"), Ok(1), "another ticket's lines are not a gap");
    }

    /// The two ledgers are not one history: neither the seam between them nor
    /// either one's last line alone may reveal an arrival.
    // §FS-rhei-agents.8.1 §FS-rhei-panta.6.2
    #[test]
    fn the_seam_between_the_two_ledgers_reveals_no_arrival() {
        let owning = tempfile::tempdir().expect("owning root");
        let running = tempfile::tempdir().expect("run root");
        let number = |state: &str| {
            ticket_entry_number(owning.path(), &running.path().join("runtime"), "plan.1", state)
        };
        ledger_at(owning.path(), "plan.1 work@verifying\nplan.1 verifying@review\n");
        ledger_at(running.path(), "plan.1 work@verifying\nplan.1 verifying@work\n");
        assert_eq!(number("work"), Ok(2), "placed once, one arrival, no gap at the seam");

        ledger_at(owning.path(), "plan.1 a@work\n");
        ledger_at(running.path(), "plan.1 b@c\n");
        assert_eq!(number("work"), Ok(1), "the owning ledger's last line lands in the stay");
    }

    /// A planned name whose log a spawn record at that name names, at an
    /// earlier `moves`, belongs to an earlier stay the ledger records no arrival
    /// for: the refusal says so and points at the ledger, not at a reset.
    // §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
    #[test]
    fn a_log_an_earlier_stays_record_names_is_refused_as_that_stays() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        ledger_at(dir.path(), "plan.1 prepare@park\n");
        let earlier =
            plan_spawn_attempt(&runtime, dir.path(), "plan.1", "gate", None, LogNumber::Entry(1));
        fs::create_dir_all(runtime.join("logs")).expect("logs");
        fs::write(&earlier.log, "GATE-1\n").expect("the earlier stay's log");
        earlier.record_spawn(SpawnEnding {
            task_id: "plan.1",
            state_name: "gate",
            kind: "agent",
            worker: "mock",
            started: "2026-11-13T10:00:00Z",
            ended: "2026-11-13T10:00:01Z",
            duration: "1s",
            code: Some(0),
            ending: "exited",
        });
        ledger_at(dir.path(), "plan.1 prepare@park\nplan.1 gate@check\nplan.1 check@gate\n");
        let plan = SpawnPlan {
            reset: Some(ResetOffer::new(dir.path())),
            ..plan_spawn_attempt(&runtime, dir.path(), "plan.1", "gate", None, LogNumber::Entry(1))
        };
        assert_eq!(plan.log, earlier.log, "the same name, planned again");
        let refusal = plan.unaccounted_log_refusal("plan.1").expect("refused");
        let (first, remedy) = refusal.split_once('\n').expect("a remedy line");
        assert_eq!(
            first,
            format!(
                "refusing to spawn: {} belongs to an earlier stay in 'gate' (spawn record at \
                 moves 1), but the ledger records no arrival in 'gate' before that stay, so this \
                 stay took the same entry number",
                plan.log.display()
            )
        );
        assert!(remedy.starts_with("  help: "), "{remedy}");
        assert!(remedy.contains("rhei transition"), "the ledger is the cause:\n{remedy}");
        assert!(remedy.contains(&plan.record.display().to_string()), "{remedy}");
        assert!(!remedy.contains("rhei reset"), "a reset is far too much here:\n{remedy}");
        assert_eq!(fs::read_to_string(&plan.log).expect("kept"), "GATE-1\n");
    }

    /// A record at the planned name that names some other log does not account
    /// for this one: the no-record sentence and its reset stay as they are.
    // §FS-rhei-agents.8.1 §FS-rhei-reset.5.1
    #[test]
    fn a_record_naming_another_log_leaves_the_no_record_refusal() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        ledger_at(dir.path(), "plan.1 prepare@park\n");
        let earlier =
            plan_spawn_attempt(&runtime, dir.path(), "plan.1", "gate", None, LogNumber::Entry(1));
        let other = SpawnPlan {
            log: runtime.join("logs").join("somewhere-else.log"),
            ..earlier
        };
        other.record_spawn(SpawnEnding {
            task_id: "plan.1",
            state_name: "gate",
            kind: "agent",
            worker: "mock",
            started: "2026-11-13T10:00:00Z",
            ended: "2026-11-13T10:00:01Z",
            duration: "1s",
            code: Some(0),
            ending: "exited",
        });
        ledger_at(dir.path(), "plan.1 prepare@park\nplan.1 gate@check\nplan.1 check@gate\n");
        let plan = SpawnPlan {
            reset: Some(ResetOffer::new(dir.path())),
            ..plan_spawn_attempt(&runtime, dir.path(), "plan.1", "gate", None, LogNumber::Entry(1))
        };
        fs::create_dir_all(runtime.join("logs")).expect("logs");
        fs::write(&plan.log, "planted").expect("plant");
        let refusal = plan.unaccounted_log_refusal("plan.1").expect("refused");
        let (first, remedy) = refusal.split_once('\n').expect("a remedy line");
        assert!(first.ends_with("task-plan.1-gate.log exists and no spawn record accounts for it"));
        assert!(remedy.contains("rhei reset"), "{remedy}");
    }
}
