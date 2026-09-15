//! The diagnostic path from a rejected structured result through task-detail
//! inspection. §FS-rhei-cost-accounting.3.2 §FS-rhei-cost-accounting.4
//! §FS-rhei-cost-accounting.8 §FS-rhei-cost-accounting.11

use std::fs;
use std::path::Path;

use super::accounting_support::{
    accounting_workspace_with_agent, invocation_records, WORKING_PLAN,
};
use super::*;

const MISSING_RESULT_DIAGNOSTIC: &str =
    "claude-code result envelope is missing required `result` text";
const HISTORICAL_FALLBACK: &str = "usage capture reported extractor-failed without a diagnostic";

fn capture_events(root: &Path) -> Vec<serde_json::Value> {
    let directory = root.join("runtime/accounting/captures");
    let mut events = Vec::new();
    for entry in fs::read_dir(&directory).expect("read usage captures") {
        let path = entry.expect("capture entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
            continue;
        }
        let text = fs::read_to_string(path).expect("read usage capture");
        events.extend(
            text.lines().map(|line| {
                serde_json::from_str(line).expect("every retained capture line is JSON")
            }),
        );
    }
    events
}

fn run_fixture(prefix: &str, body: &str) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let (dir, plan, machine) = accounting_workspace_with_agent(prefix, WORKING_PLAN, body);
    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);
    assert_all_tasks_in_state(&plan, &machine, "completed");
    assert!(
        run.stdout.contains("Task plan.1 auto-advanced: 'work' -> 'completed'")
            && run.stdout.contains("1/1 tasks in terminal state"),
        "the accounting failure must not halt completion or transitions:\n{}",
        run.stdout
    );
    (dir, plan, machine)
}

/// The issue's incomplete Claude envelope, with the fixture's task result
/// supplied so only the missing diagnostic can fail this regression.
#[test]
fn incomplete_claude_result_diagnostic_survives_capture_record_and_task_inspection() {
    let (dir, plan, machine) = run_fixture(
        "accounting-extraction-diagnostic",
        r#"import json

print(json.dumps({
    'type': 'result',
    'usage': {'input_tokens': 123},
}), flush=True)
result('## Result\n\nMalformed usage fixture finished.\n')
"#,
    );

    let events = capture_events(&dir);
    let failed_event = events
        .iter()
        .find(|event| event["status"] == "extractor-failed")
        .expect("the malformed result writes an extractor-failed event");
    let records = invocation_records(&dir);
    assert_eq!(records.len(), 1, "one fixture invocation");
    assert_eq!(records[0]["extraction_status"], "extractor-failed");

    let text = run_cli("cost", &plan, &machine, &["--task", "plan.1"]);
    assert_success(&text);
    let json = run_cli("cost", &plan, &machine, &["--json", "--task", "plan.1"]);
    assert_success(&json);
    let payload: serde_json::Value = serde_json::from_str(&json.stdout).expect("task-detail JSON");
    let published = payload["task"]["invocations"]
        .as_array()
        .expect("task detail exposes its invocation array");
    assert_eq!(published.len(), 1);

    let expected = serde_json::json!([MISSING_RESULT_DIAGNOSTIC]);
    let mut failures = Vec::new();
    if failed_event.get("diagnostic") != Some(&serde_json::json!(MISSING_RESULT_DIAGNOSTIC)) {
        failures.push(format!("capture event has no exact diagnostic: {failed_event}"));
    }
    if records[0].get("extraction_diagnostics") != Some(&expected) {
        failures.push(format!("durable invocation has no diagnostic array: {}", records[0]));
    }
    let expected_line = format!("      extractor-failed: {MISSING_RESULT_DIAGNOSTIC}");
    if !text.stdout.lines().any(|line| line == expected_line) {
        failures.push(format!("task text has no exact diagnostic line:\n{}", text.stdout));
    }
    if published[0].get("extraction_diagnostics") != Some(&expected) {
        failures.push(format!("task JSON has no diagnostic array: {}", published[0]));
    }
    assert!(
        failures.is_empty(),
        "the extraction reason did not survive every required surface:\n{}",
        failures.join("\n\n")
    );
}

/// Parser diagnostics describe the rejected shape without copying the raw
/// payload into an accounting artifact. §FS-rhei-cost-accounting.4
#[test]
fn parser_diagnostic_is_bounded_single_line_and_does_not_quote_raw_output() {
    const RAW_MARKER: &str = "SECRET_RAW_AGENT_OUTPUT_263";
    let body = format!(
        r#"import json

print(json.dumps({{
    'type': 'result',
    'result': {{'private': '{RAW_MARKER}'}},
    'usage': {{'input_tokens': 123}},
}}), flush=True)
result('## Result\n\nMalformed usage fixture finished.\n')
"#
    );
    let (dir, _, _) = run_fixture("accounting-safe-extraction-diagnostic", &body);
    let events = capture_events(&dir);
    let diagnostic = events
        .iter()
        .find(|event| event["status"] == "extractor-failed")
        .and_then(|event| event["diagnostic"].as_str())
        .expect("extractor-failed capture event carries a diagnostic");

    assert!(!diagnostic.is_empty());
    assert!(diagnostic.chars().count() <= 240, "diagnostic was {diagnostic:?}");
    assert!(!diagnostic.contains(['\n', '\r']), "diagnostic was {diagnostic:?}");
    assert!(!diagnostic.contains(RAW_MARKER), "diagnostic quoted raw output: {diagnostic}");
}

/// Capture aggregation preserves reason boundaries and order while a retained
/// failure still outranks measured telemetry. §FS-rhei-cost-accounting.4
#[test]
fn capture_aggregation_keeps_distinct_reasons_in_order_and_failure_precedence() {
    let (dir, _, _) = run_fixture(
        "accounting-ordered-extraction-diagnostics",
        r#"capture = env('RHEI_ACCOUNTING_USAGE_PATH')
append(capture, '{"schema":"rhei.accounting.usage.v1","status":"extractor-failed","diagnostic":"first reason"}\n')
append(capture, '{"schema":"rhei.accounting.usage.v1","usage":{"input_tokens":10,"output_tokens":2}}\n')
append(capture, '{"schema":"rhei.accounting.usage.v1","status":"extractor-failed","diagnostic":"first reason"}\n')
append(capture, '{"schema":"rhei.accounting.usage.v1","status":"extractor-failed","diagnostic":"second reason"}\n')
result('## Result\n\nOrdered failure fixture finished.\n')
"#,
    );
    let records = invocation_records(&dir);
    assert_eq!(records[0]["extraction_status"], "extractor-failed");
    assert_eq!(
        records[0]["extraction_diagnostics"],
        serde_json::json!(["first reason", "second reason"])
    );
}

/// A current writer supplies a factual reason when it aggregates an event
/// written before failure diagnostics existed. §FS-rhei-cost-accounting.4
#[test]
fn historical_failure_event_gets_the_stable_fallback_reason() {
    let (dir, _, _) = run_fixture(
        "accounting-historical-failure-event",
        r#"capture = env('RHEI_ACCOUNTING_USAGE_PATH')
append(capture, '{"schema":"rhei.accounting.usage.v1","status":"extractor-failed"}\n')
result('## Result\n\nHistorical failure fixture finished.\n')
"#,
    );
    let records = invocation_records(&dir);
    assert_eq!(records[0]["extraction_status"], "extractor-failed");
    assert_eq!(records[0]["extraction_diagnostics"], serde_json::json!([HISTORICAL_FALLBACK]));
}

/// Historical invocation records deserialize and republish without fabricating
/// a diagnostic field or an extra text line. §FS-rhei-cost-accounting.3
/// §FS-rhei-cost-accounting.8.4
#[test]
fn historical_failed_invocation_keeps_its_field_omission_and_text_rendering() {
    let (dir, plan, machine) =
        accounting_workspace_with_agent("accounting-historical-failed-record", WORKING_PLAN, "");
    let source = fixture_path("accounting-archive/codex-3-no-usage.json");
    let mut record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(source).expect("historical fixture"))
            .expect("historical invocation JSON");
    record["invocation_id"] = serde_json::json!("historical-failed-263");
    record["task_id"] = serde_json::json!("plan.1");
    record["agent"] = serde_json::json!("claude-code");
    record["model"] = serde_json::json!("claude-sonnet-4-6");
    record["extraction_status"] = serde_json::json!("extractor-failed");
    record.as_object_mut().expect("record object").remove("extraction_diagnostics");
    let directory = dir.join("runtime/accounting/invocations");
    fs::create_dir_all(&directory).expect("invocation directory");
    fs::write(
        directory.join("historical.json"),
        serde_json::to_vec_pretty(&record).expect("serialize historical record"),
    )
    .expect("write historical record");

    let text = run_cli("cost", &plan, &machine, &["--task", "plan.1"]);
    assert_success(&text);
    assert!(
        text.stdout.lines().any(|line| {
            line == "    historical-failed-263 claude-code claude-sonnet-4-6 unpriced"
        }),
        "historical invocation line moved:\n{}",
        text.stdout
    );
    assert!(!text.stdout.contains("extractor-failed:"), "unexpected added line:\n{}", text.stdout);

    let json = run_cli("cost", &plan, &machine, &["--json", "--task", "plan.1"]);
    assert_success(&json);
    let payload: serde_json::Value = serde_json::from_str(&json.stdout).expect("task JSON");
    assert!(payload["task"]["invocations"][0].get("extraction_diagnostics").is_none());
}
