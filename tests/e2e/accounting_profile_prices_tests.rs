//! Black-box coverage for profile-authored accounting rates, their generated
//! snapshots, and the compatibility boundaries around explicit and built-in
//! books. §FS-rhei-agents.1.1.3 §FS-rhei-agents.1.3
//! §FS-rhei-agents.1.4 §FS-rhei-cost-accounting.3
//! §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.5.2

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::*;

pub(super) const ONE_TASK_PLAN: &str = r#"# Rhei: Profile Prices

## Tasks

### Task 1: Measure this invocation
**State:** work
"#;

pub(super) fn one_model_machine(model: Option<&str>, target: Option<&str>) -> String {
    let selection = match (model, target) {
        (Some(model), None) => format!("    agent: codex\n    model: {model}\n"),
        (None, Some(target)) => format!("    target: {target}\n"),
        (None, None) => "    agent: codex\n".to_string(),
        _ => panic!("a state has one selection surface"),
    };
    format!(
        r#"name: profile-prices
version: 1
models: [base, priced, a, b]
states:
  work:
    initial: true
    description: Emit usage
{selection}    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
    )
}

pub(super) fn profile_prices(
    currency: &str,
    input_total_micro: u64,
    note: &str,
) -> serde_json::Value {
    serde_json::json!({
        "currency": currency,
        "effective_at": "2026-09-18T00:00:00Z",
        "input_total_micro": input_total_micro,
        "input_cached_read_micro": 400_000,
        "input_cache_write_micro": 5_000_000,
        "output_total_micro": 20_000_000,
        "note": note,
        "service_tier": "priority"
    })
}

pub(super) fn profile(
    provider: &str,
    model: &str,
    prices: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "provider": provider,
        "model": model,
        "default_agent": "codex"
    });
    if let Some(prices) = prices {
        value["prices"] = prices;
    }
    value
}

fn measured_agent(root: &Path, name: &str, before_usage: &str) -> PathBuf {
    write_python_agent(
        root,
        name,
        &format!(
            r#"import json
import pathlib
{before_usage}
print(json.dumps({{
    'type': 'turn.completed',
    'usage': {{
        'input_tokens': 1250000,
        'cached_input_tokens': 500000,
        'cache_creation_input_tokens': 250000,
        'output_tokens': 750000,
    }},
}}), flush=True)
result('## Result\n\nMeasured invocation completed.\n')
"#
        ),
    )
}

pub(super) fn write_settings(
    root: &Path,
    models: serde_json::Value,
    defaults_model: Option<&str>,
    before_usage: &str,
) {
    let script = measured_agent(root, "profile-prices-codex.py", before_usage);
    let command: serde_json::Value =
        serde_json::from_str(&fixture_command(&script)).expect("fixture command JSON");
    let mut settings = serde_json::json!({
        "agents": {
            "codex": {
                "command": command,
                "prompt_flag": "--prompt",
                "timeout": "10s"
            }
        },
        "models": models
    });
    if let Some(model) = defaults_model {
        settings["defaults"] = serde_json::json!({ "agent": "codex", "model": model });
    }
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("write settings");
}

pub(super) fn invocation_records(root: &Path) -> Vec<(PathBuf, serde_json::Value)> {
    let directory = root.join("runtime/accounting/invocations");
    let mut records = fs::read_dir(&directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .map(|path| {
            let value =
                serde_json::from_str(&fs::read_to_string(&path).expect("read invocation record"))
                    .expect("parse invocation record");
            (path, value)
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(&right.0));
    records
}

pub(super) fn expected_generated_book(
    currency: &str,
    entries: &[(&str, &str, &[&str], serde_json::Value)],
) -> serde_json::Value {
    let mut generated_entries = entries
        .iter()
        .map(|(provider, model, profiles, prices)| {
            let mut entry = prices.as_object().expect("prices object").clone();
            entry.remove("currency");
            entry.insert("provider".to_string(), serde_json::json!(provider));
            entry.insert("model".to_string(), serde_json::json!(model));
            entry.insert("unit".to_string(), serde_json::json!("1m_tokens"));
            entry.insert("source".to_string(), serde_json::json!({ "kind": "profile" }));
            let mut profiles = profiles.iter().map(|id| (*id).to_string()).collect::<Vec<_>>();
            profiles.sort();
            profiles.dedup();
            entry.insert("model_profiles".to_string(), serde_json::json!(profiles));
            serde_json::Value::Object(entry)
        })
        .collect::<Vec<_>>();
    generated_entries.sort_by(|left, right| {
        (left["provider"].as_str(), left["model"].as_str())
            .cmp(&(right["provider"].as_str(), right["model"].as_str()))
    });
    let semantic = serde_json::json!({
        "currency": currency,
        "entries": generated_entries,
        "schema": "rhei.accounting.prices.v1"
    });
    let digest = Sha256::digest(serde_json::to_vec(&semantic).expect("canonical JSON"));
    let id = format!("profiles-sha256-{digest:x}");
    let mut book = semantic;
    book["price_book_id"] = serde_json::json!(id);
    book
}

pub(super) fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display())),
    )
    .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn assert_profile_pricing(record: &serde_json::Value, profile: &str, book_id: &str, amount: u64) {
    assert_eq!(record["model_profile"], profile);
    assert_eq!(record["pricing"]["status"], "priced");
    assert_eq!(record["pricing"]["price_book_id"], book_id);
    assert_eq!(record["pricing"]["amount_micro"], amount);
    assert_eq!(record["pricing"]["priced_amount_micro"], amount);
}

/// A selected profile absent from the built-in book supplies the pre-agent
/// snapshot and exact rates without `--prices`.
// §FS-rhei-agents.1.1.3 §FS-rhei-cost-accounting.3
// §FS-rhei-cost-accounting.5.1 §FS-rhei-run.2.1
#[test]
fn selected_profile_prices_without_a_flag_and_archives_the_pre_agent_snapshot() {
    let dir = unique_temp_dir("profile-prices-primary");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", &one_model_machine(Some("priced"), None));
    let observed = dir.join("observed-pre-agent-book.json");
    let before = format!(
        "snapshot = pathlib.Path({})\nobservation = {{'exists': snapshot.exists()}}\nif snapshot.exists():\n    observation['book'] = json.loads(snapshot.read_text())\npathlib.Path({}).write_text(json.dumps(observation))",
        serde_json::to_string(dir.join("runtime/accounting/prices.json").to_string_lossy().as_ref())
            .expect("snapshot path"),
        serde_json::to_string(observed.to_string_lossy().as_ref()).expect("observation path")
    );
    let rates = profile_prices("CHF", 4_000_000, "primary profile rate");
    write_settings(
        &dir,
        serde_json::json!({ "priced": profile("openai", "site-model-x", Some(rates.clone())) }),
        None,
        &before,
    );

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let expected =
        expected_generated_book("CHF", &[("openai", "site-model-x", &["priced"], rates)]);
    let observed = read_json(&observed);
    assert_eq!(observed["exists"], true, "the snapshot must exist before usage is emitted");
    assert_eq!(observed["book"], expected, "the agent saw the wrong pre-agent snapshot");
    let current = read_json(&dir.join("runtime/accounting/prices.json"));
    assert_eq!(current, expected);
    let id = expected["price_book_id"].as_str().expect("generated id");
    assert_eq!(
        read_json(&dir.join("runtime/accounting/price-books").join(format!("{id}.json"))),
        expected
    );
    let records = invocation_records(&dir);
    assert_eq!(records.len(), 1);
    assert_profile_pricing(&records[0].1, "priced", id, 18_450_000);
}

/// Every invalid authored rate points to its complete settings path.
// §FS-rhei-agents.1.1.3
#[test]
fn invalid_profile_prices_report_the_full_field_path() {
    let cases = [
        (
            "empty-currency",
            r#"{"currency":"","effective_at":"now","input_total_micro":1,"input_cached_read_micro":1,"input_cache_write_micro":1,"output_total_micro":1}"#,
            "currency",
        ),
        (
            "missing-output",
            r#"{"currency":"USD","effective_at":"now","input_total_micro":1,"input_cached_read_micro":1,"input_cache_write_micro":1}"#,
            "output_total_micro",
        ),
        (
            "fraction",
            r#"{"currency":"USD","effective_at":"now","input_total_micro":1.5,"input_cached_read_micro":1,"input_cache_write_micro":1,"output_total_micro":1}"#,
            "input_total_micro",
        ),
        (
            "negative",
            r#"{"currency":"USD","effective_at":"now","input_total_micro":-1,"input_cached_read_micro":1,"input_cache_write_micro":1,"output_total_micro":1}"#,
            "input_total_micro",
        ),
        (
            "overflow",
            r#"{"currency":"USD","effective_at":"now","input_total_micro":18446744073709551616,"input_cached_read_micro":1,"input_cache_write_micro":1,"output_total_micro":1}"#,
            "input_total_micro",
        ),
        (
            "reserved-unit",
            r#"{"currency":"USD","effective_at":"now","input_total_micro":1,"input_cached_read_micro":1,"input_cache_write_micro":1,"output_total_micro":1,"unit":"1m_tokens"}"#,
            "unit",
        ),
    ];
    let mut failures = Vec::new();
    for (label, prices, field) in cases {
        let dir = unique_temp_dir(&format!("profile-prices-invalid-{label}"));
        let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
        let machine =
            write_fixture_file(&dir, "states.yaml", &one_model_machine(Some("priced"), None));
        let settings_dir = dir.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings_dir).expect("settings directory");
        fs::write(
            settings_dir.join("settings.json"),
            r#"{"agents":{"codex":{"command":["unused"]}},"models":{"priced":{"provider":"openai","model":"site-model-x","default_agent":"codex","prices":PRICES}}}"#
                .replace("PRICES", prices),
        )
        .expect("invalid settings fixture");
        let result = run_cli("validate", &plan, &machine, &[]);
        let expected = format!("models.priced.prices.{field}");
        if result.status.success()
            || !format!("{}{}", result.stdout, result.stderr).contains(&expected)
        {
            failures.push(format!(
                "{label}: expected refusal naming {expected}\nstdout:\n{}\nstderr:\n{}",
                result.stdout, result.stderr
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Absence inherits the object, an authored object replaces it, and null
/// clears it even when provider/model fields still inherit normally.
// §FS-rhei-agents.1.3 §FS-rhei-cost-accounting.5.1
#[test]
fn project_profile_prices_inherit_replace_and_clear_as_one_object() {
    for (label, project_prices, expected_currency, expected_status) in [
        ("inherit", None, "USD", "priced"),
        ("replace", Some(profile_prices("CHF", 7_000_000, "project replacement")), "CHF", "priced"),
        ("clear", Some(serde_json::Value::Null), "USD", "unpriced"),
    ] {
        let dir = unique_temp_dir(&format!("profile-prices-merge-{label}"));
        let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
        let machine =
            write_fixture_file(&dir, "states.yaml", &one_model_machine(Some("priced"), None));
        let home_settings = dir.join(".home/.config/rhei");
        fs::create_dir_all(&home_settings).expect("global settings directory");
        let script = measured_agent(&dir, "merge-codex.py", "");
        let command: serde_json::Value =
            serde_json::from_str(&fixture_command(&script)).expect("fixture command JSON");
        fs::write(
            home_settings.join("settings.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "agents": { "codex": { "command": command, "prompt_flag": "--prompt" } },
                "models": { "priced": profile("openai", "merge-model", Some(profile_prices("USD", 4_000_000, "global rate"))) }
            }))
            .expect("global settings"),
        )
        .expect("write global settings");
        let mut project_profile =
            serde_json::json!({ "provider": "openai", "model": "merge-model" });
        if let Some(prices) = project_prices {
            project_profile["prices"] = prices;
        }
        let project_dir = dir.join(".agent-grounds/rhei");
        fs::create_dir_all(&project_dir).expect("project settings directory");
        fs::write(
            project_dir.join("settings.json"),
            serde_json::to_string_pretty(
                &serde_json::json!({ "models": { "priced": project_profile } }),
            )
            .expect("project settings"),
        )
        .expect("write project settings");

        let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
        assert_success(&result);
        let record = &invocation_records(&dir)[0].1;
        assert_eq!(record["pricing"]["status"], expected_status, "{label}");
        assert_eq!(record["pricing"]["currency"], expected_currency, "{label}");
        assert_eq!(record["model_profile"], "priced", "{label}");
        if label == "replace" {
            let book = read_json(&dir.join("runtime/accounting/prices.json"));
            assert_eq!(book["entries"][0]["note"], "project replacement");
            assert_ne!(book["entries"][0]["note"], "global rate");
        }
    }
}

fn run_selector_case(
    label: &str,
    plan_text: &str,
    machine_text: &str,
    args: &[&str],
) -> serde_json::Value {
    let dir = unique_temp_dir(&format!("profile-identity-{label}"));
    let plan = write_fixture_file(&dir, "plan.rhei.md", plan_text);
    let machine = write_fixture_file(&dir, "states.yaml", machine_text);
    write_settings(
        &dir,
        serde_json::json!({
            "base": profile("openai", "base-model", None),
            "priced": profile("openai", "selected-model", None)
        }),
        (label == "default").then_some("priced"),
        "",
    );
    let result = run_cli("run", &plan, &machine, args);
    assert_success(&result);
    invocation_records(&dir).remove(0).1
}

/// Every named-model selector carries the chosen id into durable provenance.
// §FS-rhei-agents.1.4 §FS-rhei-cost-accounting.3
#[test]
fn named_model_precedence_surfaces_retain_profile_identity() {
    let state = one_model_machine(Some("priced"), None);
    let default = one_model_machine(None, None);
    let base = one_model_machine(Some("base"), None);
    let all_models = one_model_machine(Some("base"), None)
        .replace("    model: base\n", "    all_models: [priced]\n");
    let task_model = ONE_TASK_PLAN.replace("**State:** work", "**State:** work\n**Model:** priced");
    for (label, plan, machine, args) in [
        ("default", ONE_TASK_PLAN.to_string(), default, vec![]),
        ("state", ONE_TASK_PLAN.to_string(), state, vec![]),
        ("cli", ONE_TASK_PLAN.to_string(), base.clone(), vec!["--model", "priced"]),
        ("all-models", ONE_TASK_PLAN.to_string(), all_models, vec![]),
        ("task", task_model, base, vec![]),
    ] {
        let record = run_selector_case(label, &plan, &machine, &args);
        assert_eq!(record["model_profile"], "priced", "selector {label}");
        assert_eq!(record["model"], "selected-model", "selector {label}");
    }
}

/// A literal target does not acquire a same-pair profile or its rates.
// §FS-rhei-agents.1.4 §FS-rhei-cost-accounting.5.1
#[test]
fn literal_target_does_not_implicitly_select_a_profile() {
    let dir = unique_temp_dir("profile-prices-literal-control");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &one_model_machine(None, Some("codex:openai:literal-model")),
    );
    write_settings(
        &dir,
        serde_json::json!({
            "priced": profile("openai", "literal-model", Some(profile_prices("CHF", 4_000_000, "must not leak")))
        }),
        None,
        "",
    );
    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    let record = &invocation_records(&dir)[0].1;
    assert!(record.get("model_profile").is_none());
    assert_eq!(record["pricing"]["status"], "unpriced");
    assert_eq!(record["pricing"]["price_book_id"], "builtin-2026-05-20");
}

/// A named priced model over a literal provider must still match its declared
/// pair; it is rejected before spawn when it does not.
// §FS-rhei-agents.1.4 §FS-rhei-cost-accounting.5.1
#[test]
fn priced_task_model_mismatch_with_literal_target_fails_before_spawn() {
    let dir = unique_temp_dir("profile-prices-literal-mismatch");
    let plan_text = ONE_TASK_PLAN.replace("**State:** work", "**State:** work\n**Model:** priced");
    let plan = write_fixture_file(&dir, "plan.rhei.md", &plan_text);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &one_model_machine(None, Some("codex:anthropic:literal-model")),
    );
    let spawned = dir.join("spawned.marker");
    write_settings(
        &dir,
        serde_json::json!({
            "priced": profile("openai", "selected-model", Some(profile_prices("CHF", 4_000_000, "profile")))
        }),
        None,
        &format!(
            "pathlib.Path({}).write_text('spawned')",
            serde_json::to_string(spawned.to_string_lossy().as_ref()).expect("marker")
        ),
    );
    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!result.status.success(), "mismatched profile pair must fail");
    let output = format!("{}{}", result.stdout, result.stderr);
    for expected in ["priced", "openai", "selected-model", "anthropic"] {
        assert!(output.contains(expected), "missing {expected:?} from:\n{output}");
    }
    assert!(!spawned.exists(), "the agent spawned before mismatch refusal");
}

/// A literal and a priced profile on the same pair cannot share the profile's
/// rate by accident; their effective sources conflict before either spawn.
// §FS-rhei-agents.1.4 §FS-rhei-cost-accounting.5.1
#[test]
fn same_pair_profile_and_literal_sources_conflict_before_spawn() {
    let dir = unique_temp_dir("profile-prices-literal-conflict");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Literal Conflict\n\n## Tasks\n\n### Task 1: Profile\n**State:** profiled\n\n### Task 2: Literal\n**State:** literal\n",
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: literal-conflict
version: 1
models: [priced]
states:
  profiled:
    description: Named profile
    agent: codex
    model: priced
  literal:
    description: Literal target
    target: codex:openai:shared-model
  completed:
    final: true
    description: Done
transitions:
  - from: profiled
    to: completed
  - from: literal
    to: completed
"#,
    );
    let spawned = dir.join("spawned.marker");
    write_settings(
        &dir,
        serde_json::json!({
            "priced": profile("openai", "shared-model", Some(profile_prices("CHF", 4_000_000, "profile-only tier")))
        }),
        None,
        &format!(
            "pathlib.Path({}).write_text('spawned')",
            serde_json::to_string(spawned.to_string_lossy().as_ref()).expect("marker")
        ),
    );
    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--parallel", "2"]);
    assert!(!result.status.success(), "profile/literal source conflict must fail");
    let output = format!("{}{}", result.stdout, result.stderr);
    for expected in ["openai", "shared-model", "priced", "literal"] {
        assert!(output.contains(expected), "missing {expected:?} from:\n{output}");
    }
    assert!(!spawned.exists(), "an agent spawned before source-conflict refusal");
}
