// Which number names an invocation's log: the entry counter over the ledgers,
// and the policy that picks it, `{visit_count}`, or none.
//
// Its own part because the end-to-end tests see these only as a log name three
// layers away, and each ledger shape here is a different way to miscount.

// §AR-source-file-size.3 §FS-rhei-agents.8.1 §FS-rhei-agents.8.4

mod entry_log_numbers {
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

    /// A ticket placed in a state is on its first entry whether or not the
    /// ledger has said anything about it yet.
    // §FS-rhei-agents.8.1
    #[test]
    fn initial_placement_counts_as_the_first_entry() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "");
        assert_eq!(entry(dir.path(), "measure"), Ok(1), "no ledger line yet");

        ledger_at(dir.path(), "plan.1 measure@cover\nplan.1 cover@measure\n");
        assert_eq!(entry(dir.path(), "measure"), Ok(2), "placed there, then one arrival");
        assert_eq!(entry(dir.path(), "cover"), Ok(1), "arrived once, never placed");
    }

    /// Other tickets' lines are someone else's history.
    // §FS-rhei-agents.8.1
    #[test]
    fn only_this_tickets_arrivals_count() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.2 a@b\nplan.1 a@b\nplan.2 b@a\nplan.2 a@b\n");
        assert_eq!(entry(dir.path(), "b"), Ok(1));
    }

    /// A forced transition is two rows and one arrival.
    // §FS-rhei-agents.8.1 §FS-rhei-complete.3.1
    #[test]
    fn a_forced_metadata_and_movement_pair_counts_once() {
        let audit = rhei_core::transition_history::ForceAudit {
            confirmation: "typed-hop-v1".into(),
            from: "verifying".into(),
            to: "work".into(),
            os_user: "operator".into(),
            reason: "fix route".into(),
            recovery_id: "018f0000-0000-7000-8000-000000000001".into(),
            result_sha256: None,
            schema_version: 1,
            task_id: "plan.1".into(),
            timestamp: "2026-09-18T12:00:00Z".into(),
        };
        let (metadata, movement) = audit.pair().expect("pair");
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), &format!("plan.1 work@verifying\n{metadata}{movement}"));
        assert_eq!(entry(dir.path(), "work"), Ok(2));
    }

    /// A metadata row this reader does not know is not an arrival.
    // §FS-rhei-agents.8.1
    #[test]
    fn an_unknown_metadata_row_counts_nothing() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.1 work@verifying\nplan.1 !future-v9 work\n");
        assert_eq!(entry(dir.path(), "work"), Ok(1));
    }

    /// No ledger at all is a fresh ticket: entry 1.
    // §FS-rhei-agents.8.1
    #[test]
    fn a_missing_ledger_is_entry_one() {
        let dir = tempfile::tempdir().expect("tmpdir");
        assert_eq!(entry(dir.path(), "work"), Ok(1));
    }

    /// An entry number guessed from a ledger that cannot be read is how a log
    /// gets reused, so it is a diagnostic instead.
    // §FS-rhei-agents.8.1
    #[test]
    fn an_unreadable_ledger_is_a_diagnostic() {
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.1 !force-v1 not-base64\nplan.1 verifying@work\n");
        let err = entry(dir.path(), "work").expect_err("a corrupt pair is unreadable");
        assert!(err.starts_with("refusing to spawn: the transition ledger "), "{err}");

        let unreadable = tempfile::tempdir().expect("tmpdir");
        fs::create_dir_all(unreadable.path().join("runtime/state-transitions.log"))
            .expect("a directory where the ledger should be");
        assert!(entry(unreadable.path(), "work").is_err());
    }

    /// A Panta project can route a ticket's moves to its owning root while the
    /// run's runtime is the project's: both are counted, and summed.
    // §FS-rhei-agents.8.1 §FS-rhei-panta.6.2
    #[test]
    fn the_two_ledgers_are_summed() {
        let owning = tempfile::tempdir().expect("owning root");
        let running = tempfile::tempdir().expect("run root");
        ledger_at(owning.path(), "plan.1 work@verifying\nplan.1 verifying@work\n");
        ledger_at(running.path(), "plan.1 work@verifying\nplan.1 verifying@work\n");
        let number =
            ticket_entry_number(owning.path(), &running.path().join("runtime"), "plan.1", "work");
        assert_eq!(number, Ok(3), "placed once, one arrival in each ledger");
    }

    fn policy_machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(
            r#"
name: entry-policy
version: 1
states:
  plain:
    initial: true
    description: Uncounted
    agent: mock
  counted:
    description: Counted by visits
    agent: mock
    visits: 3
  looping:
    description: Counted by its self-loop
    agent: mock
  waiting:
    description: Polls
    agent: mock
    poll: { interval: 1m, max_attempts: 3 }
  supervising:
    description: Supervises
    agent: mock
    execute_on: child-terminal
  done:
    description: Done
    final: true
transitions:
  - { from: plain, to: counted }
  - { from: counted, to: looping }
  - { from: looping, to: looping }
  - { from: looping, to: waiting }
  - { from: waiting, to: waiting }
  - { from: waiting, to: supervising }
  - { from: supervising, to: supervising }
  - { from: supervising, to: done }
"#,
        )
        .expect("machine should parse")
    }

    /// Counted states keep `{visit_count}`, a poll state takes no number, and
    /// every other state is numbered by its entries.
    // §FS-rhei-agents.8.1 §FS-rhei-transitions.4.3
    #[test]
    fn the_policy_picks_visit_count_none_or_the_entry_number() {
        let machine = policy_machine();
        let dir = tempfile::tempdir().expect("tmpdir");
        ledger_at(dir.path(), "plan.1 plain@counted\nplan.1 counted@plain\n");
        let resolve = |state: &str| {
            resolve_log_number(
                &machine,
                state,
                4,
                dir.path(),
                &dir.path().join("runtime"),
                "plan.1",
            )
        };
        assert_eq!(resolve("counted"), Ok(LogNumber::Visit(4)), "visits:");
        assert_eq!(resolve("looping"), Ok(LogNumber::Visit(4)), "a self-loop");
        assert_eq!(resolve("supervising"), Ok(LogNumber::Visit(4)), "execute_on");
        assert_eq!(resolve("waiting"), Ok(LogNumber::Unnumbered), "poll:");
        assert_eq!(resolve("plain"), Ok(LogNumber::Entry(2)), "placed, then one arrival");
    }

    /// The identity comes before the number, and a number of 1 is left out.
    // §FS-rhei-agents.8.1
    #[test]
    fn the_identity_comes_before_the_number() {
        assert_eq!(log_suffix(Some("opus"), Some(2)).as_deref(), Some("opus-2"));
        assert_eq!(log_suffix(Some("opus"), Some(1)).as_deref(), Some("opus"));
        assert_eq!(log_suffix(None, Some(3)).as_deref(), Some("3"));
        assert_eq!(log_suffix(None, None), None);
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        let plan = plan_spawn_attempt(
            &runtime,
            dir.path(),
            "plan.1",
            "cover",
            Some("opus"),
            LogNumber::Entry(2),
        );
        assert!(plan.log.ends_with("task-plan.1-cover-opus-2.log"));
        assert!(plan.record.ends_with("task-plan.1-cover-opus-2.json"));
    }

    /// A runtime upgraded during an uncounted re-entry holds the old unsuffixed
    /// record at the current `moves`; the retry continues it as `-2-attempt2`.
    // §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
    #[test]
    fn an_upgraded_reentry_continues_the_unsuffixed_record() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        ledger_at(dir.path(), "plan.1 work@verifying\nplan.1 verifying@work\n");
        let legacy =
            plan_spawn_attempt(&runtime, dir.path(), "plan.1", "work", None, LogNumber::Entry(1));
        legacy.record_spawn(SpawnEnding {
            task_id: "plan.1",
            state_name: "work",
            kind: "agent",
            worker: "mock",
            started: "2026-08-29T10:00:00Z",
            ended: "2026-08-29T10:00:01Z",
            duration: "1s",
            code: Some(1),
            ending: "exited",
        });
        let retry =
            plan_spawn_attempt(&runtime, dir.path(), "plan.1", "work", None, LogNumber::Entry(2));
        assert_eq!(retry.attempt, 2);
        assert!(retry.log.ends_with("task-plan.1-work-2-attempt2.log"));
        assert!(retry.record.ends_with("task-plan.1-work-2.json"));
    }

    /// A taken name is refused by the one sentence the spec spells.
    // §FS-rhei-agents.8.1
    #[test]
    fn a_taken_log_name_is_refused() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        let plan =
            plan_spawn_attempt(&runtime, dir.path(), "plan.1", "work", None, LogNumber::Entry(2));
        assert_eq!(plan.unaccounted_log_refusal(), None);
        fs::create_dir_all(runtime.join("logs")).expect("logs");
        fs::write(&plan.log, "planted").expect("plant");
        let refusal = plan.unaccounted_log_refusal().expect("refused");
        assert!(refusal.starts_with("refusing to spawn: "));
        assert!(
            refusal.ends_with("task-plan.1-work-2.log exists and no spawn record accounts for it")
        );
        assert_eq!(create_log_exclusively(&plan.log).err(), Some(refusal));
        assert_eq!(fs::read_to_string(&plan.log).expect("kept"), "planted");
    }
}
