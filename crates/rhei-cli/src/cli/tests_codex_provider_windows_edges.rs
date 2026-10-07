fn zurich_rules(absolute: bool) -> YearRules {
    YearRules {
        bias: -60,
        standard_bias: 0,
        daylight_bias: -60,
        daylight: TransitionDate {
            year: if absolute { 2032 } else { 0 },
            month: 3,
            weekday: 0,
            day: if absolute { 28 } else { 5 },
            hour: 2,
            ..TransitionDate::default()
        },
        standard: TransitionDate {
            year: if absolute { 2032 } else { 0 },
            month: 10,
            weekday: 0,
            day: if absolute { 31 } else { 5 },
            hour: 3,
            ..TransitionDate::default()
        },
    }
}

fn assert_rules(rules: YearRules, cases: &[(&str, Option<&str>)]) {
    for &(text, expected) in cases {
        let limit = classify(text, observed(), &|minute| resolve_query(minute, &|_| Some(rules)));
        assert_eq!(limit.as_ref().map(|limit| limit.next_attempt_at.as_str()), expected, "{text}");
        if let Some(limit) = limit {
            assert_eq!(provider_limit_from_value(&provider_limit_value(&limit)), Some(limit));
        }
    }
}

/// Native data runs through the actual Windows conversion and full recognizer.
/// Both edges and their unique neighbors occur in both minute roles. Recurring
/// dates also exercise the fourth/fifth occurrence of the last Sunday.
/// §FS-rhei-run.3.3 §FS-rhei-agents.2.3
#[test]
fn codex_windows_native_rules_pin_half_open_dst_edges() {
    for absolute in [false, true] {
        assert_rules(
            zurich_rules(absolute),
            &[
                ("Mar 28th, 2032 1:58 AM", Some("2032-03-28T00:59:00Z")),
                ("Mar 28th, 2032 1:59 AM", None),
                ("Mar 28th, 2032 2:00 AM", None),
                ("Mar 28th, 2032 2:58 AM", None),
                ("Mar 28th, 2032 2:59 AM", None),
                ("Mar 28th, 2032 3:00 AM", Some("2032-03-28T01:01:00Z")),
                ("Oct 31st, 2032 1:58 AM", Some("2032-10-30T23:59:00Z")),
                ("Oct 31st, 2032 1:59 AM", None),
                ("Oct 31st, 2032 2:00 AM", None),
                ("Oct 31st, 2032 2:58 AM", None),
                ("Oct 31st, 2032 2:59 AM", None),
                ("Oct 31st, 2032 3:00 AM", Some("2032-10-31T02:01:00Z")),
            ],
        );
    }
}

/// Southern ordering and a half-hour adjustment use the same native algorithm;
/// calendar labels do not determine whether the transition is a gap or fold.
/// §FS-rhei-run.3.3
#[test]
fn codex_windows_native_rules_support_southern_half_hour_transitions() {
    let northern = zurich_rules(false);
    let rules = YearRules {
        bias: -600,
        daylight_bias: -30,
        standard: TransitionDate { hour: 3, ..northern.daylight },
        daylight: TransitionDate { hour: 2, ..northern.standard },
        ..northern
    };
    assert_rules(
        rules,
        &[
            ("Mar 28th, 2032 2:28 AM", Some("2032-03-27T15:59:00Z")),
            ("Mar 28th, 2032 2:29 AM", None),
            ("Mar 28th, 2032 2:30 AM", None),
            ("Mar 28th, 2032 2:58 AM", None),
            ("Mar 28th, 2032 2:59 AM", None),
            ("Mar 28th, 2032 3:00 AM", Some("2032-03-27T17:01:00Z")),
            ("Oct 31st, 2032 1:58 AM", Some("2032-10-30T15:59:00Z")),
            ("Oct 31st, 2032 1:59 AM", None),
            ("Oct 31st, 2032 2:00 AM", None),
            ("Oct 31st, 2032 2:28 AM", None),
            ("Oct 31st, 2032 2:29 AM", None),
            ("Oct 31st, 2032 2:30 AM", Some("2032-10-30T16:01:00Z")),
        ],
    );
}

/// A positive daylight bias reverses both gap/fold directions. No transition
/// minute is special-cased or rejected merely for being on a change date.
/// §FS-rhei-run.3.3
#[test]
fn codex_windows_native_rules_support_negative_daylight_adjustment() {
    let northern = zurich_rules(false);
    let rules = YearRules {
        daylight_bias: 60,
        standard: TransitionDate { hour: 2, ..northern.standard },
        ..northern
    };
    assert_rules(
        rules,
        &[
            ("Mar 28th, 2032 12:58 AM", Some("2032-03-27T23:59:00Z")),
            ("Mar 28th, 2032 12:59 AM", None),
            ("Mar 28th, 2032 1:00 AM", None),
            ("Mar 28th, 2032 1:58 AM", None),
            ("Mar 28th, 2032 1:59 AM", None),
            ("Mar 28th, 2032 2:00 AM", Some("2032-03-28T02:01:00Z")),
            ("Oct 31st, 2032 1:58 AM", Some("2032-10-31T01:59:00Z")),
            ("Oct 31st, 2032 1:59 AM", None),
            ("Oct 31st, 2032 2:00 AM", None),
            ("Oct 31st, 2032 2:58 AM", None),
            ("Oct 31st, 2032 2:59 AM", None),
            ("Oct 31st, 2032 3:00 AM", Some("2032-10-31T02:01:00Z")),
        ],
    );
}
