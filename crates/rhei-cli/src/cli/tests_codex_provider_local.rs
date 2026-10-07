mod provider_limit_codex_local_resolution {
    use super::*;

    /// Run the actual classifier with a fixed completion clock and injected
    /// local resolver, without changing process timezone. §FS-rhei-run.3.3
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

    /// Resolve only unique civil times using a deterministic non-UTC zone.
    /// §FS-rhei-run.3.3
    fn zurich(minute: &NaiveDateTime) -> Option<DateTime<Utc>> {
        chrono_tz::Europe::Zurich
            .from_local_datetime(minute)
            .single()
            .map(|value| value.with_timezone(&Utc))
    }

    /// Pin every English month, summer/winter offsets, noon/midnight, leap-day
    /// and year rollover against literal UTC deadlines. §FS-rhei-run.3.3
    #[test]
    fn codex_local_classifier_pins_zurich_months_and_rollovers() {
        let observed = Utc.with_ymd_and_hms(2026, 10, 7, 0, 0, 0).unwrap();
        let cases = [
            ("Oct 9th, 2026 11:19 PM", "2026-10-09T21:20:00Z"),
            ("Jan 1st, 2032 12:00 AM", "2031-12-31T23:01:00Z"),
            ("Feb 1st, 2032 12:00 PM", "2032-02-01T11:01:00Z"),
            ("Mar 1st, 2032 11:19 PM", "2032-03-01T22:20:00Z"),
            ("Apr 1st, 2032 11:19 PM", "2032-04-01T21:20:00Z"),
            ("May 1st, 2032 11:19 PM", "2032-05-01T21:20:00Z"),
            ("Jun 1st, 2032 11:19 PM", "2032-06-01T21:20:00Z"),
            ("Jul 1st, 2032 11:19 PM", "2032-07-01T21:20:00Z"),
            ("Aug 1st, 2032 11:19 PM", "2032-08-01T21:20:00Z"),
            ("Sep 1st, 2032 11:19 PM", "2032-09-01T21:20:00Z"),
            ("Oct 1st, 2032 11:19 PM", "2032-10-01T21:20:00Z"),
            ("Nov 1st, 2032 11:19 PM", "2032-11-01T22:20:00Z"),
            ("Dec 1st, 2032 11:19 PM", "2032-12-01T22:20:00Z"),
            ("Jan 3rd, 2032 1:59 AM", "2032-01-03T01:00:00Z"),
            ("Feb 29th, 2032 11:59 PM", "2032-02-29T23:00:00Z"),
            ("Dec 31st, 2032 11:59 PM", "2032-12-31T23:00:00Z"),
            ("Jan 1st, 9999 12:00 AM", "9998-12-31T23:01:00Z"),
            ("Dec 31st, 9999 11:59 PM", "9999-12-31T23:00:00Z"),
        ];
        for (text, expected) in cases {
            let limit = classify(text, observed, &zurich).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(limit.next_attempt_at, expected, "{text}");
            assert_eq!(limit.observed_at, "2026-10-07T00:00:00Z");
        }
        // Year 0001 is valid syntax and reaches local resolution, but is stale
        // at this portable completion clock. §FS-rhei-agents.2.3 §FS-rhei-run.3.3
        let calls = std::cell::Cell::new(0);
        assert!(classify("Jan 1st, 0001 12:00 AM", observed, &|minute| {
            assert_eq!(minute.date(), NaiveDate::from_ymd_opt(1, 1, 1).unwrap());
            calls.set(calls.get() + 1);
            zurich(minute)
        })
        .is_none());
        assert_eq!(calls.get(), 2, "both valid year-0001 minutes reach the resolver");
    }

    /// Gap/overlap in either minute and unavailable OS-local conversion remain
    /// ordinary failures through the full classifier. §FS-rhei-run.3.3
    #[test]
    fn codex_local_classifier_rejects_dst_and_unavailable_resolution() {
        let observed = Utc.with_ymd_and_hms(2026, 10, 7, 0, 0, 0).unwrap();
        for text in [
            "Mar 28th, 2032 1:59 AM",
            "Mar 28th, 2032 2:20 AM",
            "Oct 31st, 2032 1:59 AM",
            "Oct 31st, 2032 2:20 AM",
        ] {
            assert!(classify(text, observed, &zurich).is_none(), "{text}");
        }
        assert!(classify("Oct 9th, 2026 11:19 PM", observed, &|_| None).is_none());
        let calls = std::cell::Cell::new(0);
        assert!(classify("Oct 9th, 2026 11:19 PM", observed, &|minute| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                zurich(minute)
            } else {
                None
            }
        })
        .is_none());
        assert_eq!(calls.get(), 2, "unavailable following-minute conversion fails closed");
    }

    /// The following minute must be strictly future; an absolute date does not
    /// roll forward when equal, expired, or on a later day. §FS-rhei-run.3.3
    #[test]
    fn codex_local_classifier_accepts_only_before_absolute_boundary() {
        let boundary = Utc.with_ymd_and_hms(2026, 10, 9, 21, 20, 0).unwrap();
        let text = "Oct 9th, 2026 11:19 PM";
        let limit = classify(text, boundary - TimeDelta::seconds(1), &zurich).unwrap();
        assert_eq!(limit.next_attempt_at, "2026-10-09T21:20:00Z");
        for observed in [boundary, boundary + TimeDelta::seconds(1), boundary + TimeDelta::days(1)]
        {
            assert!(classify(text, observed, &zurich).is_none(), "{observed}");
        }
    }
}
