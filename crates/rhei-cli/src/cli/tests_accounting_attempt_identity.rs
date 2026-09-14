// Legacy attempt identity is a separate reader concern from root enumeration.
// These tests share the root fixtures included immediately before this file.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-panta.6.5

fn write_attempt_record(root: &Path, file: &str, record: &AccountingInvocationRecord) {
    let dir = root.join(ACCOUNTING_DIR).join("invocations");
    fs::create_dir_all(&dir).expect("invocations directory");
    fs::write(
        dir.join(file),
        serde_json::to_vec_pretty(record).expect("serialize record"),
    )
    .expect("write record");
}

fn legacy_attempt(
    run_id: Option<&str>,
    started_at: &str,
    ended_at: &str,
    total: u64,
) -> AccountingInvocationRecord {
    let mut record = roots_record("plan.1::work::codex::visit-1", "plan.1", started_at);
    record.run_id = run_id.map(str::to_string);
    record.ended_at = ended_at.to_string();
    record.target_slug = Some("codex".to_string());
    record.token_convention = Some(TOKEN_CONVENTION_INCLUDES_CACHE.to_string());
    record.tokens.total = AccountingTokenDimension::measured(total);
    record.tokens.input.total = AccountingTokenDimension::measured(total - 10);
    record.tokens.input.cached_read = AccountingTokenDimension::measured(0);
    record.tokens.input.cache_write = AccountingTokenDimension::measured(0);
    record.tokens.output.total = AccountingTokenDimension::measured(10);
    record.pricing.amount_micro = Some(total * 10);
    record.pricing.priced_amount_micro = Some(total * 10);
    record
}

fn inspect_attempt_pair(
    first: AccountingInvocationRecord,
    second: AccountingInvocationRecord,
) -> CostInspection {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("root");
    write_attempt_record(&root, "first.json", &first);
    write_attempt_record(&root, "second.json", &second);
    read_cost_inspection_over(&[roots_entry(&root, &["plan"], false)], &None)
}

/// A run boundary distinguishes old records whose ids name only the visit.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-panta.6.5
#[test]
fn attempt_identity_legacy_cross_run_attempts_are_both_counted() {
    let inspection = inspect_attempt_pair(
        legacy_attempt(
            Some("run-one"),
            "2026-09-01T10:00:00Z",
            "2026-09-01T10:00:01Z",
            100,
        ),
        legacy_attempt(
            Some("run-two"),
            "2026-09-01T10:01:00Z",
            "2026-09-01T10:01:01Z",
            200,
        ),
    );
    assert_eq!(inspection.invocations.len(), 2, "cross-run attempts are separate");
    let summary = inspection.summary.expect("both measured attempts produce a summary");
    assert_eq!(summary.invocation_count, 2);
    assert_eq!(summary.total.value, Some(300));
    assert_eq!(summary.cost_micro, Some(3_000));
    assert!(inspection.errors.is_empty(), "separate attempts are not conflicts");
}

/// Disjoint valid intervals distinguish retries within one run.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-panta.6.5
#[test]
fn attempt_identity_legacy_same_run_disjoint_attempts_are_both_counted() {
    let inspection = inspect_attempt_pair(
        legacy_attempt(
            Some("same-run"),
            "2026-09-01T10:00:00Z",
            "2026-09-01T10:00:01Z",
            100,
        ),
        legacy_attempt(
            Some("same-run"),
            "2026-09-01T10:01:00Z",
            "2026-09-01T10:01:01Z",
            200,
        ),
    );
    assert_eq!(inspection.invocations.len(), 2, "disjoint intervals are separate attempts");
    assert_eq!(inspection.summary.as_ref().and_then(|summary| summary.total.value), Some(300));
    assert!(inspection.errors.is_empty(), "separate attempts are not conflicts");
}

/// Missing run attribution preserves both disjoint processes as history.
// §FS-rhei-cost-accounting.3.5 §FS-rhei-cost-accounting.3.7
#[test]
fn attempt_identity_unattributed_disjoint_attempts_are_both_counted() {
    let inspection = inspect_attempt_pair(
        legacy_attempt(None, "2026-09-01T10:00:00Z", "2026-09-01T10:00:01Z", 100),
        legacy_attempt(None, "2026-09-01T10:01:00Z", "2026-09-01T10:01:01Z", 200),
    );
    assert_eq!(inspection.invocations.len(), 2);
    assert!(inspection.invocations.iter().all(|held| held.record.run_id.is_none()));
    assert_eq!(inspection.summary.as_ref().and_then(|summary| summary.total.value), Some(300));
    assert!(inspection.errors.is_empty(), "separate attempts are not conflicts");
}

/// Exact copies within one root and across roots are still one attempt.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-panta.6.5
#[test]
fn attempt_identity_exact_copies_within_and_across_roots_count_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let record = legacy_attempt(
        Some("one-run"),
        "2026-09-01T10:00:00Z",
        "2026-09-01T10:00:01Z",
        100,
    );
    write_attempt_record(&first, "original.json", &record);
    write_attempt_record(&first, "copy.json", &record);
    write_attempt_record(&second, "archive.json", &record);

    let roots = [roots_entry(&first, &["a"], false), roots_entry(&second, &["b"], false)];
    let inspection = read_cost_inspection_over(&roots, &None);
    assert_eq!(inspection.invocations.len(), 1);
    assert_eq!(inspection.summary.as_ref().map(|summary| summary.invocation_count), Some(1));
    assert_eq!(inspection.summary.as_ref().and_then(|summary| summary.total.value), Some(100));
    assert!(inspection.errors.is_empty(), "exact copies are silently deduplicated");
}

/// Malformed timing and disagreement in base facts remain conflicts.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.11
#[test]
fn attempt_identity_ambiguous_timing_and_inconsistent_facts_remain_conflicts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let third = dir.path().join("third");
    let valid = legacy_attempt(
        Some("same-run"),
        "2026-09-01T10:00:00Z",
        "2026-09-01T10:00:01Z",
        100,
    );
    let ambiguous = legacy_attempt(
        Some("same-run"),
        "2026-09-01T10:01:00Z",
        "2026-09-01T09:59:00Z",
        200,
    );
    let mut inconsistent = legacy_attempt(
        Some("another-run"),
        "2026-09-01T10:02:00Z",
        "2026-09-01T10:02:01Z",
        300,
    );
    inconsistent.task_id = "other.1".to_string();
    write_attempt_record(&first, "valid.json", &valid);
    write_attempt_record(&second, "ambiguous.json", &ambiguous);
    write_attempt_record(&third, "inconsistent.json", &inconsistent);

    let roots = [
        roots_entry(&first, &["a"], false),
        roots_entry(&second, &["b"], false),
        roots_entry(&third, &["c"], false),
    ];
    let inspection = read_cost_inspection_over(&roots, &None);
    assert_eq!(inspection.invocations.len(), 1, "the first valid record is retained");
    assert_eq!(inspection.invocations[0].record.tokens.total.value, Some(100));
    assert_eq!(inspection.errors.len(), 2, "both conflicts are reported");
    assert!(inspection.errors.iter().any(|error| {
        error.contains("ambiguous.json") && error.contains("valid.json")
    }));
    assert!(inspection.errors.iter().any(|error| {
        error.contains("inconsistent.json") && error.contains("valid.json")
    }));
}
