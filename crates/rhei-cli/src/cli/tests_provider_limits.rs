mod provider_limits {
    use super::*;

    fn status(code: i32) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code as u32)
        }
    }

    fn codex_openai() -> ResolvedAgent {
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        let mut resolved = resolve_target_agent(
            "codex:openai:gpt-5.6-sol",
            None,
            &settings,
        )
        .expect("built-in codex target");
        resolved.timeout_secs = Some(60);
        resolved
    }

    /// Strict matching accepts either captured stream after decoration removal
    /// and rejects duplicates and transport/exit precedence violations.
    /// §FS-rhei-agents.2
    #[test]
    fn codex_provider_limit_classifier_is_strict() {
        let resolved = codex_openai();
        let observed = std::time::UNIX_EPOCH + Duration::from_secs(1_789_579_200);
        let signal = "\x1b[31mYou've hit your session limit · resets 10:20pm (Europe/Zurich)\x1b[0m";
        let classified = classify_provider_limit(
            &resolved,
            status(1),
            false,
            false,
            &[format!("  {signal}  ")],
            observed,
        );
        assert_eq!(
            classified.unwrap().next_attempt_at,
            "2026-09-16T20:21:00Z",
            "a future local reset uses the observation date"
        );
        assert!(classify_provider_limit(
            &resolved,
            status(1),
            false,
            false,
            &[signal.to_string(), signal.to_string()],
            observed,
        )
        .is_none());
        assert!(classify_provider_limit(
            &resolved,
            status(1),
            false,
            true,
            &[signal.to_string()],
            observed,
        )
        .is_none());
        let mut anthropic = resolved.clone();
        anthropic.model_provider = Some("anthropic".to_string());
        assert!(classify_provider_limit(
            &anthropic,
            status(1),
            false,
            false,
            &[signal.to_string()],
            observed,
        )
        .is_none());
        for lookalike in [
            "You've hit your session limit - resets 10:20pm (Europe/Zurich)",
            "You've hit your session limit · resets 10:20PM (Europe/Zurich)",
            "You've hit your session limit · resets 10:20pm (Not/AZone)",
        ] {
            assert!(classify_provider_limit(
                &resolved,
                status(1),
                false,
                false,
                &[lookalike.to_string()],
                observed,
            )
            .is_none());
        }
        let next_day = classify_provider_limit(
            &resolved,
            status(1),
            false,
            false,
            &[signal.to_string()],
            observed + Duration::from_secs(4 * 60 * 60),
        )
        .unwrap();
        assert_eq!(next_day.next_attempt_at, "2026-09-17T20:21:00Z");
        assert!(classify_provider_limit(
            &resolved,
            status(0),
            false,
            false,
            &[signal.to_string()],
            observed,
        )
        .is_none());
        assert!(classify_provider_limit(
            &resolved,
            status(1),
            true,
            false,
            &[signal.to_string()],
            observed,
        )
        .is_none());
    }

    /// Deadline resolution uses the next local date only after today's safe
    /// boundary and never guesses through DST gaps or overlaps. §FS-rhei-run.3.3
    #[test]
    fn provider_limit_deadlines_cover_day_and_dst_boundaries() {
        let zone: Tz = "Europe/Zurich".parse().unwrap();
        let ordinary = NaiveDate::from_ymd_opt(2026, 9, 16).unwrap();
        assert_eq!(
            unique_safe_boundary(zone, ordinary, 22, 20)
                .unwrap()
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            "2026-09-16T20:21:00Z"
        );
        let overlap = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
        assert!(unique_safe_boundary(zone, overlap, 2, 20).is_none());
        let gap = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
        assert!(unique_safe_boundary(zone, gap, 2, 20).is_none());
    }

    /// Repeated observations retain the later deadline and reset removes the
    /// provider-owned runtime map. §FS-rhei-run.3.3 §FS-rhei-reset.2
    #[test]
    fn provider_limit_metadata_keeps_later_deadline_and_clears() {
        let task = parse_task_id("1");
        let limit = |deadline: &str| ProviderLimit {
            identity: ProviderIdentity { agent: "codex".into(), provider: "openai".into() },
            signal: "signal".into(),
            observed_at: "2026-09-16T19:03:12Z".into(),
            next_attempt_at: deadline.into(),
        };
        let (metadata, _) =
            set_provider_limit_metadata(None, &task, "working", &limit("2026-09-16T20:21:00Z"));
        let (metadata, effective) = set_provider_limit_metadata(
            Some(&metadata),
            &task,
            "working",
            &limit("2026-09-16T20:00:00Z"),
        );
        assert_eq!(effective.next_attempt_at, "2026-09-16T20:21:00Z");
        let cleared = clear_runtime_provider_limits(Some(&metadata)).unwrap();
        assert!(provider_limit_for_task_state(Some(&cleared), &task, "working").is_none());
    }

    /// Both OSC terminators preserve a hyperlink's visible standalone signal;
    /// decoration removal must not relax standalone matching. §FS-rhei-agents.2
    #[test]
    fn provider_limit_osc_links_preserve_visible_text() {
        let resolved = codex_openai();
        let observed = std::time::UNIX_EPOCH + Duration::from_secs(1_789_579_200);
        let signal = "You've hit your session limit · resets 10:20pm (Europe/Zurich)";
        for open_end in ["\x07", "\x1b\\"] {
            for close_end in ["\x07", "\x1b\\"] {
                let linked = format!("\x1b]8;;https://example.invalid/reset{open_end}{signal}\x1b]8;;{close_end}");
                assert_eq!(strip_terminal_decoration(&linked), signal);
                let parsed = classify_provider_limit(
                    &resolved, status(1), false, false, std::slice::from_ref(&linked), observed,
                ).expect("one visible linked signal");
                assert_eq!(parsed.signal, signal);
                assert_eq!(parsed.next_attempt_at, "2026-09-16T20:21:00Z");
                for invalid in [format!("quoted: {linked}"), format!("{linked} trailing")] {
                    assert!(classify_provider_limit(
                        &resolved, status(1), false, false, &[invalid], observed,
                    ).is_none());
                }
            }
        }
    }

    /// Cleanup belongs to a matching invocation started at or after the wait,
    /// never an earlier sibling or a different transport. §FS-rhei-run.3.3
    #[test]
    fn provider_limit_cleanup_requires_eligible_resumption() {
        let resolved = codex_openai();
        let limit = ProviderLimit {
            identity: resolved_provider_identity(&resolved).unwrap(),
            signal: "signal".into(),
            observed_at: deadline(100),
            next_attempt_at: deadline(200),
        };
        let started = |at| std::time::UNIX_EPOCH + Duration::from_secs(at);
        assert!(!provider_limit_resumed(&limit, &resolved, started(199)));
        assert!(provider_limit_resumed(&limit, &resolved, started(200)));
        let mut unrelated = resolved.clone();
        unrelated.model_provider = Some("other".into());
        assert!(!provider_limit_resumed(&limit, &unrelated, started(201)));
        let extended = ProviderLimit { next_attempt_at: deadline(300), ..limit };
        assert!(!provider_limit_resumed(&extended, &resolved, started(200)));
    }

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

    fn deadline(epoch: u64) -> String {
        DateTime::<Utc>::from(std::time::UNIX_EPOCH + Duration::from_secs(epoch))
            .to_rfc3339_opts(SecondsFormat::Secs, true)
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
        assert_eq!(
            earliest_pending_agent_deadline(
                &rhei,
                &machines,
                &settings,
                &default_run_options(),
                &None,
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
            remaining_work_is_only_gating_or_poll_blocked(&rhei, &machines, &None),
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

        assert_eq!(
            earliest_pending_agent_deadline(
                &rhei,
                &machines,
                &settings,
                &default_run_options(),
                &None,
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
            !remaining_work_is_only_gating_or_poll_blocked(&polled, &machines, &None),
            "a claim really does stop a poll from reaching its next attempt"
        );

        let gated = rhei_core::parse(
            "# Rhei: Claims\n\n## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** review\n\
             **Assignee:** alice\n",
        )
        .expect("parse the gated plan");
        assert!(
            remaining_work_is_only_gating_or_poll_blocked(&gated, &machines, &None),
            "releasing the claim moves nothing while the gate holds"
        );
    }
}
