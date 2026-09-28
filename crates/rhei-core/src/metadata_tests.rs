// The register, the exclusion filter, and the YAML-to-JSON conversion.
//
// The register's `cleared_by_reset` column is what `rhei reset` reads, so it is
// pinned here against the five keys it declares rather than left to the one
// caller that consumes it.

// §FS-rhei-transitions.2.5 §FS-rhei-render.3.1.1

use super::*;

fn document(yaml: &str) -> Metadata {
    serde_yaml::from_str(yaml).expect("frontmatter should parse")
}

fn converted(yaml: &str) -> Value {
    frontmatter_to_json(&document(yaml)).expect("every value should have a JSON image")
}

fn findings(yaml: &str) -> Vec<String> {
    frontmatter_to_json(&document(yaml))
        .expect_err("the document should hold a value JSON cannot represent")
        .iter()
        .map(UnrepresentableValue::describe)
        .collect()
}

/// The register is the whole set, and every key rhei writes resolves through a
/// constant it holds. §FS-rhei-transitions.2.5
#[test]
fn the_register_names_every_key_rhei_writes() {
    let names: Vec<&str> = RHEI_WRITTEN_KEYS.iter().map(|key| key.name).collect();
    assert_eq!(
        names,
        vec!["stateVisits", "pollNextAttemptAt", "providerLimits", "supervision", "budgetTicketId",]
    );
    for name in &names {
        assert!(is_rhei_written_key(name), "{name} should be registered");
    }
    assert!(!is_rhei_written_key("context"), "an author's field is not rhei's");
    assert!(!is_rhei_written_key("statevisits"), "the register is spelled exactly");
}

/// The `Cleared by rhei reset` column as `rhei reset` was measured to behave:
/// the counters, the parked provider wait and the supervision block go; the
/// poll deadline is one state's own scheduling and the budget identity would
/// hand the ticket a fresh travel history. §FS-rhei-reset.2
#[test]
fn the_reset_column_is_what_reset_actually_deletes() {
    let cleared: Vec<&str> = keys_cleared_by_reset().collect();
    assert_eq!(cleared, vec!["stateVisits", "providerLimits", "supervision"]);
    for key in RHEI_WRITTEN_KEYS {
        let expected = matches!(key.name, "stateVisits" | "providerLimits" | "supervision");
        assert_eq!(key.cleared_by_reset, expected, "{} clears?", key.name);
    }
}

/// The filter publishes the author's half of a mixed map and nothing else,
/// including where the author wrote a registered name. §FS-rhei-list.4.2
#[test]
fn the_filter_publishes_the_authors_half_alone() {
    let frontmatter = converted(
        "metadata:\n  tasks:\n    plan.1:\n      context: /checkouts/widget\n      \
         stateVisits:\n        review: 9\n      budgetTicketId: not-yours-to-write\n",
    );

    assert_eq!(
        author_task_metadata(&frontmatter, "plan.1"),
        Some(serde_json::json!({ "context": "/checkouts/widget" }))
    );
}

/// Absence is the declaration, so a map holding only registered keys publishes
/// nothing rather than an empty object — and so does a task with no entry, or a
/// document with no `metadata.tasks` at all. §FS-rhei-list.4.2
#[test]
fn the_filter_publishes_nothing_where_only_rheis_own_keys_are_stored() {
    let frontmatter = converted(
        "owner: keep-me\nmetadata:\n  tasks:\n    plan.1:\n      stateVisits:\n        review: 2\n      \
         supervision:\n        phase: released\n",
    );

    assert_eq!(author_task_metadata(&frontmatter, "plan.1"), None);
    assert_eq!(author_task_metadata(&frontmatter, "plan.9"), None, "no entry of its own");
    assert_eq!(author_task_metadata(&serde_json::json!({}), "plan.1"), None, "no tasks map");
}

/// Keys pass through exactly as authored and nested values survive whole: the
/// object is the stored map, not a normalization of it. §FS-rhei-list.4.2
#[test]
fn the_conversion_keeps_the_authors_own_spelling_and_shape() {
    let frontmatter = converted(
        "metadata:\n  tasks:\n    plan.1:\n      snake_case_key: kept\n      \
         routes:\n        primary:\n          - alpha\n          - beta\n",
    );

    assert_eq!(
        author_task_metadata(&frontmatter, "plan.1"),
        Some(serde_json::json!({
            "snake_case_key": "kept",
            "routes": { "primary": ["alpha", "beta"] },
        }))
    );
}

/// A mapping key becomes its YAML text as a JSON string, whatever scalar shape
/// the author used for it. §FS-rhei-render.3.1.1
#[test]
fn a_scalar_key_becomes_its_yaml_text() {
    assert_eq!(
        converted("12: a\ntrue: b\n1.5: c\n~: d\n"),
        serde_json::json!({ "12": "a", "true": "b", "1.5": "c", "null": "d" })
    );
}

/// A tagged value becomes a one-key object naming the tag, which is what this
/// surface has always emitted for one. §FS-rhei-render.3.1.1
#[test]
fn a_tagged_value_becomes_a_one_key_object_naming_the_tag() {
    assert_eq!(
        converted("greeting: !custom hello\n"),
        serde_json::json!({ "greeting": { "!custom": "hello" } })
    );
}

/// Everything else converts as its JSON counterpart. §FS-rhei-render.3.1.1
#[test]
fn every_other_shape_converts_to_its_json_counterpart() {
    assert_eq!(
        converted("flag: true\ncount: -3\nbig: 18446744073709551615\nratio: 1.5\nnone: ~\n"),
        serde_json::json!({
            "flag": true,
            "count": -3,
            "big": 18446744073709551615u64,
            "ratio": 1.5,
            "none": null,
        })
    );
}

/// A key JSON cannot name is named by the shape used as one, because it has no
/// name to give, and located by the task whose metadata holds it.
/// §FS-rhei-render.3.1.1
#[test]
fn a_non_scalar_key_is_reported_by_its_shape() {
    assert_eq!(
        findings("metadata:\n  tasks:\n    plan.1:\n      ? [alpha, beta]\n      : keyed\n"),
        vec!["plan.1: a sequence used as a mapping key, which JSON cannot name"]
    );
    assert_eq!(
        findings("metadata:\n  tasks:\n    plan.1:\n      ? {a: b}\n      : keyed\n"),
        vec!["plan.1: a mapping used as a mapping key, which JSON cannot name"]
    );
}

/// A float JSON has no number for is named by the spelling the author wrote,
/// wherever in the task's map it sits. §FS-rhei-render.3.1.1
#[test]
fn a_non_finite_float_is_reported_by_its_yaml_spelling() {
    assert_eq!(
        findings(
            "metadata:\n  tasks:\n    plan.2:\n      ratio: .inf\n      floor: -.inf\n      \
             limits:\n        drift: .nan\n"
        ),
        vec![
            "plan.2: \"ratio\" holds .inf, which JSON has no number for",
            "plan.2: \"floor\" holds -.inf, which JSON has no number for",
            "plan.2: \"limits\".\"drift\" holds .nan, which JSON has no number for",
        ]
    );
}

/// Every unrepresentable value in the document is reported in one run, in
/// document order, so an author fixing frontmatter by hand sees the whole list.
// §FS-rhei-errors.1.1 §FS-rhei-render.3.1.1
#[test]
fn one_run_reports_every_value_json_cannot_hold() {
    assert_eq!(
        findings(
            "metadata:\n  tasks:\n    plan.1:\n      ? [alpha, beta]\n      : keyed\n    \
             plan.2:\n      ratio: .inf\n"
        ),
        vec![
            "plan.1: a sequence used as a mapping key, which JSON cannot name",
            "plan.2: \"ratio\" holds .inf, which JSON has no number for",
        ]
    );
}

/// A merged project keys `metadata.tasks` by the project-qualified id, so the
/// report names the id the listing prints rather than the rhei-local one the
/// file authored. §FS-rhei-render.3.1.1
#[test]
fn a_qualified_id_is_the_site_the_report_names() {
    assert_eq!(
        findings("metadata:\n  tasks:\n    billing.1.2:\n      ratio: .nan\n"),
        vec!["billing.1.2: \"ratio\" holds .nan, which JSON has no number for"]
    );
}

/// A value outside a task's metadata has no owning ticket to name, so the report
/// falls back to the document and the whole path. §FS-rhei-render.3.1.1
#[test]
fn a_value_outside_a_tasks_metadata_is_reported_against_the_document() {
    assert_eq!(
        findings("owner: .inf\n"),
        vec!["frontmatter: \"owner\" holds .inf, which JSON has no number for"]
    );
    assert_eq!(
        findings("metadata:\n  tasks:\n    ? [alpha]\n    : {}\n"),
        vec![
            "frontmatter: \"metadata\".\"tasks\".a sequence used as a mapping key, which JSON \
             cannot name"
        ]
    );
}
