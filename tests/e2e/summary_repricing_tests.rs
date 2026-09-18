//! Black-box contract for read-only completed-run repricing.
//! §FS-rhei-summary.1 §FS-rhei-summary.2 §FS-rhei-cost-accounting.5

use std::fs;
use std::path::Path;

use super::summary_repricing_support::*;
use super::*;

fn selected_summary(fixture: &RepriceFixture, extra: &[&str]) -> CliRun {
    let book = fixture.later_book.to_string_lossy().into_owned();
    let mut args = vec!["--run", SELECTED_RUN, "--prices", book.as_str()];
    args.extend_from_slice(extra);
    run_cli("summary", &fixture.plan, &fixture.machine, &args)
}

fn said(result: &CliRun) -> String {
    format!("stdout:\n{}\nstderr:\n{}", result.stdout, result.stderr)
}

fn assert_row(output: &str, name: &str, value: &str) {
    let row = format!("| {name} | {value} |");
    assert!(output.contains(&row), "missing {row:?} from:\n{output}");
}

// §FS-rhei-summary.1 §FS-rhei-summary.2.1 §FS-rhei-summary.2.2
// §FS-rhei-summary.2.3 §FS-rhei-summary.4
#[test]
fn completed_run_uses_the_later_rate_and_only_that_runs_steps_without_writes() {
    let fixture = RepriceFixture::new("summary-reprice-changed-rate");
    fixture.seed_changed_rate_run();

    let ordinary_summary = run_cli("summary", &fixture.plan, &fixture.machine, &[]);
    let ordinary_details = run_cli("summary", &fixture.plan, &fixture.machine, &["--details"]);
    let ordinary_cost = run_cli("cost", &fixture.plan, &fixture.machine, &[]);
    let ordinary_run_cost =
        run_cli("cost", &fixture.plan, &fixture.machine, &["--run", SELECTED_RUN]);
    for result in [&ordinary_summary, &ordinary_details, &ordinary_cost, &ordinary_run_cost] {
        assert_success(result);
    }
    let before = tree_snapshot(&fixture.root.join("runtime"));

    let result = selected_summary(&fixture, &[]);
    assert_success(&result);

    assert!(
        result.stdout.lines().next().unwrap_or_default().contains(SELECTED_RUN),
        "{}",
        said(&result)
    );
    assert!(result.stdout.contains("1 agent invocation"), "{}", said(&result));
    assert!(result.stdout.contains("`plan.1` completed"), "{}", said(&result));
    assert!(!result.stdout.contains("plan.2"), "other run leaked:\n{}", result.stdout);
    assert!(!result.stdout.contains("plan.3"), "current task leaked:\n{}", result.stdout);
    assert_row(&result.stdout, "price book", "fixture-later-2026-09-01");
    assert_row(&result.stdout, "currency", "CHF");
    assert_row(&result.stdout, "pricing", "priced");
    // 19.25 CHF is exactly 19,250,000 integer micro-CHF for this fixture.
    assert_row(&result.stdout, "cost", "19.25 CHF");
    assert!(!result.stdout.contains(&fixture.later_book.display().to_string()));

    assert_eq!(tree_snapshot(&fixture.root.join("runtime")), before, "repricing changed runtime");
    assert_eq!(
        run_cli("summary", &fixture.plan, &fixture.machine, &[]).stdout,
        ordinary_summary.stdout
    );
    assert_eq!(
        run_cli("summary", &fixture.plan, &fixture.machine, &["--details"]).stdout,
        ordinary_details.stdout
    );
    assert_eq!(run_cli("cost", &fixture.plan, &fixture.machine, &[]).stdout, ordinary_cost.stdout);
    assert_eq!(
        run_cli("cost", &fixture.plan, &fixture.machine, &["--run", SELECTED_RUN]).stdout,
        ordinary_run_cost.stdout
    );
}

// §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.5.2
#[test]
fn a_later_rate_prices_previously_unpriced_legacy_dimensions_after_restatement() {
    let fixture = RepriceFixture::new("summary-reprice-unpriced-legacy");
    write_record(
        &fixture.root,
        "legacy-unpriced",
        SELECTED_RUN,
        "plan.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-excludes-cache",
        (500_000, 500_000, 250_000, 750_000),
        UNPRICED,
    );
    write_report(&fixture.root, SELECTED_RUN, "2026-09-01T10:00:00Z", "completed");
    write_stored_book(&fixture.root);
    let before = tree_snapshot(&fixture.root.join("runtime"));

    let result = selected_summary(&fixture, &[]);
    assert_success(&result);

    assert_row(&result.stdout, "pricing", "priced");
    assert_row(&result.stdout, "cost", "19.25 CHF");
    assert_row(&result.stdout, "total tokens", "2.0M");
    assert_row(&result.stdout, "input tokens (incl. cache)", "1.2M");
    assert_row(&result.stdout, "input cache read", "500.0k");
    assert_row(&result.stdout, "input cache write", "250.0k");
    assert_eq!(tree_snapshot(&fixture.root.join("runtime")), before);
}

// §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.6.2
#[test]
fn exact_matching_and_authoritative_currency_distinguish_partial_and_unpriced() {
    let fixture = RepriceFixture::new("summary-reprice-coverage");
    for (id, provider, model, task) in [
        ("matched", "openai", "gpt-5.6-luna", "plan.1"),
        ("provider-miss", "azure", "gpt-5.6-luna", "plan.2"),
        ("model-miss", "openai", "gpt-5.6-luna-preview", "plan.3"),
    ] {
        write_record(
            &fixture.root,
            id,
            SELECTED_RUN,
            task,
            "completed",
            provider,
            model,
            "input-total-includes-cache",
            (1_250_000, 500_000, 250_000, 750_000),
            OLD_PRICE,
        );
    }
    write_report(&fixture.root, SELECTED_RUN, "2026-09-01T10:00:00Z", "completed");
    let euro = write_book(
        &fixture.root,
        "euro-prices.json",
        "fixture-euro-2026-09-01",
        "EUR",
        &[rate("openai", "gpt-5.6-luna", 4_000_000, 500_000, 8_000_000, 20_000_000)],
    );
    let euro_arg = euro.to_string_lossy().into_owned();
    let partial = run_cli(
        "summary",
        &fixture.plan,
        &fixture.machine,
        &["--run", SELECTED_RUN, "--prices", &euro_arg],
    );
    assert_success(&partial);
    assert_row(&partial.stdout, "price book", "fixture-euro-2026-09-01");
    assert_row(&partial.stdout, "currency", "EUR");
    assert_row(&partial.stdout, "pricing", "partial-price");
    assert_row(&partial.stdout, "priced cost (lower bound)", "19.25 EUR");
    assert!(!partial.stdout.contains("| cost |"), "{}", said(&partial));
    assert!(!partial.stdout.contains(&euro_arg), "book path leaked:\n{}", partial.stdout);

    let none = write_book(
        &fixture.root,
        "no-matches.json",
        "fixture-none-2026-09-01",
        "JPY",
        &[rate("another", "model", 1, 1, 1, 1)],
    );
    let none_arg = none.to_string_lossy().into_owned();
    let unpriced = run_cli(
        "summary",
        &fixture.plan,
        &fixture.machine,
        &["--run", SELECTED_RUN, "--prices", &none_arg],
    );
    assert_success(&unpriced);
    assert_row(&unpriced.stdout, "price book", "fixture-none-2026-09-01");
    assert_row(&unpriced.stdout, "currency", "JPY");
    assert_row(&unpriced.stdout, "pricing", "unpriced");
    assert!(!unpriced.stdout.contains("| cost |"));
    assert!(!unpriced.stdout.contains("priced cost"));
    assert!(!unpriced.stdout.contains("0 JPY"), "unpriced must not claim zero");
}

// §FS-rhei-summary.2.3
#[test]
fn an_all_unmeasured_run_uses_only_the_unmeasured_presentation() {
    let fixture = RepriceFixture::new("summary-reprice-unmeasured");
    write_unmeasured_record(&fixture.root, SELECTED_RUN);
    write_report(&fixture.root, SELECTED_RUN, "2026-09-01T10:00:00Z", "completed");

    let result = selected_summary(&fixture, &[]);
    assert_success(&result);
    assert_eq!(
        result.stdout.matches("Token accounting was not measured for this run.").count(),
        1,
        "{}",
        said(&result)
    );
    assert!(!result.stdout.contains("| Accounting | Value |"), "{}", said(&result));
    assert!(!result.stdout.contains("not-applicable"), "{}", said(&result));
}

// §FS-rhei-summary.1 §FS-rhei-summary.3
#[test]
fn paired_flags_and_details_keep_one_selected_reading() {
    let fixture = RepriceFixture::new("summary-reprice-flags-details");
    fixture.seed_changed_rate_run();
    let book = fixture.later_book.to_string_lossy().into_owned();

    for args in [vec!["--run", SELECTED_RUN], vec!["--prices", book.as_str()]] {
        let result = run_cli("summary", &fixture.plan, &fixture.machine, &args);
        assert!(!result.status.success(), "either flag alone must fail:\n{}", said(&result));
        assert!(
            result.stderr.contains("--run") && result.stderr.contains("--prices"),
            "{}",
            said(&result)
        );
        assert!(result.stdout.is_empty(), "usage failure emitted Markdown:\n{}", result.stdout);
    }

    let details = selected_summary(&fixture, &["--details"]);
    assert_success(&details);
    assert!(details.stdout.starts_with("<details>\n<summary>AI workflow:"), "{}", said(&details));
    assert!(details.stdout.contains(SELECTED_RUN), "{}", said(&details));
    assert_row(&details.stdout, "cost", "19.25 CHF");
    assert!(details.stdout.ends_with("</details>\n"), "{}", said(&details));
}

// §FS-rhei-summary.5 §FS-rhei-cost-accounting.5.1
#[test]
fn alternate_book_validation_fails_before_markdown_and_names_the_input_path() {
    let fixture = RepriceFixture::new("summary-reprice-invalid-books");
    fixture.seed_changed_rate_run();
    let missing = fixture.root.join("missing-book.json");
    let malformed = write_fixture_file(&fixture.root, "malformed.json", "{not json");
    let wrong_schema = write_fixture_file(
        &fixture.root,
        "wrong-schema.json",
        r#"{"schema":"other","price_book_id":"wrong","currency":"CHF","entries":[]}"#,
    );
    let duplicate = write_fixture_file(
        &fixture.root,
        "duplicate.json",
        &serde_json::json!({
            "schema": "rhei.accounting.prices.v1",
            "price_book_id": "duplicate",
            "currency": "CHF",
            "entries": [
                rate("openai", "gpt-5.6-luna", 1, 1, 1, 1),
                rate("openai", "gpt-5.6-luna", 2, 2, 2, 2)
            ]
        })
        .to_string(),
    );
    let invalid = write_book(&fixture.root, "empty-currency.json", "invalid", "", &[]);

    for path in [missing, malformed, wrong_schema, duplicate, invalid] {
        let arg = path.to_string_lossy().into_owned();
        let result = run_cli(
            "summary",
            &fixture.plan,
            &fixture.machine,
            &["--run", SELECTED_RUN, "--prices", &arg],
        );
        assert!(!result.status.success(), "invalid book succeeded: {arg}");
        assert!(result.stdout.is_empty(), "invalid book emitted Markdown:\n{}", result.stdout);
        assert!(result.stderr.contains(&arg), "diagnostic omitted {arg:?}:\n{}", result.stderr);
    }
}

// §FS-rhei-summary.5
#[test]
fn exact_run_selection_rejects_absent_and_prefix_ids() {
    let fixture = RepriceFixture::new("summary-reprice-exact-id");
    fixture.seed_changed_rate_run();
    let book = fixture.later_book.to_string_lossy().into_owned();

    for (id, word) in [("missing", "absent"), ("abc", "exact")] {
        let result =
            run_cli("summary", &fixture.plan, &fixture.machine, &["--run", id, "--prices", &book]);
        assert!(!result.status.success(), "unknown or prefix id succeeded: {id}");
        assert!(result.stdout.is_empty());
        assert!(result.stderr.to_lowercase().contains(word), "{}", said(&result));
    }
}

// §FS-rhei-summary.1 §FS-rhei-summary.5 §FS-rhei-run-report.1
#[test]
fn completion_must_be_one_immutable_report_and_active_evidence_wins() {
    let missing = RepriceFixture::new("summary-reprice-no-report");
    write_record(
        &missing.root,
        "record-without-report",
        SELECTED_RUN,
        "plan.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-includes-cache",
        (1, 0, 0, 1),
        OLD_PRICE,
    );
    let missing_result = selected_summary(&missing, &[]);
    assert!(!missing_result.status.success());
    assert!(missing_result.stdout.is_empty());
    assert!(
        missing_result.stderr.contains("completion")
            && missing_result.stderr.contains("not established"),
        "{}",
        said(&missing_result)
    );

    let ambiguous = RepriceFixture::new("summary-reprice-ambiguous-report");
    ambiguous.seed_changed_rate_run();
    write_report(&ambiguous.root, SELECTED_RUN, "2026-09-02T10:00:00Z", "completed");
    let ambiguous_result = selected_summary(&ambiguous, &[]);
    assert!(!ambiguous_result.status.success());
    assert!(ambiguous_result.stdout.is_empty());
    assert!(
        ambiguous_result.stderr.to_lowercase().contains("ambiguous"),
        "{}",
        said(&ambiguous_result)
    );

    let active = RepriceFixture::new("summary-reprice-active");
    active.seed_changed_rate_run();
    write_running_descriptor(&active.root, SELECTED_RUN);
    let active_result = selected_summary(&active, &[]);
    assert!(!active_result.status.success());
    assert!(active_result.stdout.is_empty());
    assert!(active_result.stderr.to_lowercase().contains("active"), "{}", said(&active_result));
}

// §FS-rhei-panta.6.5 §FS-rhei-summary.5
#[test]
fn a_run_found_only_in_another_member_is_outside_scope() {
    let dir = unique_temp_dir("summary-reprice-outside-scope");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    fs::write(project.join("index.panta.md"), "# Panta: Reprice Scope\n").expect("manifest");
    for member in ["alpha", "beta"] {
        let root = project.join(member);
        fs::create_dir_all(root.join("tasks")).expect("member tasks");
        fs::write(root.join("index.rhei.md"), format!("# Rhei: {member}\n")).expect("member index");
        fs::write(root.join("tasks/01-work.md"), "### Task 1: Work\n**State:** completed\n")
            .expect("task");
    }
    write_record(
        &project.join("beta"),
        "beta-record",
        SELECTED_RUN,
        "beta.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-includes-cache",
        (1, 0, 0, 1),
        OLD_PRICE,
    );
    write_report(&project.join("beta"), SELECTED_RUN, "2026-09-01T10:00:00Z", "completed");
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let book = write_book(
        &dir,
        "later.json",
        "later",
        "CHF",
        &[rate("openai", "gpt-5.6-luna", 1, 1, 1, 1)],
    );
    let project_arg = project.to_string_lossy().into_owned();
    let book_arg = book.to_string_lossy().into_owned();
    let result = run_cli(
        "summary",
        Path::new(&project_arg),
        &machine,
        &["--rhei", "alpha", "--run", SELECTED_RUN, "--prices", &book_arg],
    );
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(
        result.stderr.to_lowercase().contains("outside") && result.stderr.contains("alpha"),
        "{}",
        said(&result)
    );
}

// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.6.1 §FS-rhei-summary.2
#[test]
fn project_selection_spans_roots_deduplicates_and_filters_a_shared_root() {
    let dir = unique_temp_dir("summary-reprice-project-roots");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    fs::write(project.join("index.panta.md"), "# Panta: Reprice Project\n").expect("manifest");
    for member in ["alpha", "beta"] {
        let root = project.join(member);
        fs::create_dir_all(root.join("tasks")).expect("member tasks");
        fs::write(root.join("index.rhei.md"), format!("# Rhei: {member}\n")).expect("index");
        fs::write(
            root.join("tasks/01-work.md"),
            "### Task 1: Selected work\n**State:** completed\n",
        )
        .expect("task");
    }
    for member in ["ledger", "spool"] {
        fs::write(
            project.join(format!("{member}.rhei.md")),
            format!(
                "# Rhei: {member}\n\n## Tasks\n\n### Task 1: Shared work\n**State:** completed\n"
            ),
        )
        .expect("single-file rhei");
    }
    write_record(
        &project.join("alpha"),
        "alpha-attempt",
        SELECTED_RUN,
        "alpha.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-includes-cache",
        (1_250_000, 500_000, 250_000, 750_000),
        OLD_PRICE,
    );
    // The project run root holds an exact copy; identity deduplication counts it once.
    write_record(
        &project,
        "alpha-attempt",
        SELECTED_RUN,
        "alpha.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-includes-cache",
        (1_250_000, 500_000, 250_000, 750_000),
        OLD_PRICE,
    );
    write_record(
        &project.join("beta"),
        "beta-attempt",
        SELECTED_RUN,
        "beta.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-includes-cache",
        (500_000, 0, 0, 500_000),
        OLD_PRICE,
    );
    write_record(
        &project,
        "other-run",
        OTHER_RUN,
        "spool.1",
        "completed",
        "openai",
        "gpt-5.6-luna",
        "input-total-includes-cache",
        (9_000_000, 0, 0, 9_000_000),
        OLD_PRICE,
    );
    const SHARED_RUN: &str = "fedcba";
    for member in ["ledger", "spool"] {
        write_record(
            &project,
            &format!("{member}-attempt"),
            SHARED_RUN,
            &format!("{member}.1"),
            "completed",
            "openai",
            "gpt-5.6-luna",
            "input-total-includes-cache",
            (1, 0, 0, 1),
            OLD_PRICE,
        );
    }
    write_report(&project, SELECTED_RUN, "2026-09-01T10:00:00Z", "completed");
    write_report(&project, SHARED_RUN, "2026-09-01T11:00:00Z", "completed");
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let book = write_book(
        &dir,
        "later.json",
        "project-later",
        "CHF",
        &[rate("openai", "gpt-5.6-luna", 4_000_000, 500_000, 8_000_000, 20_000_000)],
    );
    let book_arg = book.to_string_lossy().into_owned();
    let project_result =
        run_cli("summary", &project, &machine, &["--run", SELECTED_RUN, "--prices", &book_arg]);
    assert_success(&project_result);
    assert!(project_result.stdout.contains("2 agent invocations"), "{}", said(&project_result));
    assert!(project_result.stdout.contains("`alpha.1` completed"));
    assert!(project_result.stdout.contains("`beta.1` completed"));
    assert!(!project_result.stdout.contains("spool.1"));
    assert_row(&project_result.stdout, "cost", "31.25 CHF");

    let shared = run_cli(
        "summary",
        &project,
        &machine,
        &["--rhei", "ledger", "--run", SHARED_RUN, "--prices", &book_arg],
    );
    assert_success(&shared);
    assert!(shared.stdout.contains("1 agent invocation"), "{}", said(&shared));
    assert!(shared.stdout.contains("`ledger.1` completed"));
    assert!(!shared.stdout.contains("spool.1"), "shared-root sibling leaked:\n{}", shared.stdout);
}
