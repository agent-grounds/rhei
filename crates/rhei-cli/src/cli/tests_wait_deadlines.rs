// How the pass loop reads a wait: the deadline it sleeps for and publishes,
// and the plan-wide judgment of whether what is left is only waiting.
//
// Its own part because these ask the scheduler rather than the provider-limit
// parser beside them: a recognized limit is one input here, beside the poll,
// the claim, the gate, and the spawn records a spent bound is read from.

// §AR-source-file-size.3 §FS-rhei-run.5.1 §FS-rhei-run-report.3.1

mod wait_deadlines {
    use super::provider_limits::{codex_openai, deadline};
    use super::*;

    fn provider_machine(poll: bool) -> rhei_validator::StateMachine {
        let self_loop = if poll {
            "  - from: working\n    to: working\n    condition: pollAttempts < pollMaxAttempts\n"
        } else {
            ""
        };
        let poll = if poll {
            "    poll:\n      interval: 1m\n      max_attempts: 3\n"
        } else {
            ""
        };
        rhei_validator::StateMachine::from_yaml_str(&format!(
            r#"name: provider-limit-test
version: 1
states:
  working:
    initial: true
    target: codex:openai:gpt-5.6-sol
{poll}  completed:
    final: true
transitions:
{self_loop}  - from: working
    to: completed
"#
        ))
        .expect("provider-limit state machine")
    }

    /// Shared execution identity uses the latest active deadline and ignores a
    /// record belonging to an obsolete task state. §FS-rhei-run.3.3
    #[test]
    fn shared_identity_uses_latest_active_non_stale_deadline() {
        let mut rhei = rhei_core::parse(
            "# Rhei: Limits\n\n## Tasks\n\n### Task 1: First\n**State:** working\n\n### Task 2: Second\n**State:** working\n",
        )
        .unwrap();
        let now = current_unix_secs();
        let identity = ProviderIdentity { agent: "codex".into(), provider: "openai".into() };
        let make = |at| ProviderLimit {
            identity: identity.clone(),
            signal: "signal".into(),
            observed_at: deadline(now),
            next_attempt_at: deadline(at),
        };
        let (metadata, _) =
            set_provider_limit_metadata(None, &parse_task_id("1"), "working", &make(now + 60));
        let (metadata, _) = set_provider_limit_metadata(
            Some(&metadata),
            &parse_task_id("2"),
            "working",
            &make(now + 120),
        );
        rhei.metadata = Some(metadata);
        let machines = rhei_validator::MachineSet::single(provider_machine(false));
        assert_eq!(
            active_provider_deadline_for_identity(&rhei, &machines, &identity, now),
            Some(now + 120)
        );
        let codex = codex_openai();
        assert_eq!(
            resolved_provider_deadline(&rhei, &machines, &codex, now),
            Some(now + 120),
            "queued work sharing the reporting identity is suppressed"
        );
        let mut unrelated = codex.clone();
        unrelated.model_provider = Some("anthropic".to_string());
        assert_eq!(
            resolved_provider_deadline(&rhei, &machines, &unrelated, now),
            None,
            "another provider remains eligible"
        );

        let (metadata, _) = set_provider_limit_metadata(
            rhei.metadata.as_ref(),
            &parse_task_id("2"),
            "obsolete",
            &make(now + 180),
        );
        rhei.metadata = Some(metadata);
        assert_eq!(
            active_provider_deadline_for_identity(&rhei, &machines, &identity, now),
            Some(now + 120),
            "the current-state record remains authoritative; obsolete state data is ignored"
        );
    }

    /// An expired identity record is inactive for admission but remains an
    /// immediate scheduler wake-up until the queued task is rescanned.
    /// §FS-rhei-run.3.3 §FS-rhei-run.5.1
    #[test]
    fn expired_identity_deadline_requests_immediate_scheduler_rescan() {
        let mut rhei = rhei_core::parse(
            "# Rhei: Limits\n\n## Tasks\n\n### Task 1: First\n**State:** working\n",
        )
        .unwrap();
        let now = current_unix_secs();
        let identity = ProviderIdentity { agent: "codex".into(), provider: "openai".into() };
        let limit = ProviderLimit {
            identity: identity.clone(),
            signal: "signal".into(),
            observed_at: deadline(now - 2),
            next_attempt_at: deadline(now - 1),
        };
        let (metadata, _) =
            set_provider_limit_metadata(None, &parse_task_id("1"), "working", &limit);
        rhei.metadata = Some(metadata);
        let machines = rhei_validator::MachineSet::single(provider_machine(false));
        let resolved = codex_openai();

        assert_eq!(
            resolved_provider_deadline(&rhei, &machines, &resolved, now),
            None,
            "expired records no longer suppress admission"
        );
        assert_eq!(
            resolved_provider_eligibility_deadline(&rhei, &machines, &resolved, now),
            Some(now),
            "the scheduler must rescan instead of concluding the run"
        );
    }

    /// Poll and provider waits share one scheduler eligibility instant: the
    /// task cannot resume until the later condition permits it. §FS-rhei-run.5.1
    #[test]
    fn provider_and_poll_deadlines_combine_by_task() {
        let mut rhei = rhei_core::parse(
            "# Rhei: Limits\n\n## Tasks\n\n### Task 1: First\n**State:** working\n",
        )
        .unwrap();
        let task = parse_task_id("1");
        let now = current_unix_secs();
        let metadata = set_poll_next_attempt_metadata(None, &task, "working", now + 180, 1);
        let limit = ProviderLimit {
            identity: ProviderIdentity { agent: "codex".into(), provider: "openai".into() },
            signal: "signal".into(),
            observed_at: deadline(now),
            next_attempt_at: deadline(now + 120),
        };
        let (metadata, _) =
            set_provider_limit_metadata(Some(&metadata), &task, "working", &limit);
        rhei.metadata = Some(metadata);
        let machines = rhei_validator::MachineSet::single(provider_machine(true));
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        assert_eq!(
            earliest_pending_agent_deadline(
                &rhei,
                &machines,
                &settings,
                &None,
                &SpawnSiteRoots { settings: Some(&settings), ..SpawnSiteRoots::plan_only(dir.path(), &runtime) },
            ),
            Some(now + 180)
        );
    }

    /// The provider-limit term of the plan-wide deliberate-wait judgment.
    ///
    /// The judgment and the deadline scan diverged over provider limits: the
    /// scan knew about them and the judgment did not. In the continuous mode
    /// nobody noticed, because the run simply slept. Under `--until-idle` the
    /// split produces a concretely wrong answer — the one wait the report
    /// calls calm, reported as attention and exited in the failure category.
    /// A task parked by a recognized provider limit is a deliberate wait.
    /// §FS-rhei-run-report.3.1 §FS-rhei-run.3.3
    #[test]
    fn a_provider_limit_only_wait_is_a_deliberate_wait() {
        let mut rhei = rhei_core::parse(
            "# Rhei: Limits\n\n## Tasks\n\n### Task 1: Parked\n**State:** working\n",
        )
        .expect("parse plan");
        let now = current_unix_secs();
        let limit = ProviderLimit {
            identity: ProviderIdentity { agent: "codex".into(), provider: "openai".into() },
            signal: "signal".into(),
            observed_at: deadline(now),
            next_attempt_at: deadline(now + 1800),
        };
        let (metadata, _) =
            set_provider_limit_metadata(None, &parse_task_id("1"), "working", &limit);
        rhei.metadata = Some(metadata);
        let machines = rhei_validator::MachineSet::single(provider_machine(false));

        assert!(
            remaining_work_is_only_gating_or_poll_blocked(&rhei, &machines, &None, &SpawnSiteRoots::plan_only(Path::new("."), Path::new("runtime"))),
            "a parked task resumes when its deadline elapses, with nothing for an \
             operator to repair"
        );
    }

    /// The only-blocker filter on the reported next attempt.
    ///
    /// A deadline is a schedule hint until something says whether its expiry
    /// can actually make the task runnable. Task 2's poll is real and still
    /// ahead, but task 1's gate is what holds it: expiry alone changes
    /// nothing, so the deadline contributes no next attempt and a timer woken
    /// for it would find the same gate.
    /// §FS-rhei-run.5.1
    #[test]
    fn a_deadline_behind_a_gated_prior_contributes_no_next_attempt() {
        let mut rhei = rhei_core::parse(
            "# Rhei: Limits\n\n## Tasks\n\n### Task 1: Gate\n**State:** review\n\n\
             ### Task 2: Behind the gate\n**State:** working\n**Prior:** Task 1\n",
        )
        .expect("parse plan");
        let now = current_unix_secs();
        rhei.metadata =
            Some(set_poll_next_attempt_metadata(None, &parse_task_id("2"), "working", now + 600, 1));
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: gated-provider
version: 1
states:
  review:
    initial: true
    gating: true
  working:
    target: codex:openai:gpt-5.6-sol
    poll:
      interval: 1m
      max_attempts: 3
  completed:
    final: true
transitions:
  - from: review
    to: completed
  - from: working
    to: working
    condition: pollAttempts < pollMaxAttempts
  - from: working
    to: completed
"#,
        )
        .expect("gated provider state machine");
        let machines = rhei_validator::MachineSet::single(machine);
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };

        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        assert_eq!(
            earliest_pending_agent_deadline(
                &rhei,
                &machines,
                &settings,
                &None,
                &SpawnSiteRoots { settings: Some(&settings), ..SpawnSiteRoots::plan_only(dir.path(), &runtime) },
            ),
            None,
            "expiry cannot release a task its prior's gate still holds"
        );
    }

    /// The `**Assignee:**` rung of the plan-wide judgment, both ways round.
    ///
    /// §FS-rhei-run.3 step 9 states the pair: gated *and* claimed classifies as
    /// gated and is idle-compatible, because releasing the claim moves nothing
    /// while the gate holds; polled and claimed classifies as claimed and is
    /// not, because a claim really does stop a poll from reaching its next
    /// attempt. The judgment had no such rung, so the polled half read as a
    /// deliberate wait while the same run's ledger row read `claimed by alice`.
    /// §FS-rhei-run-report.3.1
    #[test]
    fn a_claim_outranks_a_poll_and_a_gate_outranks_a_claim() {
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"name: claimed-poll
version: 1
states:
  review: { initial: true, gating: true }
  working:
    target: codex:openai:gpt-5.6-sol
    poll: { interval: 1m, max_attempts: 3 }
  completed: { final: true }
transitions:
  - { from: review, to: completed }
  - { from: working, to: working, condition: pollAttempts < pollMaxAttempts }
  - { from: working, to: completed }
"#,
        )
        .expect("claimed poll state machine");
        let machines = rhei_validator::MachineSet::single(machine);

        let mut polled = rhei_core::parse(
            "# Rhei: Claims\n\n## Tasks\n\n### Task 1: Retries later\n**State:** working\n\
             **Assignee:** alice\n",
        )
        .expect("parse the polled plan");
        polled.metadata = Some(set_poll_next_attempt_metadata(
            None,
            &parse_task_id("1"),
            "working",
            current_unix_secs() + 600,
            1,
        ));
        assert!(
            !remaining_work_is_only_gating_or_poll_blocked(&polled, &machines, &None, &SpawnSiteRoots::plan_only(Path::new("."), Path::new("runtime"))),
            "a claim really does stop a poll from reaching its next attempt"
        );

        let gated = rhei_core::parse(
            "# Rhei: Claims\n\n## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** review\n\
             **Assignee:** alice\n",
        )
        .expect("parse the gated plan");
        assert!(
            remaining_work_is_only_gating_or_poll_blocked(&gated, &machines, &None, &SpawnSiteRoots::plan_only(Path::new("."), Path::new("runtime"))),
            "releasing the claim moves nothing while the gate holds"
        );
    }

    /// A program poll bounded at three attempts, as the deadline scan meets it.
    const POLL_MACHINE: &str = r#"name: poll
version: 1
states:
  waiting:
    program:
      command: "true"
    poll:
      interval: 3s
      max_attempts: 3
  done:
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: done
"#;

    /// Once the reverted attempts have spent the bound, the deadline the last
    /// charge left adds nothing to the scan, even when it is due at the very
    /// second the pass read its ready set. One attempt short, it still does.
    // §FS-rhei-run.3.7.4 §FS-rhei-run.5.1
    #[test]
    fn a_spent_poll_bound_adds_no_deadline_to_the_scan() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let (root, runtime) = (dir.path(), dir.path().join("runtime"));
        let machine = rhei_validator::StateMachine::from_yaml_str(POLL_MACHINE).expect("machine");
        let machines = rhei_validator::MachineSet::single(machine.clone());
        let plan = "# Rhei: Poll\n\n## Tasks\n\n### Task 1: One\n**State:** waiting\n";
        let mut rhei = rhei_core::parse(plan).expect("plan");
        let since = current_unix_secs();
        rhei.metadata =
            Some(set_poll_next_attempt_metadata(None, &parse_task_id("1"), "waiting", since, 1));
        let settings = RheiSettings::default();
        let spawns = SpawnSiteRoots::plan_only(root, &runtime);
        let scan = || agent_deadline_since(&rhei, &machines, &settings, &None, &spawns, since);
        for attempt in 1..=3 {
            assert_eq!(scan(), Some(since), "attempt {attempt} is still to come");
            record_reverted_poll_attempt(&machine, root, &runtime);
        }
        assert_eq!(scan(), None, "no attempt follows a spent bound, so none is waited for");
    }

    /// A program poll asked with no run behind it answers from its spawn record
    /// alone: it resolves no agents, so it reads no settings, and a project
    /// settings file that does not load leaves its answer as it is.
    // §FS-rhei-run.5.1 §FS-rhei-run.3.7.4
    #[test]
    fn a_program_poll_answers_its_spent_bound_without_reading_settings() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let (root, runtime) = (dir.path(), dir.path().join("runtime"));
        let settings_file = root.join(PROJECT_SETTINGS_RELATIVE_PATH);
        fs::create_dir_all(settings_file.parent().expect("settings parent")).expect("mkdir");
        fs::write(&settings_file, "{ not json").expect("write malformed settings");
        assert!(load_merged_settings(root).is_err(), "the settings do not load");
        let machine = rhei_validator::StateMachine::from_yaml_str(POLL_MACHINE).expect("machine");
        let plan = "# Rhei: Poll\n\n## Tasks\n\n### Task 1: One\n**State:** waiting\n";
        let mut rhei = rhei_core::parse(plan).expect("plan");
        let since = current_unix_secs();
        rhei.metadata =
            Some(set_poll_next_attempt_metadata(None, &parse_task_id("1"), "waiting", since, 1));
        let spawns = SpawnSiteRoots { settings: None, ..SpawnSiteRoots::plan_only(root, &runtime) };
        for _ in 1..=3 {
            record_reverted_poll_attempt(&machine, root, &runtime);
        }
        let task = &rhei.tasks[0];
        assert_eq!(spawns.pending_poll_deadline(&rhei, &machine, task, "waiting", since), None);
    }

    /// Spawn the program poll's next attempt under `root` and record it as an
    /// attempt whose edit was reverted, charged to its bound.
    fn record_reverted_poll_attempt(
        machine: &rhei_validator::StateMachine,
        root: &Path,
        runtime: &Path,
    ) {
        let (plan, _) =
            plan_named_spawn(machine, root, runtime, "1", "waiting", 1, None).expect("plan");
        plan.record_spawn(SpawnEnding {
            task_id: "1",
            state_name: "waiting",
            kind: "program",
            worker: "true",
            started: "2026-10-28T10:00:00Z",
            ended: "2026-10-28T10:00:01Z",
            duration: "1s",
            code: Some(75),
            ending: "exited",
        });
        let edit = RevertedEdit {
            location: "plan.md:1".into(),
            message: "broken".into(),
            text: root.join("reverted.md"),
        };
        record_spawn_revert(&plan.record, &edit).expect("revert kept on the record");
    }
}
