/// Private TZif v2 data carries Zurich's 2026/2032 transitions and recurring
/// rules, exercising the production file parser without host data. §FS-rhei-run.3.3
fn tzif(offset: i32, zurich: bool) -> Vec<u8> {
    let mut data = Vec::new();
    for wide in [false, true] {
        data.extend_from_slice(b"TZif2");
        data.extend_from_slice(&[0; 15]);
        let (transitions, types, chars) = if zurich { (4u32, 2u32, 9u32) } else { (0, 1, 4) };
        for count in [0u32, 0, 0, transitions, types, chars] {
            data.extend_from_slice(&count.to_be_bytes());
        }
        if zurich {
            for (year, month, day) in [(2026, 3, 29), (2026, 10, 25), (2032, 3, 28), (2032, 10, 31)]
            {
                let instant = Utc.with_ymd_and_hms(year, month, day, 1, 0, 0).unwrap().timestamp();
                if wide {
                    data.extend_from_slice(&instant.to_be_bytes());
                } else {
                    data.extend_from_slice(&i32::try_from(instant).unwrap().to_be_bytes());
                }
            }
            data.extend_from_slice(&[1, 0, 1, 0]);
            data.extend_from_slice(&3600i32.to_be_bytes());
            data.extend_from_slice(&[0, 0]);
            data.extend_from_slice(&7200i32.to_be_bytes());
            data.extend_from_slice(&[1, 4]);
            data.extend_from_slice(b"CET\0CEST\0");
        } else {
            data.extend_from_slice(&offset.to_be_bytes());
            data.extend_from_slice(&[0, 0]);
            data.extend_from_slice(b"TST\0");
        }
    }
    data.extend_from_slice(if zurich { b"\nCET-1CEST,M3.5.0,M10.5.0/3\n" } else { b"\n\n" });
    data
}

/// Named TZ sources use the real system database on the Unix CI hosts; the
/// separate file fixtures require no name or database. §FS-rhei-run.3.3
#[test]
fn codex_native_named_sources_and_native_name_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    for source in ["Europe/Zurich", ":Europe/Zurich"] {
        assert_deadline(
            select_unix_zone(Some(source), &missing, &|| None),
            Some("2032-01-01T22:20:00Z"),
        );
    }
    assert_deadline(
        select_unix_zone(None, &missing, &|| Some("Europe/Zurich".into())),
        Some("2032-01-01T22:20:00Z"),
    );
}

fn assert_deadline(zone: Option<LocalZone>, expected: Option<&str>) {
    let limit =
        classify("Jan 1st, 2032 11:19 PM", observed(), &|minute| zone.as_ref()?.resolve(minute));
    assert_eq!(limit.as_ref().map(|limit| limit.next_attempt_at.as_str()), expected);
}

/// No source, read errors, corrupt payloads and failed name lookup remain
/// ordinary through production selection and classification. §FS-rhei-run.3.3
#[test]
fn codex_native_unavailable_sources_never_substitute_utc() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let directory = dir.path().join("unreadable-as-data");
    std::fs::create_dir(&directory).unwrap();
    let corrupt = dir.path().join("corrupt");
    std::fs::write(&corrupt, b"invalid TZif contents").unwrap();
    let invalid = format!(":{}", missing.display());
    for path in [&missing, &directory, &corrupt] {
        for explicit in [None, Some(invalid.as_str())] {
            assert_deadline(select_unix_zone(explicit, path, &|| None), None);
            assert_deadline(select_unix_zone(explicit, path, &|| Some(invalid.clone())), None);
        }
    }
}

/// Copy/link payloads, explicit files/POSIX/UTC, and valid fallback after
/// an invalid explicit source all use actual fallible loaders. §FS-rhei-run.3.3
#[test]
fn codex_native_selection_preserves_valid_sources_and_fallbacks() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let copied = dir.path().join("copied-without-iana-name");
    std::fs::write(&copied, tzif(3600, true)).unwrap();
    let linked = dir.path().join("localtime-link");
    symlink(&copied, &linked).unwrap();
    let utc_file = dir.path().join("utc-file");
    std::fs::write(&utc_file, tzif(0, false)).unwrap();
    let invalid = format!(":{}", missing.display());
    let explicit_file = copied.to_str().unwrap();
    let colon_file = format!(":{explicit_file}");
    for explicit in [explicit_file, colon_file.as_str()] {
        assert_deadline(
            select_unix_zone(Some(explicit), &missing, &|| None),
            Some("2032-01-01T22:20:00Z"),
        );
    }
    for path in [&copied, &linked] {
        for explicit in [None, Some(invalid.as_str()), Some("localtime")] {
            assert_deadline(
                select_unix_zone(explicit, path, &|| None),
                Some("2032-01-01T22:20:00Z"),
            );
        }
    }
    for explicit in ["", "UTC0"] {
        assert_deadline(
            select_unix_zone(Some(explicit), &copied, &|| panic!("explicit UTC wins")),
            Some("2032-01-01T23:20:00Z"),
        );
    }
    assert_deadline(select_unix_zone(None, &utc_file, &|| None), Some("2032-01-01T23:20:00Z"));
    assert_deadline(
        select_unix_zone(Some("XST-5"), &copied, &|| None),
        Some("2032-01-01T18:20:00Z"),
    );
    for explicit in [None, Some(invalid.as_str())] {
        assert_deadline(
            select_unix_zone(explicit, &missing, &|| Some(explicit_file.to_string())),
            Some("2032-01-01T22:20:00Z"),
        );
        assert_deadline(
            select_unix_zone(explicit, &missing, &|| Some("XST-5".into())),
            Some("2032-01-01T18:20:00Z"),
        );
    }
}

/// Selection owns its parsed snapshot: changing a private source after loading
/// cannot change the second minute or trigger another discovery. §FS-rhei-run.3.3
#[test]
fn codex_native_zone_is_retained_for_both_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let copied = dir.path().join("localtime");
    std::fs::write(&copied, tzif(3600, true)).unwrap();
    let zone = select_unix_zone(None, &copied, &|| panic!("valid payload wins")).unwrap();
    let calls = std::cell::Cell::new(0);
    let limit = classify("Jan 1st, 2032 11:19 PM", observed(), &|minute| {
        calls.set(calls.get() + 1);
        let result = zone.resolve(minute);
        std::fs::write(&copied, b"corrupted after selection").unwrap();
        result
    })
    .unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(limit.next_attempt_at, "2032-01-01T22:20:00Z");
}
