mod provider_limit_codex_native {
    use super::*;

    fn classify(
        text: &str,
        observed: DateTime<Utc>,
        resolve: &impl Fn(&NaiveDateTime) -> Option<DateTime<Utc>>,
    ) -> Option<ProviderLimit> {
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1 << 8)
        };
        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1)
        };
        let settings = RheiSettings { agents: built_in_agents(), ..RheiSettings::default() };
        let resolved = resolve_target_agent("codex:openai:alpha", None, &settings).unwrap();
        let signal = format!(
            "You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at {text}."
        );
        classify_provider_limit_with_local_resolver(
            &resolved,
            status,
            false,
            false,
            &[signal],
            observed.into(),
            resolve,
        )
    }

    fn observed() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 7, 0, 0, 0).unwrap()
    }

    /// Exercise the native query mapping Windows uses, including failure of
    /// either minute and zero-offset success, through full sentences. §FS-rhei-run.3.3
    #[test]
    fn codex_native_query_preserves_failure_and_uniqueness() {
        use chrono::{FixedOffset, LocalResult};
        use provider_local_time::resolve_native_query;
        let text = "Jan 1st, 2032 11:19 PM";
        let utc = FixedOffset::east_opt(0).unwrap();
        let other = FixedOffset::east_opt(3600).unwrap();
        for failed_call in [1, 2] {
            for failure in [LocalResult::None, LocalResult::Ambiguous(utc, other)] {
                let calls = std::cell::Cell::new(0);
                let query = |_: &NaiveDateTime| {
                    calls.set(calls.get() + 1);
                    if calls.get() == failed_call {
                        failure
                    } else {
                        LocalResult::Single(utc)
                    }
                };
                assert!(classify(text, observed(), &|minute| resolve_native_query(minute, &query))
                    .is_none());
                assert_eq!(calls.get(), failed_call);
            }
        }
        let limit = classify(text, observed(), &|minute| {
            resolve_native_query(minute, &|_| LocalResult::Single(utc))
        })
        .unwrap();
        assert_eq!(limit.next_attempt_at, "2032-01-01T23:20:00Z");
        assert_eq!(limit.deadline_epoch(), Some(1_956_612_000));
    }

    /// The unchanged scheduling reader must read every accepted UTC boundary,
    /// including native-query upper-year conversions. §FS-rhei-run.3.3
    #[test]
    fn codex_native_query_upper_year_requires_persistence_round_trip() {
        use chrono::{FixedOffset, LocalResult};
        for (offset, text, expected) in [
            (0, "Dec 31st, 9999 11:59 PM", None),
            (-18000, "Dec 31st, 9999 7:00 PM", None),
            (0, "Dec 31st, 9999 11:58 PM", Some("9999-12-31T23:59:00Z")),
            (-18000, "Dec 31st, 9999 6:58 PM", Some("9999-12-31T23:59:00Z")),
            (3600, "Dec 31st, 9999 11:59 PM", Some("9999-12-31T23:00:00Z")),
        ] {
            let offset = FixedOffset::east_opt(offset).unwrap();
            let limit = classify(text, observed(), &|minute| {
                provider_local_time::resolve_native_query(minute, &|_| LocalResult::Single(offset))
            });
            assert_eq!(
                limit.as_ref().map(|limit| limit.next_attempt_at.as_str()),
                expected,
                "{text}: {offset}"
            );
            if let Some(limit) = limit {
                let persisted = provider_limit_from_value(&provider_limit_value(&limit)).unwrap();
                assert_eq!(persisted, limit);
                assert!(persisted.deadline_epoch().is_some());
            }
        }
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use provider_local_time::{select_unix_zone, LocalZone};

        include!("tests_codex_provider_unix_sources.rs");
        include!("tests_codex_provider_unix_dates.rs");
    }
}
