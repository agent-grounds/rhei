// The fourth settings key and the chain it resolves on.
//
// `spend_per_day` is the first of the four bounds whose value is not an
// integer, so it is the first whose reader has anything to decide: a
// fractional value is the *point* of the key, and the refusals are the
// narrower set of zero, a negative, a word, and more precision than the
// micro-unit can hold. It is also the only one of the four a plan may not
// declare, which is why nothing here resolves it from a profile.

// §FS-rhei-budgets.2.1 §FS-rhei-budgets.2

/// Machine-global settings with one `defaults` body, and the merged view of
/// them from a project that writes nothing of its own.
fn merged_with(defaults: &str) -> Result<RheiSettings, miette::Report> {
    let dir = tempfile::tempdir().expect("tmpdir");
    let plan_root = dir.path().join("plan");
    std::fs::create_dir_all(&plan_root).expect("plan root");
    let home = TempHome::new();
    let global_dir = home_dir().expect("home").join(".config/rhei");
    std::fs::create_dir_all(&global_dir).expect("global dir");
    std::fs::write(global_dir.join("settings.json"), format!(r#"{{ "defaults": {defaults} }}"#))
        .expect("write global");
    let merged = load_merged_settings(&plan_root);
    drop(home);
    merged
}

/// A bare number, to at most six decimal places, and `400` and `400.00` are
/// the same number. §FS-rhei-budgets.2.1
#[test]
fn spend_per_day_takes_a_bare_number_however_it_is_written() {
    for written in ["400", "400.00", "400.000000"] {
        let settings = merged_with(&format!(r#"{{ "spend_per_day": {written} }}"#))
            .unwrap_or_else(|err| panic!("`{written}` is a lawful amount; got: {err}"));
        assert_eq!(
            resolve_count_bounds(&settings, None).spend.effective,
            rhei_core::budget::built_in::SPEND_PER_DAY
        );
    }
    let half = merged_with(r#"{ "spend_per_day": 9.5 }"#).expect("a fraction is the point");
    assert_eq!(resolve_count_bounds(&half, None).spend.effective, 9_500_000);
}

/// Four ways of writing no bound at all, each a settings error that names the
/// key rather than the value: a settings file has several numbers in it, and
/// the one that is wrong is the only thing the reader is missing.
/// §FS-rhei-budgets.2.1 §REQ-bounded-neural-work.2
#[test]
fn a_spend_bound_of_nothing_is_a_settings_error_naming_the_key() {
    for refused in ["0", "-5", r#""unlimited""#, "0.1234567"] {
        let error = merged_with(&format!(r#"{{ "spend_per_day": {refused} }}"#))
            .err()
            .unwrap_or_else(|| panic!("`{refused}` is not a bound"));
        let message = format!("{error:?}");
        assert!(
            message.contains("spend_per_day"),
            "the refusal names the key that is wrong; got: {message}"
        );
    }
}

/// A machine that configures nothing is bounded by the built-in, and a plan
/// that declares nothing is bounded by the machine. Nothing declares this key
/// on a profile at all, which is the one way it differs from
/// `transition_limit`. §FS-rhei-budgets.2.1
#[test]
fn spend_per_day_resolves_on_the_machine_and_project_tiers_only() {
    let bare = merged_with("{}").expect("no settings is a lawful state");
    let bounds = resolve_count_bounds(&bare, Some(4));

    assert_eq!(bounds.spend.effective, rhei_core::budget::built_in::SPEND_PER_DAY);
    assert_eq!(bounds.spend.source, rhei_core::budget::BoundSource::BuiltIn);
    assert_eq!(
        bounds.travel.effective, 4,
        "the profile's number reaches travel, which is the tier spend does not have"
    );
    assert_eq!(
        bounds.report_lines(),
        vec![
            "transition_limit: 4 (plan)".to_string(),
            "invocations_per_day: 200 (built_in)".to_string(),
            "invocation_lifetime_max: 6000 (built_in)".to_string(),
            "spend_per_day: 400.00 (built_in)".to_string(),
        ],
        "and every surface reports all four, in dimension order"
    );
}
