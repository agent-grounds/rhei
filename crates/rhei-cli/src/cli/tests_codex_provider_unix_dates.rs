/// Test TZif transitions and POSIX extrapolation through the production adapter,
/// with each exact edge appearing as reported or following minute. §FS-rhei-run.3.3
#[test]
fn codex_native_dst_edges_use_half_open_intervals() {
    let dir = tempfile::tempdir().unwrap();
    let copied = dir.path().join("localtime");
    std::fs::write(&copied, tzif(3600, true)).unwrap();
    for explicit in [None, Some("CET-1CEST,M3.5.0,M10.5.0/3")] {
        let zone = select_unix_zone(explicit, &copied, &|| None).unwrap();
        for (text, expected) in [
            ("Mar 28th, 2032 1:58 AM", Some("2032-03-28T00:59:00Z")),
            ("Mar 28th, 2032 1:59 AM", None),
            ("Mar 28th, 2032 2:00 AM", None),
            ("Mar 28th, 2032 2:59 AM", None),
            ("Mar 28th, 2032 3:00 AM", Some("2032-03-28T01:01:00Z")),
            ("Oct 31st, 2032 1:58 AM", Some("2032-10-30T23:59:00Z")),
            ("Oct 31st, 2032 1:59 AM", None),
            ("Oct 31st, 2032 2:00 AM", None),
            ("Oct 31st, 2032 2:59 AM", None),
            ("Oct 31st, 2032 3:00 AM", Some("2032-10-31T02:01:00Z")),
        ] {
            let limit = classify(text, observed(), &|minute| zone.resolve(minute));
            assert_eq!(
                limit.as_ref().map(|limit| limit.next_attempt_at.as_str()),
                expected,
                "{text}: {explicit:?}"
            );
        }
    }
}

/// Expiry uses the absolute safe boundary after native conversion; failure of
/// either minute must survive the conversion/query seam. §FS-rhei-run.3.3
#[test]
fn codex_native_conversion_failure_and_future_comparison() {
    use chrono::Timelike;
    let dir = tempfile::tempdir().unwrap();
    let copied = dir.path().join("localtime");
    std::fs::write(&copied, tzif(3600, true)).unwrap();
    let zone = select_unix_zone(None, &copied, &|| None).unwrap();
    let text = "Oct 9th, 2026 11:19 PM";
    let boundary = Utc.with_ymd_and_hms(2026, 10, 9, 21, 20, 0).unwrap();
    let limit =
        classify(text, boundary - TimeDelta::seconds(1), &|minute| zone.resolve(minute)).unwrap();
    assert_eq!(limit.next_attempt_at, "2026-10-09T21:20:00Z");
    for at in [boundary, boundary + TimeDelta::seconds(1), boundary + TimeDelta::days(1)] {
        assert!(classify(text, at, &|minute| zone.resolve(minute)).is_none());
    }
    for unavailable in [19, 20] {
        assert!(classify(text, observed(), &|minute| {
            if minute.minute() == unavailable {
                None
            } else {
                zone.resolve(minute)
            }
        })
        .is_none());
    }
}

/// UTC/western boundaries outside the reader's year range are ordinary;
/// representable endpoints survive the real metadata reader. §FS-rhei-run.3.3
#[test]
fn codex_native_upper_year_deadlines_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    for (source, text, expected) in [
        ("UTC0", "Dec 31st, 9999 11:59 PM", None),
        ("XST5", "Dec 31st, 9999 11:59 PM", None),
        ("XST5", "Dec 31st, 9999 6:59 PM", None),
        ("UTC0", "Dec 31st, 9999 11:58 PM", Some("9999-12-31T23:59:00Z")),
        ("XST5", "Dec 31st, 9999 6:58 PM", Some("9999-12-31T23:59:00Z")),
        ("XST-1", "Dec 31st, 9999 11:59 PM", Some("9999-12-31T23:00:00Z")),
    ] {
        let zone = select_unix_zone(Some(source), &missing, &|| None).unwrap();
        let limit = classify(text, observed(), &|minute| zone.resolve(minute));
        assert_eq!(
            limit.as_ref().map(|limit| limit.next_attempt_at.as_str()),
            expected,
            "{text}: {source}"
        );
        if let Some(limit) = limit {
            let persisted = provider_limit_from_value(&provider_limit_value(&limit)).unwrap();
            assert_eq!(persisted, limit);
            assert!(persisted.deadline_epoch().is_some());
        }
    }
}
