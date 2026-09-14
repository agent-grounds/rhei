//! Attempt identity from historical v1 records through newly spawned retries.
//!
//! §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.4
//! §FS-rhei-cost-accounting.7.1 §FS-rhei-cost-accounting.11
//! §FS-rhei-panta.6.5

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::*;

const PLAN: &str = r#"# Rhei: Attempt Identity

## Tasks

### Task 1: Retry once
**State:** work
"#;

const PASSIVE_MACHINE: &str = r#"name: attempt-identity-history
version: 1
states:
  work:
    initial: true
    description: Historical work
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

const RETRY_MACHINE: &str = r#"name: attempt-identity-writer
version: 1
models: [luna]
states:
  work:
    initial: true
    description: Miss once, then finish
    agent: codex
    model: luna
    attempts: 3
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

fn historical_record(
    run_id: &str,
    started_at: &str,
    ended_at: &str,
    total: u64,
    cost_micro: u64,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "rhei.accounting.invocation.v1",
        "invocation_id": "plan.1::work::codex-openai-luna::visit-1",
        "run_id": run_id,
        "task_id": "plan.1",
        "state": "work",
        "visit": 1,
        "target_slug": "codex-openai-luna",
        "agent": "codex",
        "provider": "openai",
        "model": "gpt-5.6-luna",
        "started_at": started_at,
        "ended_at": ended_at,
        "duration_ms": 1000,
        "extraction_status": "measured",
        "scope": "aggregate-agent-process",
        "token_convention": "input-total-includes-cache",
        "tokens": {
            "total": { "value": total, "source": "agent-usage-capture" },
            "input": {
                "total": { "value": total - 10, "source": "agent-usage-capture" },
                "cached_read": { "value": 0, "source": "agent-usage-capture" },
                "cache_write": { "value": 0, "source": "agent-usage-capture" }
            },
            "output": {
                "total": { "value": 10, "source": "agent-usage-capture" },
                "cached_read": { "status": "unsupported" },
                "cache_write": { "status": "unsupported" }
            }
        },
        "pricing": {
            "status": "priced",
            "currency": "USD",
            "amount_micro": cost_micro,
            "priced_amount_micro": cost_micro,
            "price_book_id": "historical-fixture"
        }
    })
}

fn write_record(root: &Path, name: &str, record: &serde_json::Value) {
    let invocations = root.join("runtime/accounting/invocations");
    fs::create_dir_all(&invocations).expect("create invocation directory");
    fs::write(
        invocations.join(name),
        serde_json::to_vec_pretty(record).expect("serialize invocation"),
    )
    .expect("write invocation");
}

fn cost_payload(plan: &Path, machine: &Path) -> serde_json::Value {
    let cost = run_cli("cost", plan, machine, &["--json"]);
    assert_success(&cost);
    serde_json::from_str(&cost.stdout).expect("cost emits JSON")
}

/// The intake fixture, strengthened with measured usage and cost: two old v1
/// records share a visit-level id but name distinct runs and time intervals.
/// Both later startups must be admitted and both amounts must remain visible.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-panta.6.5
#[test]
fn attempt_identity_historical_cross_run_attempts_allow_later_startups_and_keep_cost() {
    let (dir, plan, machine) = setup_single_file("legacy-attempt-startup", PLAN);
    fs::write(&machine, PASSIVE_MACHINE).expect("write passive machine");
    write_record(
        &dir,
        "attempt-1.json",
        &historical_record(
            "retry-run-1",
            "2026-09-12T09:11:23Z",
            "2026-09-12T09:11:24Z",
            110,
            1_100,
        ),
    );
    write_record(
        &dir,
        "attempt-2.json",
        &historical_record(
            "retry-run-2",
            "2026-09-12T09:16:48Z",
            "2026-09-12T09:16:49Z",
            220,
            2_200,
        ),
    );

    for ordinal in ["first", "second"] {
        let run = run_cli(
            "run",
            &plan,
            &machine,
            &["--no-tui", "--no-callbacks", "--no-agent", "--no-program"],
        );
        assert_success(&run);
        assert!(
            !run.stderr.contains("different record is already read as invocation"),
            "{ordinal} startup treated separate attempts as one:\n{}",
            run.stderr
        );
    }

    let payload = cost_payload(&plan, &machine);
    assert_eq!(payload["selection"]["invocation_count"], 2);
    assert_eq!(payload["summary"]["total"]["value"], 330);
    assert_eq!(payload["summary"]["cost_micro"], 3_300);
    assert!(dir.join("runtime/accounting/invocations/attempt-1.json").exists());
    assert!(dir.join("runtime/accounting/invocations/attempt-2.json").exists());
}

fn write_retrying_codex(root: &Path) {
    let agent = write_python_agent(
        root,
        "retrying-codex.py",
        r#"import json
attempt = int(env('RHEI_ATTEMPT', '1'))
input_tokens = attempt * 1000
print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': input_tokens,
        'cached_input_tokens': 0,
        'cache_creation_input_tokens': 0,
        'output_tokens': attempt * 10,
    },
}), flush=True)
if attempt >= 2:
    result('## Result\n\nFinished on the retry.\n')
"#,
    );
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings directory");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "codex", "model": "luna", "agent_timeout": "10s" }},
  "agents": {{
    "codex": {{ "command": {}, "prompt_flag": "--prompt", "timeout": "10s" }}
  }},
  "models": {{
    "luna": {{ "provider": "openai", "model": "gpt-5.6-luna", "default_agent": "codex" }}
  }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");
}

fn durable_records(root: &Path) -> Vec<serde_json::Value> {
    let directory = root.join("runtime/accounting/invocations");
    let mut records = fs::read_dir(&directory)
        .expect("read invocations")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .map(|entry| {
            serde_json::from_str::<serde_json::Value>(
                &fs::read_to_string(entry.path()).expect("read record"),
            )
            .expect("parse record")
        })
        .collect::<Vec<_>>();
    records.sort_by_key(|record| record["tokens"]["total"]["value"].as_u64());
    records
}

fn assert_retry_identity(parallel: usize, task_count: usize) {
    let dir = unique_temp_dir(&format!("attempt-writer-{parallel}"));
    let workspace = dir.join("workspace");
    let tasks = workspace.join("tasks");
    fs::create_dir_all(&tasks).expect("create directory workspace");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Attempt Writer\n")
        .expect("write workspace index");
    for task in 1..=task_count {
        fs::write(
            tasks.join(format!("{task:02}-retry.md")),
            format!("### Task {task}: Retry once\n**State:** work\n"),
        )
        .expect("write task");
    }
    let machine = write_fixture_file(&dir, "states.yaml", RETRY_MACHINE);
    write_retrying_codex(&workspace);
    let parallel_arg = parallel.to_string();
    let args = ["--no-tui", "--no-callbacks", "--parallel", parallel_arg.as_str()];

    let first = run_cli("run", &workspace, &machine, &args);
    assert!(!first.status.success(), "the first attempt deliberately leaves its result missing");
    let second = run_cli("run", &workspace, &machine, &args);
    assert_success(&second);
    let later = run_cli("run", &workspace, &machine, &args);
    assert_success(&later);
    assert!(
        !later.stderr.contains("different record is already read as invocation"),
        "retained retries blocked a later startup:\n{}",
        later.stderr
    );

    let records = durable_records(&workspace);
    assert_eq!(records.len(), task_count * 2, "each task keeps both attempts");
    let ids = records
        .iter()
        .map(|record| record["invocation_id"].as_str().expect("string invocation id"))
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), records.len(), "every spawned attempt has a distinct identity");
    for id in ids {
        assert!(id.contains("::run-"), "attempt id names its run: {id}");
        assert!(id.contains("::move-0::attempt-"), "attempt id names its visit and attempt: {id}");
    }

    let payload = cost_payload(&workspace, &machine);
    assert_eq!(payload["selection"]["invocation_count"], (task_count * 2) as u64);
    assert_eq!(payload["summary"]["total"]["value"], (task_count as u64) * 3_030);
    let stored_cost: u64 = records
        .iter()
        .map(|record| record["pricing"]["amount_micro"].as_u64().expect("priced attempt"))
        .sum();
    assert_eq!(payload["summary"]["cost_micro"], stored_cost);
}

/// Sequential retries use the shared spawn plan's run/move/attempt identity.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.7.1
#[test]
fn attempt_identity_sequential_retry_writes_two_ids_and_allows_a_later_run() {
    assert_retry_identity(1, 1);
}

/// The worker pool has the same identity contract as the sequential path.
// §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.7.1
#[test]
fn attempt_identity_parallel_retries_write_distinct_ids_and_allow_a_later_run() {
    assert_retry_identity(2, 2);
}

/// A contradictory pair remains a refusal, but it is an identity failure and
/// not a price-book error. The first valid record and both paths stay visible.
// §FS-rhei-cost-accounting.11
#[test]
fn attempt_identity_conflict_is_reported_as_identity_not_currency() {
    let (dir, plan, machine) = setup_single_file("attempt-conflict", PLAN);
    fs::write(&machine, PASSIVE_MACHINE).expect("write passive machine");
    let first =
        historical_record("same-run", "2026-09-12T09:11:23Z", "2026-09-12T09:11:24Z", 110, 1_100);
    let mut second = first.clone();
    second["tokens"]["total"]["value"] = 999.into();
    write_record(&dir, "first.json", &first);
    write_record(&dir, "second.json", &second);

    let run = run_cli(
        "run",
        &plan,
        &machine,
        &["--no-tui", "--no-callbacks", "--no-agent", "--no-program"],
    );
    assert!(!run.status.success(), "contradictory identity evidence must refuse startup");
    assert!(run.stderr.contains("accounting identity"), "wrong diagnostic:\n{}", run.stderr);
    assert!(
        !run.stderr.contains("selected currency"),
        "identity was blamed on pricing:\n{}",
        run.stderr
    );
    assert!(run.stderr.contains("first.json") && run.stderr.contains("second.json"));
}
