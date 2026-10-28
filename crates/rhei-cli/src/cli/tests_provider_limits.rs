include!("tests_codex_provider_limits.rs");

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

    pub(super) fn codex_openai() -> ResolvedAgent {
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
    /// §FS-rhei-agents.2.3
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
        // The second provider of the closed set is recognized on the same
        // terms, and the registry id it ran as is recorded rather than tested.
        // §FS-rhei-agents.2.3
        let mut anthropic = resolved.clone();
        anthropic.model_provider = Some("anthropic".to_string());
        let recognized = classify_provider_limit(
            &anthropic,
            status(1),
            false,
            false,
            &[signal.to_string()],
            observed,
        )
        .expect("a provider in the closed set is recognized");
        assert_eq!(recognized.identity.provider, "anthropic");
        assert_eq!(recognized.identity.agent, "codex", "the registry id is recorded, not tested");
        // The first three are the long-standing rejections; the last three are
        // decisions §FS-rhei-agents.2.3 now records: a dated reset names a date
        // the grammar does not carry, and the period vocabulary is closed.
        for lookalike in [
            "You've hit your session limit - resets 10:20pm (Europe/Zurich)",
            "You've hit your session limit · resets 10:20PM (Europe/Zurich)",
            "You've hit your session limit · resets 10:20pm (Not/AZone)",
            "You've hit your session limit · resets Oct 1, 5:59am (Europe/Zurich)",
            "You've hit your 5-hour limit · resets 6am (Europe/Zurich)",
            "You've hit your Weekly limit · resets 6am (Europe/Zurich)",
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

    /// The refusal is one grammar with two period words and an optional minute
    /// group. `weekly` parks on exactly the terms `session` already does, and a
    /// reset with no minutes is that hour at `00` and nothing else: one
    /// arithmetic, not two. §FS-rhei-agents.2.3 §FS-rhei-run.3.3
    #[test]
    fn both_period_words_and_a_bare_hour_reset_are_recognized() {
        let resolved = codex_openai();
        let observed = std::time::UNIX_EPOCH + Duration::from_secs(1_789_579_200);
        let classify = |line: &str| {
            classify_provider_limit(
                &resolved,
                status(1),
                false,
                false,
                &[line.to_string()],
                observed,
            )
        };

        // One word away from the control fixture above, and nothing else: the
        // same reset must reach the same deadline.
        let weekly = classify("You've hit your weekly limit · resets 10:20pm (Europe/Zurich)")
            .expect("the period word is not what carries the claim");
        assert_eq!(weekly.next_attempt_at, "2026-09-16T20:21:00Z");

        // A reset with no minutes, under either period word. Both boundaries
        // fall before the observation, so both take the next local date.
        let session_bare = classify("You've hit your session limit · resets 6am (Europe/Zurich)")
            .expect("a bare hour names a reset instant");
        assert_eq!(session_bare.next_attempt_at, "2026-09-17T04:01:00Z");
        let weekly_bare = classify("You've hit your weekly limit · resets 2pm (Europe/Zurich)")
            .expect("a bare hour names a reset instant");
        assert_eq!(weekly_bare.next_attempt_at, "2026-09-17T12:01:00Z");

        // An absent `:<mm>` means `00`, asserted rather than implied: the two
        // spellings are the same instant to the byte.
        let spelled = classify("You've hit your session limit · resets 6:00am (Europe/Zurich)")
            .expect("the spelled-out equivalent is unchanged");
        assert_eq!(session_bare.next_attempt_at, spelled.next_attempt_at);

        // The record keeps the matched sentence verbatim and derives no period
        // field from it. §FS-rhei-run.3.3
        assert_eq!(
            weekly_bare.signal,
            "You've hit your weekly limit · resets 2pm (Europe/Zurich)"
        );
    }

    /// Recognition keys on the resolved provider, in a closed set, and never on
    /// the agent registry id: an entry the user named may park, a provider
    /// outside the set never does, and a state resolving no provider has no
    /// execution identity to key a wait on. §FS-rhei-agents.2.3
    #[test]
    fn provider_limit_recognition_keys_on_the_closed_provider_set() {
        let observed = std::time::UNIX_EPOCH + Duration::from_secs(1_789_579_200);
        let signal = "You've hit your session limit · resets 10:20pm (Europe/Zurich)";
        let classify = |resolved: &ResolvedAgent, timed_out, interrupted| {
            classify_provider_limit(
                resolved,
                status(1),
                timed_out,
                interrupted,
                &[signal.to_string()],
                observed,
            )
        };
        let entry = |agent: &str, provider: Option<&str>| {
            let mut resolved = codex_openai();
            resolved.agent = agent.into();
            resolved.model_provider = provider.map(str::to_string);
            resolved
        };

        // A provider outside the set stays an ordinary process result whatever
        // the entry is named, and the set is matched case-sensitively.
        assert!(classify(&entry("codex", Some("acme")), false, false).is_none());
        assert!(classify(&entry("codex", Some("Anthropic")), false, false).is_none());

        // Neither direction of the agent registry id decides anything.
        let user_named = classify(&entry("cld1", Some("openai")), false, false)
            .expect("a user-named entry on openai is recognized");
        assert_eq!(user_named.identity.agent, "cld1");
        let codex_elsewhere = classify(&entry("codex", Some("anthropic")), false, false)
            .expect("the built-in codex entry on anthropic is recognized");
        assert_eq!(codex_elsewhere.identity.provider, "anthropic");

        // A state that resolves an agent but no provider has no execution
        // identity, so there is nothing to key a wait on.
        assert!(classify(&entry("cld1", None), false, false).is_none());

        // Timeout and interruption are still classified first.
        let anthropic = entry("cld1", Some("anthropic"));
        assert!(classify(&anthropic, false, false).is_some());
        assert!(classify(&anthropic, true, false).is_none());
        assert!(classify(&anthropic, false, true).is_none());
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
        let cleared = clear_runtime_task_metadata(Some(&metadata)).unwrap();
        assert!(provider_limit_for_task_state(Some(&cleared), &task, "working").is_none());
    }

    /// Both OSC terminators preserve a hyperlink's visible standalone signal;
    /// decoration removal must not relax standalone matching. §FS-rhei-agents.2.3
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

    pub(super) fn deadline(epoch: u64) -> String {
        DateTime::<Utc>::from(std::time::UNIX_EPOCH + Duration::from_secs(epoch))
            .to_rfc3339_opts(SecondsFormat::Secs, true)
    }
}
