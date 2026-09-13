//! Black-box contract coverage for published accounting schemas.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const SCHEMA_IDS: [&str; 6] = [
    "rhei.accounting.cost.v1",
    "rhei.accounting.invocation.v1",
    "rhei.accounting.prices.v1",
    "rhei.accounting.summary.v1",
    "rhei.accounting.task.v1",
    "rhei.accounting.usage.v1",
];

const ACCOUNTING_MACHINE: &str = r#"name: accounting-contract
version: 1
models: [contract]
states:
  work:
    initial: true
    description: Emit accounting artifacts
    agent: codex
    model: contract
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

const ACCOUNTING_PLAN: &str = r#"# Rhei: Accounting Contract

## Tasks

### Task 1: Produce one measured invocation
**State:** work
"#;

fn schema_output(home: &Path, schema_id: &str) -> CliRun {
    let output =
        rhei_command(home).args(["schema", schema_id]).output().expect("schema command runs");
    CliRun::from(&output)
}

fn first_file(directory: &Path, extension: &str) -> PathBuf {
    let mut paths = fs::read_dir(directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 1, "expected one {extension} file under {}", directory.display());
    paths.remove(0)
}

fn validate(schema: &serde_json::Value, instance: &serde_json::Value, label: &str) {
    // Rhei's contract id is intentionally the same self-identifying token used
    // in artifacts, which is a relative URI reference. Validation has no
    // retrieval base or external references, so compile the same schema with
    // only that annotation removed.
    let mut schema_for_validation = schema.clone();
    schema_for_validation.as_object_mut().expect("schema object").remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema_for_validation)
        .unwrap_or_else(|err| panic!("compile schema for {label}: {err}"));
    if let Err(errors) = compiled.validate(instance) {
        let messages = errors.map(|error| error.to_string()).collect::<Vec<_>>().join("\n");
        panic!("{label} did not validate:\n{messages}\ninstance:\n{instance:#}");
    };
}

fn assert_invalid(schema: &serde_json::Value, instance: &serde_json::Value, label: &str) {
    let mut schema_for_validation = schema.clone();
    schema_for_validation.as_object_mut().expect("schema object").remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema_for_validation)
        .unwrap_or_else(|err| panic!("compile schema for {label}: {err}"));
    assert!(!compiled.is_valid(instance), "{label} unexpectedly validated:\n{instance:#}");
}

fn add_unknown_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for child in object.values_mut() {
                add_unknown_fields(child);
            }
            object.insert("future_optional".to_string(), serde_json::json!(true));
        }
        serde_json::Value::Array(array) => {
            for child in array {
                add_unknown_fields(child);
            }
        }
        _ => {}
    }
}

fn replace_pointer(
    instance: &mut serde_json::Value,
    pointer: &str,
    replacement: serde_json::Value,
) {
    *instance
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("fixture has JSON pointer {pointer}")) = replacement;
}

fn assert_pricing_contract(
    schema: &serde_json::Value,
    artifact: &serde_json::Value,
    pointer: &str,
    label: &str,
) {
    let priced = serde_json::json!({
        "status": "priced",
        "currency": "USD",
        "amount_micro": 42,
        "priced_amount_micro": 42,
        "price_book_id": "fixture"
    });
    let mut valid_priced = artifact.clone();
    replace_pointer(&mut valid_priced, pointer, priced.clone());
    validate(schema, &valid_priced, &format!("{label} priced variant"));

    let mut priced_without_subtotal = artifact.clone();
    let mut incomplete_priced = priced;
    incomplete_priced.as_object_mut().expect("pricing object").remove("priced_amount_micro");
    replace_pointer(&mut priced_without_subtotal, pointer, incomplete_priced);
    assert_invalid(schema, &priced_without_subtotal, &format!("{label} incomplete priced"));

    for partial in [
        serde_json::json!({
            "status": "partial-price",
            "currency": "USD",
            "price_book_id": "fixture"
        }),
        serde_json::json!({
            "status": "partial-price",
            "currency": "USD",
            "priced_amount_micro": 21,
            "price_book_id": "fixture"
        }),
    ] {
        let mut valid_partial = artifact.clone();
        replace_pointer(&mut valid_partial, pointer, partial);
        validate(schema, &valid_partial, &format!("{label} partial-price variant"));
    }

    let mut partial_with_full_amount = artifact.clone();
    replace_pointer(
        &mut partial_with_full_amount,
        pointer,
        serde_json::json!({
            "status": "partial-price",
            "currency": "USD",
            "amount_micro": 42,
            "price_book_id": "fixture"
        }),
    );
    assert_invalid(
        schema,
        &partial_with_full_amount,
        &format!("{label} partial-price with full amount"),
    );
}

fn assert_negative_contract_cases(
    schema_id: &str,
    schema: &serde_json::Value,
    artifact: &serde_json::Value,
) {
    let (required_key, enum_pointer, bad_enum) = match schema_id {
        "rhei.accounting.cost.v1" => ("errors", "/summary/coverage", "invalid-coverage"),
        "rhei.accounting.invocation.v1" => ("tokens", "/extraction_status", "invalid-extraction"),
        "rhei.accounting.prices.v1" => ("currency", "/entries/0/unit", "per-token"),
        "rhei.accounting.summary.v1" => ("summary", "/summary/pricing_status", "invalid-pricing"),
        "rhei.accounting.task.v1" => ("subtree", "/direct/pricing_status", "invalid-pricing"),
        "rhei.accounting.usage.v1" => ("schema", "/schema", "rhei.accounting.usage.v2"),
        _ => panic!("unhandled schema {schema_id}"),
    };

    let mut missing_required = artifact.clone();
    missing_required.as_object_mut().expect("artifact object").remove(required_key);
    assert_invalid(schema, &missing_required, &format!("{schema_id} missing {required_key}"));

    let mut bad_enum_artifact = artifact.clone();
    replace_pointer(&mut bad_enum_artifact, enum_pointer, serde_json::json!(bad_enum));
    assert_invalid(schema, &bad_enum_artifact, &format!("{schema_id} invalid enum"));

    let (shape_pointer, malformed_shape) = match schema_id {
        "rhei.accounting.cost.v1" => (
            "/task/invocations/0/tokens/input/total",
            serde_json::json!({
                "value": 1,
                "source": "fixture",
                "status": "unsupported"
            }),
        ),
        "rhei.accounting.invocation.v1" => (
            "/tokens/input/total",
            serde_json::json!({
                "value": 1,
                "source": "fixture",
                "status": "unsupported"
            }),
        ),
        "rhei.accounting.prices.v1" => {
            ("/entries/0/input_total_micro", serde_json::json!({ "value": 1, "source": "fixture" }))
        }
        "rhei.accounting.summary.v1" => {
            ("/summary/total", serde_json::json!({ "value": 1, "source": "fixture" }))
        }
        "rhei.accounting.task.v1" => {
            ("/direct/total", serde_json::json!({ "value": 1, "source": "fixture" }))
        }
        "rhei.accounting.usage.v1" => {
            ("/usage/input_tokens", serde_json::json!({ "value": 1, "source": "fixture" }))
        }
        _ => panic!("unhandled schema {schema_id}"),
    };
    let mut bad_shape = artifact.clone();
    replace_pointer(&mut bad_shape, shape_pointer, malformed_shape);
    assert_invalid(schema, &bad_shape, &format!("{schema_id} malformed token shape"));

    match schema_id {
        "rhei.accounting.invocation.v1" => {
            assert_pricing_contract(schema, artifact, "/pricing", schema_id);
        }
        "rhei.accounting.cost.v1" => {
            assert_pricing_contract(schema, artifact, "/task/invocations/0/pricing", schema_id);
        }
        "rhei.accounting.usage.v1" => {
            let failed = serde_json::json!({
                "schema": "rhei.accounting.usage.v1",
                "status": "extractor-failed",
                "future_optional": true
            });
            validate(schema, &failed, "open extractor-failed usage variant");
            let mut mixed = failed;
            mixed["usage"] = serde_json::json!({ "input_tokens": 1 });
            assert_invalid(schema, &mixed, "usage event mixing both variants");
        }
        _ => {}
    }
}

fn write_contract_agent_settings(root: &Path) {
    let script = write_python_agent(
        root,
        "accounting-contract-codex.py",
        r#"import json

print(json.dumps({'type': 'thread.started', 'thread_id': 'thread-contract-141'}), flush=True)
print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': 120,
        'cached_input_tokens': 20,
        'cache_creation_input_tokens': 5,
        'output_tokens': 30,
    },
}), flush=True)
result('## Result\n\nAccounting contract emitted.\n')
"#,
    );
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "codex", "model": "contract", "agent_timeout": "10s" }},
  "agents": {{
    "codex": {{ "command": {}, "prompt_flag": "--prompt", "timeout": "10s" }}
  }},
  "models": {{
    "contract": {{ "provider": "openai", "model": "gpt-contract", "default_agent": "codex" }}
  }}
}}"#,
            fixture_command(&script)
        ),
    )
    .expect("write settings");
}

/// Every published id lists and prints the exact versioned schema source; an
/// unknown id fails with a discovery hint. §FS-rhei-cost-accounting.8.1
#[test]
fn schema_command_lists_and_prints_every_published_contract() {
    let dir = unique_temp_dir("accounting-schema-command");
    let bare =
        rhei_command(dir.join("home")).arg("schema").output().expect("bare schema list runs");
    assert!(bare.status.success(), "{}", stderr(&bare));
    assert_eq!(stdout(&bare), SCHEMA_IDS.join("\n") + "\n");

    let output = rhei_command(dir.join("home"))
        .args(["schema", "--list"])
        .output()
        .expect("schema list runs");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), SCHEMA_IDS.join("\n") + "\n");

    for schema_id in SCHEMA_IDS {
        let result = schema_output(&dir.join("home"), schema_id);
        assert_success(&result);
        let source = fs::read_to_string(
            repo_root().join("crates/rhei-cli/schemas").join(format!("{schema_id}.schema.json")),
        )
        .expect("versioned schema source");
        assert_eq!(result.stdout, source);
        let parsed: serde_json::Value = serde_json::from_str(&result.stdout).expect("schema JSON");
        assert_eq!(parsed["$id"], schema_id);
    }

    let unknown = schema_output(&dir.join("home"), "rhei.accounting.unknown.v1");
    assert!(!unknown.status.success());
    assert_stderr_contains(&unknown, "unknown accounting schema id 'rhei.accounting.unknown.v1'");
    assert_stderr_contains(&unknown, "rhei schema --list");
}

/// The published price-book contract exposes additive extension points at the
/// same two object levels consumed by `rhei run --prices`.
// §FS-rhei-cost-accounting.8.1
#[test]
fn accounting_prices_schema_opens_document_and_entry_objects() {
    let dir = unique_temp_dir("accounting-prices-schema-extensions");
    let result = schema_output(&dir.join("home"), "rhei.accounting.prices.v1");
    assert_success(&result);
    let schema: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("published price-book schema JSON");

    assert_eq!(schema["additionalProperties"], true);
    assert_eq!(schema["$defs"]["entry"]["additionalProperties"], true);
}

/// One actual run produces all six artifact/output shapes accepted by their
/// published schemas, including exact CLI session identity and duration.
// §FS-rhei-cost-accounting.3.1 §FS-rhei-cost-accounting.3.4 §FS-rhei-cost-accounting.5
// §FS-rhei-cost-accounting.8.1
#[test]
fn run_artifacts_and_cost_json_validate_against_published_schemas() {
    let dir = unique_temp_dir("accounting-schema-run");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ACCOUNTING_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", ACCOUNTING_MACHINE);
    write_contract_agent_settings(&dir);

    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);

    let accounting = dir.join("runtime/accounting");
    let invocation_path = first_file(&accounting.join("invocations"), "json");
    let task_path = first_file(&accounting.join("tasks"), "json");
    let capture_path = first_file(&accounting.join("captures"), "jsonl");
    let invocation: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(invocation_path).expect("invocation"))
            .expect("invocation JSON");
    assert_eq!(invocation["cli_session"]["id"], "thread-contract-141");
    assert!(invocation["cli_session"].get("store_path").is_none());
    assert!(invocation["duration_ms"].as_u64().is_some());

    let task_id = invocation["task_id"].as_str().expect("task id");
    let cost = run_cli("cost", &plan, &machine, &["--json", "--task", task_id]);
    assert_success(&cost);
    let cost_json: serde_json::Value = serde_json::from_str(&cost.stdout).expect("cost JSON");

    let artifacts = [
        ("rhei.accounting.invocation.v1", invocation),
        (
            "rhei.accounting.summary.v1",
            serde_json::from_str(
                &fs::read_to_string(accounting.join("summary.json")).expect("summary"),
            )
            .expect("summary JSON"),
        ),
        (
            "rhei.accounting.task.v1",
            serde_json::from_str(&fs::read_to_string(task_path).expect("task")).expect("task JSON"),
        ),
        (
            "rhei.accounting.prices.v1",
            serde_json::from_str(
                &fs::read_to_string(accounting.join("prices.json")).expect("prices"),
            )
            .expect("prices JSON"),
        ),
        (
            "rhei.accounting.usage.v1",
            serde_json::from_str(
                fs::read_to_string(capture_path)
                    .expect("usage capture")
                    .lines()
                    .next()
                    .expect("usage event"),
            )
            .expect("usage JSON"),
        ),
        ("rhei.accounting.cost.v1", cost_json),
    ];

    for (schema_id, artifact) in artifacts {
        let schema_result = schema_output(&dir.join("schema-home"), schema_id);
        assert_success(&schema_result);
        let schema: serde_json::Value =
            serde_json::from_str(&schema_result.stdout).expect("published schema JSON");
        validate(&schema, &artifact, schema_id);

        let mut additive_artifact = artifact.clone();
        add_unknown_fields(&mut additive_artifact);
        validate(&schema, &additive_artifact, &format!("{schema_id} additive fields"));
        assert_negative_contract_cases(schema_id, &schema, &artifact);

        if schema_id == "rhei.accounting.invocation.v1" {
            let mut old_artifact = artifact.clone();
            let old_object = old_artifact.as_object_mut().expect("invocation object");
            old_object.remove("duration_ms");
            old_object.remove("cli_session");
            validate(&schema, &old_artifact, "pre-session invocation v1");
        }
    }
}
