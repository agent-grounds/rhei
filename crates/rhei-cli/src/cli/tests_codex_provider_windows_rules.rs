/// Native query failure, partial/invalid dates and checked bias overflow remain
/// unavailable. A failure in the boundary year's query cannot become a wait.
/// §FS-rhei-run.3.3
#[test]
fn codex_windows_native_rule_query_and_data_failures_remain_ordinary() {
    let text = "Dec 31st, 2032 11:59 PM";
    for unavailable_year in [2032, 2033] {
        assert!(classify(text, observed(), &|minute| {
            resolve_query(minute, &|year| (year != unavailable_year).then(|| zurich_rules(false)))
        })
        .is_none());
    }
    let valid = zurich_rules(false);
    for invalid in [
        YearRules { bias: i32::MAX, ..valid },
        YearRules { standard_bias: i32::MAX, ..valid },
        YearRules { daylight_bias: i32::MAX, ..valid },
        YearRules { daylight: TransitionDate::default(), ..valid },
        YearRules { standard: TransitionDate::default(), ..valid },
        YearRules { daylight: TransitionDate { month: 13, ..valid.daylight }, ..valid },
        YearRules { daylight: TransitionDate { weekday: 7, ..valid.daylight }, ..valid },
        YearRules { daylight: TransitionDate { day: 0, ..valid.daylight }, ..valid },
        YearRules { daylight: TransitionDate { hour: 24, ..valid.daylight }, ..valid },
        YearRules { daylight: TransitionDate { millis: 1000, ..valid.daylight }, ..valid },
        YearRules {
            daylight: TransitionDate { year: 2032, month: 2, day: 30, ..valid.daylight },
            ..valid
        },
    ] {
        assert_rules(invalid, &[(text, None)]);
    }
}

/// No-DST Windows data uses Bias alone; its unused adjustments are ignored.
/// Calendar rollover queries the civil year, and the unchanged reader decides
/// whether the final upper-year UTC instant can be persisted. §FS-rhei-run.3.3
#[test]
fn codex_windows_native_rules_preserve_fixed_offsets_and_year_boundaries() {
    for (bias, text, expected) in [
        (0, "Dec 31st, 2032 11:59 PM", Some("2033-01-01T00:00:00Z")),
        (300, "Dec 31st, 2032 11:59 PM", Some("2033-01-01T05:00:00Z")),
        (-60, "Dec 31st, 2032 11:59 PM", Some("2032-12-31T23:00:00Z")),
        (0, "Feb 29th, 2032 11:59 PM", Some("2032-03-01T00:00:00Z")),
        (0, "Dec 31st, 9999 11:59 PM", None),
        (300, "Dec 31st, 9999 7:00 PM", None),
        (0, "Dec 31st, 9999 11:58 PM", Some("9999-12-31T23:59:00Z")),
        (300, "Dec 31st, 9999 6:58 PM", Some("9999-12-31T23:59:00Z")),
        (-60, "Dec 31st, 9999 11:59 PM", Some("9999-12-31T23:00:00Z")),
    ] {
        assert_rules(
            YearRules {
                bias,
                standard_bias: i32::MAX,
                daylight_bias: i32::MIN,
                ..YearRules::default()
            },
            &[(text, expected)],
        );
    }
    let years = std::cell::RefCell::new(Vec::new());
    let limit = classify("Dec 31st, 2032 11:59 PM", observed(), &|minute| {
        resolve_query(minute, &|year| {
            years.borrow_mut().push(year);
            Some(zurich_rules(false))
        })
    })
    .unwrap();
    assert_eq!(years.into_inner(), [2032, 2033]);
    assert_eq!(limit.next_attempt_at, "2032-12-31T23:00:00Z");
}

/// Ordinary dates, nth-weekday decoding, leap/month/year rollovers and strict
/// expiry remain covered through full sentences and native-rule conversion.
/// §FS-rhei-run.3.3
#[test]
fn codex_windows_native_rules_preserve_calendar_and_strict_future_policy() {
    assert_rules(
        zurich_rules(false),
        &[
            ("Jan 1st, 2032 12:00 AM", Some("2031-12-31T23:01:00Z")),
            ("Feb 29th, 2032 11:59 PM", Some("2032-02-29T23:00:00Z")),
            ("Jul 1st, 2032 12:00 PM", Some("2032-07-01T10:01:00Z")),
            ("Oct 9th, 2026 11:19 PM", Some("2026-10-09T21:20:00Z")),
            ("Mar 27th, 2033 1:59 AM", None),
            ("Oct 30th, 2033 3:00 AM", Some("2033-10-30T02:01:00Z")),
        ],
    );
    let boundary = Utc.with_ymd_and_hms(2032, 10, 31, 2, 1, 0).unwrap();
    let text = "Oct 31st, 2032 3:00 AM";
    let resolve = |minute: &NaiveDateTime| resolve_query(minute, &|_| Some(zurich_rules(false)));
    assert_eq!(
        classify(text, boundary - TimeDelta::seconds(1), &resolve).unwrap().next_attempt_at,
        "2032-10-31T02:01:00Z"
    );
    for at in [boundary, boundary + TimeDelta::seconds(1), boundary + TimeDelta::days(1)] {
        assert!(classify(text, at, &resolve).is_none());
    }
}

/// Equal offsets do not create ambiguity, subminute rule data is not rounded,
/// and a calendar/year rollover into a real gap remains ordinary failure.
/// §FS-rhei-run.3.3
#[test]
fn codex_windows_native_rules_keep_exact_transition_times_and_calendar_edges() {
    let valid = zurich_rules(false);
    assert_rules(
        YearRules { daylight_bias: 0, ..valid },
        &[
            ("Mar 28th, 2032 1:59 AM", Some("2032-03-28T01:00:00Z")),
            ("Oct 31st, 2032 2:59 AM", Some("2032-10-31T02:00:00Z")),
        ],
    );
    assert_rules(
        YearRules {
            daylight: TransitionDate { second: 30, millis: 500, ..valid.daylight },
            ..valid
        },
        &[
            ("Mar 28th, 2032 1:59 AM", Some("2032-03-28T01:00:00Z")),
            ("Mar 28th, 2032 2:00 AM", None),
            ("Mar 28th, 2032 3:00 AM", None),
            ("Mar 28th, 2032 3:01 AM", Some("2032-03-28T01:02:00Z")),
        ],
    );
    for (month, day, text) in [(3, 1, "Feb 29th, 2032 11:59 PM"), (1, 1, "Dec 31st, 2032 11:59 PM")]
    {
        let rules = YearRules {
            daylight: TransitionDate {
                month,
                day,
                hour: 0,
                year: if month == 1 { 2033 } else { 2032 },
                ..TransitionDate::default()
            },
            standard: TransitionDate {
                year: if month == 1 { 2033 } else { 2032 },
                day: 31,
                ..valid.standard
            },
            ..valid
        };
        assert_rules(rules, &[(text, None)]);
    }
}
