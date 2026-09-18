//! Durable accounting fixtures for completed-run summary repricing.
//! §FS-rhei-summary.1 §FS-rhei-cost-accounting.5.1

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{unique_temp_dir, write_fixture_file, TestDir, STATE_MACHINE};

pub const SELECTED_RUN: &str = "abc123";
pub const OTHER_RUN: &str = "def456";

const PLAN: &str = r#"# Rhei: Reprice History
**States:** integration-test

## Tasks

### Task 1: Selected work
**State:** completed

### Task 2: Other work
**State:** cancelled

### Task 3: Unrelated current work
**State:** draft
"#;

#[derive(Clone, Copy)]
pub struct RecordedPrice<'a> {
    pub status: &'a str,
    pub currency: &'a str,
    pub amount_micro: Option<u64>,
    pub book_id: &'a str,
}

pub const OLD_PRICE: RecordedPrice<'static> = RecordedPrice {
    status: "priced",
    currency: "CHF",
    amount_micro: Some(9_625_000),
    book_id: "fixture-old-2026-08-01",
};

pub const UNPRICED: RecordedPrice<'static> = RecordedPrice {
    status: "unpriced",
    currency: "CHF",
    amount_micro: None,
    book_id: "fixture-old-2026-08-01",
};

pub struct RepriceFixture {
    pub _dir: TestDir,
    pub root: PathBuf,
    pub plan: PathBuf,
    pub machine: PathBuf,
    pub later_book: PathBuf,
}

impl RepriceFixture {
    pub fn new(prefix: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let root = dir.to_path_buf();
        let plan = write_fixture_file(&root, "plan.rhei.md", PLAN);
        let machine = write_fixture_file(&root, "states.yaml", STATE_MACHINE);
        let later_book = write_book(
            &root,
            "later-prices.json",
            "fixture-later-2026-09-01",
            "CHF",
            &[rate("openai", "gpt-5.6-luna", 4_000_000, 500_000, 8_000_000, 20_000_000)],
        );
        Self { _dir: dir, root, plan, machine, later_book }
    }

    pub fn seed_changed_rate_run(&self) {
        write_record(
            &self.root,
            "selected-invocation",
            SELECTED_RUN,
            "plan.1",
            "completed",
            "openai",
            "gpt-5.6-luna",
            "input-total-includes-cache",
            (1_250_000, 500_000, 250_000, 750_000),
            OLD_PRICE,
        );
        write_record(
            &self.root,
            "other-invocation",
            OTHER_RUN,
            "plan.2",
            "cancelled",
            "openai",
            "gpt-5.6-luna",
            "input-total-includes-cache",
            (50, 0, 0, 50),
            OLD_PRICE,
        );
        write_report(&self.root, SELECTED_RUN, "2026-09-01T10:00:00Z", "completed");
        write_report(&self.root, OTHER_RUN, "2026-09-01T11:00:00Z", "completed");
        write_stored_book(&self.root);
    }
}

pub fn rate(
    provider: &str,
    model: &str,
    input: u64,
    cached_read: u64,
    cache_write: u64,
    output: u64,
) -> serde_json::Value {
    serde_json::json!({
        "provider": provider,
        "model": model,
        "effective_at": "2026-09-01T00:00:00Z",
        "unit": "1m_tokens",
        "input_total_micro": input,
        "input_cached_read_micro": cached_read,
        "input_cache_write_micro": cache_write,
        "output_total_micro": output
    })
}

pub fn write_book(
    root: &Path,
    name: &str,
    id: &str,
    currency: &str,
    entries: &[serde_json::Value],
) -> PathBuf {
    let path = root.join(name);
    let value = serde_json::json!({
        "schema": "rhei.accounting.prices.v1",
        "price_book_id": id,
        "currency": currency,
        "entries": entries
    });
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("serialize book"))
        .expect("write book");
    path
}

pub fn write_stored_book(root: &Path) {
    let accounting = root.join("runtime/accounting");
    fs::create_dir_all(&accounting).expect("create accounting root");
    let old = serde_json::json!({
        "schema": "rhei.accounting.prices.v1",
        "price_book_id": "fixture-old-2026-08-01",
        "currency": "CHF",
        "entries": [rate(
            "openai", "gpt-5.6-luna", 2_000_000, 250_000, 4_000_000, 10_000_000
        )]
    });
    fs::write(
        accounting.join("prices.json"),
        serde_json::to_vec_pretty(&old).expect("serialize stored book"),
    )
    .expect("write stored book");
}

#[allow(clippy::too_many_arguments)]
pub fn write_record(
    root: &Path,
    invocation_id: &str,
    run_id: &str,
    task_id: &str,
    state: &str,
    provider: &str,
    model: &str,
    convention: &str,
    dimensions: (u64, u64, u64, u64),
    recorded: RecordedPrice<'_>,
) {
    let (input, cached_read, cache_write, output) = dimensions;
    let mut pricing = serde_json::json!({
        "status": recorded.status,
        "currency": recorded.currency,
        "price_book_id": recorded.book_id
    });
    if let Some(amount) = recorded.amount_micro {
        pricing["amount_micro"] = amount.into();
        pricing["priced_amount_micro"] = amount.into();
    }
    let record = serde_json::json!({
        "schema": "rhei.accounting.invocation.v1",
        "invocation_id": invocation_id,
        "run_id": run_id,
        "task_id": task_id,
        "state": state,
        "visit": 1,
        "agent": "codex",
        "provider": provider,
        "model": model,
        "started_at": "2026-09-01T10:00:00Z",
        "ended_at": "2026-09-01T10:05:00Z",
        "duration_ms": 300_000,
        "extraction_status": "measured",
        "scope": "aggregate-agent-process",
        "token_convention": convention,
        "tokens": {
            "total": measured(input + output),
            "input": {
                "total": measured(input),
                "cached_read": measured(cached_read),
                "cache_write": measured(cache_write)
            },
            "output": {
                "total": measured(output),
                "cached_read": unavailable(),
                "cache_write": unavailable()
            }
        },
        "pricing": pricing
    });
    let invocations = root.join("runtime/accounting/invocations");
    fs::create_dir_all(&invocations).expect("create invocation directory");
    fs::write(
        invocations.join(format!("{invocation_id}.json")),
        serde_json::to_vec_pretty(&record).expect("serialize record"),
    )
    .expect("write invocation record");
}

pub fn write_unmeasured_record(root: &Path, run_id: &str) {
    let record = serde_json::json!({
        "schema": "rhei.accounting.invocation.v1",
        "invocation_id": "unmeasured-invocation",
        "run_id": run_id,
        "task_id": "plan.1",
        "state": "completed",
        "visit": 1,
        "agent": "codex",
        "provider": "openai",
        "model": "gpt-5.6-luna",
        "started_at": "2026-09-01T10:00:00Z",
        "ended_at": "2026-09-01T10:05:00Z",
        "extraction_status": "no-usage-emitted",
        "scope": "aggregate-agent-process",
        "token_convention": "input-total-includes-cache",
        "tokens": {
            "total": {"status": "unknown"},
            "input": {
                "total": {"status": "unknown"},
                "cached_read": unavailable(),
                "cache_write": unavailable()
            },
            "output": {
                "total": {"status": "unknown"},
                "cached_read": unavailable(),
                "cache_write": unavailable()
            }
        },
        "pricing": {"status": "not-applicable"}
    });
    let invocations = root.join("runtime/accounting/invocations");
    fs::create_dir_all(&invocations).expect("create invocation directory");
    fs::write(
        invocations.join("unmeasured-invocation.json"),
        serde_json::to_vec_pretty(&record).expect("serialize unmeasured record"),
    )
    .expect("write unmeasured record");
}

fn measured(value: u64) -> serde_json::Value {
    serde_json::json!({ "value": value, "source": "agent-usage-capture" })
}

fn unavailable() -> serde_json::Value {
    serde_json::json!({ "status": "unsupported" })
}

pub fn write_report(root: &Path, run_id: &str, started_at: &str, result: &str) -> PathBuf {
    let reports = root.join("runtime/run-reports");
    fs::create_dir_all(&reports).expect("create report history");
    let stamp = started_at.replace(':', "-");
    let path = reports.join(format!("{stamp}-{run_id}.md"));
    let body = format!(
        "# Run Report: Reprice History\n\nRun: {started_at} / {run_id}\n\
         Workspace: .\nCommand: rhei run\nMode: foreground · parallel 1\nResult: {result}\n"
    );
    fs::write(&path, body).expect("write immutable run report");
    path
}

pub fn write_running_descriptor(root: &Path, run_id: &str) {
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).expect("create runtime");
    let descriptor = serde_json::json!({
        "id": run_id,
        "pid": std::process::id(),
        "status": "running",
        "workspace": root,
        "plan": root.join("plan.rhei.md"),
        "started_at": "2026-09-01T10:00:00Z",
        "headless": false,
        "parallel": 1,
        "events": root.join("runtime/events.jsonl"),
        "exit_code": null
    });
    fs::write(
        runtime.join("run.json"),
        serde_json::to_vec_pretty(&descriptor).expect("serialize descriptor"),
    )
    .expect("write descriptor");
}

pub fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(at) else { return };
        for entry in entries.map_while(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                visit(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).expect("path below root").to_path_buf(),
                    fs::read(path).expect("read snapshot file"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
