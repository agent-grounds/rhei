//! Black-box scenarios for composing profile-authored prices across scheduler
//! modes, roots, conflicts, fallbacks, and historical snapshots.
//! §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.5.2

use std::fs;
use std::path::PathBuf;

use super::accounting_profile_prices_tests::{
    expected_generated_book, invocation_records, one_model_machine, profile, profile_prices,
    read_json, write_settings, ONE_TASK_PLAN,
};
use super::*;

fn two_profile_fixture(
    prefix: &str,
    model_a: &str,
    rates_a: serde_json::Value,
    model_b: &str,
    rates_b: serde_json::Value,
) -> (TestDir, PathBuf, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Two Profiles\n\n## Tasks\n\n### Task 1: A\n**State:** work-a\n\n### Task 2: B\n**State:** work-b\n",
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: two-profiles
version: 1
models: [a, b]
states:
  work-a:
    description: A
    agent: codex
    model: a
  work-b:
    description: B
    agent: codex
    model: b
  completed:
    final: true
    description: Done
transitions:
  - from: work-a
    to: completed
  - from: work-b
    to: completed
"#,
    );
    let spawned = dir.join("spawned.log");
    write_settings(
        &dir,
        serde_json::json!({
            "a": profile("openai", model_a, Some(rates_a)),
            "b": profile("openai", model_b, Some(rates_b))
        }),
        None,
        &format!(
            "with pathlib.Path({}).open('a') as marker:\n    marker.write('spawned\\n')",
            serde_json::to_string(spawned.to_string_lossy().as_ref()).expect("marker")
        ),
    );
    (dir, plan, machine, spawned)
}

/// Distinct profile pairs compose for both scheduler modes.
// §FS-rhei-cost-accounting.5.1
#[test]
fn distinct_profile_pairs_compose_in_sequential_and_parallel_runs() {
    for parallel in ["1", "2"] {
        let rates_a = profile_prices("CHF", 4_000_000, "tier a");
        let rates_b = profile_prices("CHF", 6_000_000, "tier b");
        let (dir, plan, machine, _) = two_profile_fixture(
            &format!("profile-prices-distinct-{parallel}"),
            "model-a",
            rates_a.clone(),
            "model-b",
            rates_b.clone(),
        );
        let result = run_cli(
            "run",
            &plan,
            &machine,
            &["--no-tui", "--no-callbacks", "--parallel", parallel],
        );
        assert_success(&result);
        let expected = expected_generated_book(
            "CHF",
            &[("openai", "model-a", &["a"], rates_a), ("openai", "model-b", &["b"], rates_b)],
        );
        assert_eq!(read_json(&dir.join("runtime/accounting/prices.json")), expected);
        let id = expected["price_book_id"].as_str().expect("generated id");
        let records = invocation_records(&dir);
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|(_, record)| record["pricing"]["status"] == "priced"));
        assert!(records.iter().all(|(_, record)| record["pricing"]["price_book_id"] == id));
    }
}

/// Once any profile supplies rates, distinct invocations still use their
/// built-in exact match or remain explicitly unpriced in the generated book.
// §FS-rhei-cost-accounting.5.1
#[test]
fn generated_book_composes_builtin_and_unpriced_fallbacks() {
    for (label, fallback_provider, fallback_model, expected_status, expected_entries) in [
        ("builtin", "anthropic", "claude-sonnet-4-6", "priced", 2),
        ("unpriced", "openai", "unknown-model", "unpriced", 1),
    ] {
        let rates = profile_prices("USD", 4_000_000, "profile source");
        let (dir, plan, machine, _) = two_profile_fixture(
            &format!("profile-prices-compose-{label}"),
            "profile-model",
            rates,
            fallback_model,
            profile_prices("USD", 4_000_000, "removed fixture rate"),
        );
        let settings_path = dir.join(".agent-grounds/rhei/settings.json");
        let mut settings = read_json(&settings_path);
        settings["models"]["b"].as_object_mut().expect("fallback profile").remove("prices");
        settings["models"]["b"]["provider"] = serde_json::json!(fallback_provider);
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings).expect("fallback settings"),
        )
        .expect("write fallback settings");

        let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
        assert_success(&result);
        let book = read_json(&dir.join("runtime/accounting/prices.json"));
        assert!(
            book["price_book_id"].as_str().is_some_and(|id| id.starts_with("profiles-sha256-")),
            "{label}: {book}"
        );
        assert_eq!(book["entries"].as_array().expect("entries").len(), expected_entries, "{label}");
        assert!(book["entries"].as_array().expect("entries").iter().any(|entry| {
            entry["model"] == "profile-model" && entry["source"]["kind"] == "profile"
        }));
        if label == "builtin" {
            assert!(book["entries"].as_array().expect("entries").iter().any(|entry| {
                entry["model"] == "claude-sonnet-4-6"
                    && entry["source"]["kind"] == "builtin"
                    && entry["source"]["price_book_id"] == "builtin-2026-05-20"
            }));
        }
        let records = invocation_records(&dir);
        let fallback = records
            .iter()
            .find(|(_, record)| record["model"] == fallback_model)
            .expect("fallback invocation");
        assert_eq!(fallback.1["pricing"]["status"], expected_status, "{label}");
    }
}

/// A project-level generated book is current and archived in the run root and
/// every participating member root before parallel agents run.
// §FS-rhei-cost-accounting.5.1
#[test]
fn generated_profile_book_is_persisted_in_every_participating_root() {
    let dir = unique_temp_dir("profile-prices-project-roots");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("project root");
    fs::write(project.join("index.panta.md"), "# Panta: Profile Prices\n")
        .expect("project manifest");
    for (member, state) in [("alpha", "work-a"), ("beta", "work-b")] {
        let root = project.join(member);
        fs::create_dir_all(root.join("tasks")).expect("member tasks");
        fs::write(root.join("index.rhei.md"), format!("# Rhei: {member}\n"))
            .expect("member manifest");
        fs::write(root.join("tasks/work.md"), format!("### Task 1: Work\n**State:** {state}\n"))
            .expect("member task");
    }
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: project-profile-prices
version: 1
models: [a, b]
states:
  work-a:
    description: A
    agent: codex
    model: a
  work-b:
    description: B
    agent: codex
    model: b
  completed:
    final: true
    description: Done
transitions:
  - from: work-a
    to: completed
  - from: work-b
    to: completed
"#,
    );
    let rates_a = profile_prices("CHF", 4_000_000, "project a");
    let rates_b = profile_prices("CHF", 6_000_000, "project b");
    write_settings(
        &project,
        serde_json::json!({
            "a": profile("openai", "project-model-a", Some(rates_a.clone())),
            "b": profile("openai", "project-model-b", Some(rates_b.clone()))
        }),
        None,
        "",
    );
    let result =
        run_cli("run", &project, &machine, &["--no-tui", "--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    let expected = expected_generated_book(
        "CHF",
        &[
            ("openai", "project-model-a", &["a"], rates_a),
            ("openai", "project-model-b", &["b"], rates_b),
        ],
    );
    let id = expected["price_book_id"].as_str().expect("generated id");
    for root in [&project, &project.join("alpha"), &project.join("beta")] {
        assert_eq!(read_json(&root.join("runtime/accounting/prices.json")), expected);
        assert_eq!(
            read_json(&root.join("runtime/accounting/price-books").join(format!("{id}.json"))),
            expected
        );
    }
}

/// Identical same-pair profile entries deduplicate and retain both identities.
// §FS-rhei-cost-accounting.5.1
#[test]
fn identical_same_pair_profile_entries_deduplicate() {
    let rates = profile_prices("CHF", 4_000_000, "shared rate");
    let (dir, plan, machine, _) = two_profile_fixture(
        "profile-prices-deduplicate",
        "shared-model",
        rates.clone(),
        "shared-model",
        rates.clone(),
    );
    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    let expected =
        expected_generated_book("CHF", &[("openai", "shared-model", &["a", "b"], rates)]);
    assert_eq!(read_json(&dir.join("runtime/accounting/prices.json")), expected);
    assert_eq!(invocation_records(&dir).len(), 2);
}

/// Same-pair differences and mixed currencies are both pre-spawn errors.
// §FS-rhei-cost-accounting.5.1
#[test]
fn conflicting_profile_sources_fail_before_any_agent_starts() {
    let cases = [
        (
            "same-pair",
            "shared-model",
            profile_prices("CHF", 4_000_000, "tier a"),
            "shared-model",
            profile_prices("CHF", 5_000_000, "tier b"),
            "shared-model",
        ),
        (
            "mixed-currency",
            "model-a",
            profile_prices("CHF", 4_000_000, "tier a"),
            "model-b",
            profile_prices("USD", 4_000_000, "tier b"),
            "currency",
        ),
    ];
    for (label, model_a, rates_a, model_b, rates_b, diagnostic) in cases {
        let (dir, plan, machine, spawned) = two_profile_fixture(
            &format!("profile-prices-conflict-{label}"),
            model_a,
            rates_a,
            model_b,
            rates_b,
        );
        let result =
            run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--parallel", "2"]);
        assert!(!result.status.success(), "{label} must fail");
        let output = format!("{}{}", result.stdout, result.stderr);
        assert!(output.contains(diagnostic), "{label} diagnostic:\n{output}");
        assert!(output.contains('a') && output.contains('b'), "profile ids missing:\n{output}");
        assert!(!spawned.exists(), "{label} spawned an agent before preflight");
        assert!(!dir.join("runtime/accounting/prices.json").exists());
    }
}

/// An explicit book bypasses profile composition and keeps exact matching.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-run.2.1
#[test]
fn explicit_book_overrides_conflicting_profiles() {
    let (dir, plan, machine, _) = two_profile_fixture(
        "profile-prices-explicit-override",
        "shared-model",
        profile_prices("CHF", 4_000_000, "tier a"),
        "shared-model",
        profile_prices("USD", 5_000_000, "tier b"),
    );
    let explicit = serde_json::json!({
        "schema": "rhei.accounting.prices.v1",
        "price_book_id": "explicit-shared-model",
        "currency": "EUR",
        "entries": [{
            "provider": "openai",
            "model": "shared-model",
            "effective_at": "2026-09-18T00:00:00Z",
            "unit": "1m_tokens",
            "input_total_micro": 1_000_000,
            "input_cached_read_micro": 100_000,
            "input_cache_write_micro": 2_000_000,
            "output_total_micro": 3_000_000
        }]
    });
    let path = write_fixture_file(
        &dir,
        "explicit-prices.json",
        &serde_json::to_string_pretty(&explicit).expect("explicit book"),
    );
    let path_arg = path.to_string_lossy().into_owned();
    let result = run_cli(
        "run",
        &plan,
        &machine,
        &["--no-tui", "--no-callbacks", "--parallel", "2", "--prices", &path_arg],
    );
    assert_success(&result);
    assert_eq!(read_json(&dir.join("runtime/accounting/prices.json")), explicit);
    for (_, record) in invocation_records(&dir) {
        assert_eq!(record["pricing"]["price_book_id"], "explicit-shared-model");
        assert_eq!(record["pricing"]["status"], "priced");
    }
    assert!(!dir.join("runtime/accounting/price-books/explicit-shared-model.json").exists());
}

/// Profiles without rates retain built-in exact matching or explicit unpriced
/// results and do not manufacture a generated book.
// §FS-rhei-cost-accounting.5.1
#[test]
fn profiles_without_prices_keep_builtin_and_unpriced_fallbacks() {
    for (label, provider, model, status) in [
        ("builtin", "anthropic", "claude-sonnet-4-6", "priced"),
        ("unpriced", "openai", "unknown-model", "unpriced"),
    ] {
        let dir = unique_temp_dir(&format!("profile-prices-fallback-{label}"));
        let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
        let machine =
            write_fixture_file(&dir, "states.yaml", &one_model_machine(Some("priced"), None));
        write_settings(
            &dir,
            serde_json::json!({ "priced": profile(provider, model, None) }),
            None,
            "",
        );
        let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
        assert_success(&result);
        let record = &invocation_records(&dir)[0].1;
        assert_eq!(record["pricing"]["status"], status, "{label}");
        assert_eq!(record["pricing"]["price_book_id"], "builtin-2026-05-20", "{label}");
        assert!(!dir.join("runtime/accounting/price-books").exists());
    }
}

/// A later settings rate creates a new current book while the first record and
/// its content-addressed source remain reachable and unchanged.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.5.2
#[test]
fn settings_rate_change_keeps_old_record_and_archived_book_reachable() {
    let dir = unique_temp_dir("profile-prices-history");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", &one_model_machine(Some("priced"), None));
    let first_rates = profile_prices("CHF", 4_000_000, "first rate");
    write_settings(
        &dir,
        serde_json::json!({ "priced": profile("openai", "history-model", Some(first_rates.clone())) }),
        None,
        "",
    );
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
    let first_book =
        expected_generated_book("CHF", &[("openai", "history-model", &["priced"], first_rates)]);
    let first_id = first_book["price_book_id"].as_str().expect("first id").to_string();
    let first_records = invocation_records(&dir);
    assert_eq!(first_records.len(), 1);
    let first_path = first_records[0].0.clone();
    let first_bytes = fs::read(&first_path).expect("first record bytes");

    fs::write(&plan, ONE_TASK_PLAN).expect("reset task for the second run");
    let second_rates = profile_prices("CHF", 8_000_000, "second rate");
    write_settings(
        &dir,
        serde_json::json!({ "priced": profile("openai", "history-model", Some(second_rates.clone())) }),
        None,
        "",
    );
    assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
    let second_book =
        expected_generated_book("CHF", &[("openai", "history-model", &["priced"], second_rates)]);
    let second_id = second_book["price_book_id"].as_str().expect("second id");
    assert_ne!(first_id, second_id);
    assert_eq!(fs::read(&first_path).expect("old record remains"), first_bytes);
    assert_eq!(
        read_json(&dir.join("runtime/accounting/price-books").join(format!("{first_id}.json"))),
        first_book
    );
    assert_eq!(
        read_json(&dir.join("runtime/accounting/price-books").join(format!("{second_id}.json"))),
        second_book
    );
    assert_eq!(read_json(&dir.join("runtime/accounting/prices.json"))["price_book_id"], second_id);
    let records = invocation_records(&dir);
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|(_, record)| record["pricing"]["price_book_id"] == first_id));
    assert!(records.iter().any(|(_, record)| record["pricing"]["price_book_id"] == second_id));
    let summary = read_json(&dir.join("runtime/accounting/summary.json"));
    assert_eq!(summary["summary"]["coverage"], "complete");
    assert_eq!(summary["summary"]["pricing_status"], "priced");
    assert_eq!(summary["summary"]["cost_micro"], 38_900_000);
}
