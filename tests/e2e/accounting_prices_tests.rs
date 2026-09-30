//! Black-box coverage for caller-selected run price books.

use std::fs;

use super::accounting_prices_support::*;
use super::*;

/// A selected exact match prices measured dimensions with integer arithmetic,
/// persists its full semantics, and produces complete run coverage.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-run.2.1
#[test]
fn sequential_run_prices_luna_with_the_selected_book() {
    let dir = unique_temp_dir("custom-prices-sequential");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    let prices = write_price_book(&dir);
    write_measured_codex_settings(&dir, None);
    let prices_arg = prices.to_string_lossy().into_owned();

    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &prices_arg]);

    assert_success(&result);
    assert_selected_pricing(&dir);
    assert_selected_book_copy(&dir, &price_book_json());
    let summary: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("runtime/accounting/summary.json"))
            .expect("read accounting summary"),
    )
    .expect("parse accounting summary");
    assert_eq!(summary["summary"]["cost_micro"], 9_625_000);
    assert_eq!(summary["summary"]["priced_cost_micro"], 9_625_000);
    assert_eq!(summary["summary"]["pricing_status"], "priced");
    assert_eq!(summary["summary"]["coverage"], "complete");
}

/// A document-only scalar extension is accepted and copied without changing
/// the price produced by the same metadata-free rates.
// §FS-rhei-cost-accounting.5.1
#[test]
fn sequential_run_preserves_document_metadata_without_changing_pricing() {
    let dir = unique_temp_dir("custom-prices-document-metadata");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    let mut expected = price_book_json();
    expected["source"] = serde_json::json!("vendor-rate-card");
    let prices = write_price_book_value(&dir, "document-metadata.json", &expected);
    write_measured_codex_settings(&dir, None);
    let prices_arg = prices.to_string_lossy().into_owned();

    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &prices_arg]);

    assert_success(&result);
    assert_selected_book_copy(&dir, &expected);
    assert_selected_pricing(&dir);
}

/// The same in-memory selection reaches parallel workers, and a project run
/// copies it into the project and every participating member execution root.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-run.2.1
#[test]
fn parallel_project_run_uses_one_selected_book_in_every_root() {
    let dir = unique_temp_dir("custom-prices-parallel");
    let project = dir.join("project");
    for member in ["alpha", "beta"] {
        let root = project.join(member);
        fs::create_dir_all(root.join("tasks")).expect("create member workspace");
        fs::write(root.join("index.rhei.md"), format!("# Rhei: {member}\n"))
            .expect("write member index");
        fs::write(
            root.join("tasks/work.md"),
            "### Task 1: Measure this invocation\n**State:** work\n",
        )
        .expect("write member task");
    }
    fs::write(project.join("index.panta.md"), "# Panta: Priced Project\n")
        .expect("write project manifest");
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    let mut expected = price_book_json();
    expected["entries"][0]["constraints"] = serde_json::json!({
        "service_tier": "standard",
        "context_tokens": 200_000
    });
    let prices = write_price_book_value(&dir, "entry-metadata.json", &expected);
    write_measured_codex_settings(&project, None);
    let prices_arg = prices.to_string_lossy().into_owned();

    let result = run_cli(
        "run",
        &project,
        &machine,
        &["--no-tui", "--no-callbacks", "--parallel", "2", "--prices", &prices_arg],
    );

    assert_success(&result);
    assert_selected_book_copy(&project, &expected);
    for member in ["alpha", "beta"] {
        let root = project.join(member);
        assert_selected_book_copy(&root, &expected);
        assert_selected_pricing(&root);
    }
}

/// Validation happens before process launch, so malformed input cannot leave
/// a partly started run behind and its diagnostic names the caller's path.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-run.2.1
#[test]
fn invalid_selected_book_fails_before_the_agent_starts() {
    let dir = unique_temp_dir("custom-prices-invalid");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    let prices = write_fixture_file(&dir, "invalid-prices.json", "{\"schema\":\"wrong\"}\n");
    let spawned = dir.join("spawned.marker");
    write_measured_codex_settings(&dir, Some(&spawned));
    let prices_arg = prices.to_string_lossy().into_owned();

    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &prices_arg]);

    assert!(!result.status.success(), "invalid price book must fail the run");
    assert!(result.stderr.contains(&prices_arg), "path missing from:\n{}", result.stderr);
    assert!(!spawned.exists(), "the fake agent started before price validation");
    assert!(!dir.join("runtime/accounting/prices.json").exists());
}

/// A custom CHF run followed by the built-in USD selection fails on a later
/// member root before an earlier root changes or its fake agent starts.
// §FS-rhei-cost-accounting.5.1
#[test]
fn successive_run_rejects_mixed_currency_before_any_root_changes() {
    let dir = unique_temp_dir("custom-prices-successive-currency");
    let project = dir.join("project");
    for member in ["alpha", "beta"] {
        let root = project.join(member);
        fs::create_dir_all(root.join("tasks")).expect("create member workspace");
        fs::write(root.join("index.rhei.md"), format!("# Rhei: {member}\n"))
            .expect("write member index");
        fs::write(
            root.join("tasks/work.md"),
            "### Task 1: Measure this invocation\n**State:** work\n",
        )
        .expect("write member task");
    }
    fs::write(project.join("index.panta.md"), "# Panta: Priced Project\n")
        .expect("write project manifest");
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    let mut expected = price_book_json();
    expected["entries"][0]["note"] =
        serde_json::json!("Rate confirmed against the vendor price page");
    let prices = write_price_book_value(&dir, "reported-note.json", &expected);
    write_measured_codex_settings(&project, None);
    let prices_arg = prices.to_string_lossy().into_owned();

    let first = run_cli(
        "run",
        &project,
        &machine,
        &["--no-tui", "--no-callbacks", "--rhei", "beta", "--prices", &prices_arg],
    );
    assert_success(&first);
    assert_selected_book_copy(&project, &expected);
    assert_selected_book_copy(&project.join("beta"), &expected);
    assert_selected_pricing(&project.join("beta"));
    assert!(!project.join("alpha/runtime/accounting").exists());

    let project_book_before = fs::read(project.join("runtime/accounting/prices.json"))
        .expect("read project book before conflict");
    let beta_book_before = fs::read(project.join("beta/runtime/accounting/prices.json"))
        .expect("read beta book before conflict");
    let beta_invocation_before = invocation_json(&project.join("beta"));
    let spawned = dir.join("second-run-spawned.marker");
    write_measured_codex_settings_for_model(
        &project,
        Some(&spawned),
        "anthropic",
        "claude-sonnet-4-6",
    );

    let second = run_cli("run", &project, &machine, &["--no-tui", "--no-callbacks"]);

    assert!(
        !second.status.success(),
        "mixed currencies must reject the second run\nstdout:\n{}\nstderr:\n{}",
        second.stdout,
        second.stderr
    );
    assert!(second.stderr.contains("USD"), "selected currency missing from:\n{}", second.stderr);
    assert!(second.stderr.contains("CHF"), "durable currency missing from:\n{}", second.stderr);
    let beta_accounting = project.join("beta/runtime/accounting");
    let portable_stderr = second.stderr.replace('\\', "/");
    assert!(
        portable_stderr.contains("project/beta/runtime/accounting"),
        "conflicting root missing from:\n{}",
        second.stderr
    );
    assert!(!spawned.exists(), "the second run started an agent before all-root preflight");
    assert!(!project.join("alpha/runtime/accounting").exists());
    assert_eq!(
        fs::read(project.join("runtime/accounting/prices.json")).expect("read project book"),
        project_book_before
    );
    assert_eq!(
        fs::read(beta_accounting.join("prices.json")).expect("read beta book"),
        beta_book_before
    );
    assert_eq!(invocation_json(&project.join("beta")), beta_invocation_before);
}
