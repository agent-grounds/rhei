//! The one-task priced workspace the price-book scenarios drive, and the
//! numbers they assert on.
//!
//! Both the `--prices` flag (`accounting_prices_tests`) and the
//! `defaults.prices` settings key (`accounting_settings_prices_tests`) select
//! the *same* caller-owned book, so they assert against one fixture rather
//! than two that could drift apart.
//! §FS-rhei-cost-accounting.5.1

use std::fs;
use std::path::{Path, PathBuf};

use super::{fixture_command, write_python_agent};

pub const PRICED_MACHINE: &str = r#"name: priced-run
version: 1
models: [luna]
states:
  work:
    initial: true
    description: Emit measured usage and finish
    agent: codex
    model: luna
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

pub const ONE_TASK_PLAN: &str = r#"# Rhei: Priced Run

## Tasks

### Task 1: Measure this invocation
**State:** work
"#;

pub fn price_book_json() -> serde_json::Value {
    serde_json::json!({
        "schema": "rhei.accounting.prices.v1",
        "price_book_id": "fixture-luna-2026-09-01",
        "currency": "CHF",
        "entries": [{
            "provider": "openai",
            "model": "gpt-5.6-luna",
            "effective_at": "2026-09-01T00:00:00Z",
            "unit": "1m_tokens",
            "input_total_micro": 2_000_000,
            "input_cached_read_micro": 250_000,
            "input_cache_write_micro": 4_000_000,
            "output_total_micro": 10_000_000
        }]
    })
}

pub fn write_price_book(dir: &Path) -> PathBuf {
    write_price_book_value(dir, "luna-prices.json", &price_book_json())
}

pub fn write_price_book_value(dir: &Path, name: &str, price_book: &serde_json::Value) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().expect("price book path has a parent"))
        .expect("create price book directory");
    fs::write(&path, serde_json::to_string_pretty(price_book).expect("serialize price book"))
        .expect("write price book");
    path
}

pub fn write_measured_codex_settings(root: &Path, spawned_marker: Option<&Path>) {
    write_measured_codex_settings_for_model(root, spawned_marker, "openai", "gpt-5.6-luna");
}

pub fn write_measured_codex_settings_for_model(
    root: &Path,
    spawned_marker: Option<&Path>,
    provider: &str,
    model: &str,
) {
    let marker = spawned_marker
        .map(|path| {
            format!(
                "write(pathlib.Path({}), 'spawned\\n')\n",
                serde_json::to_string(path.to_string_lossy().as_ref()).expect("encode marker")
            )
        })
        .unwrap_or_default();
    let script = write_python_agent(
        root,
        "measured-codex.py",
        &format!(
            r#"{marker}import json
import time

print(json.dumps({{
    'type': 'turn.completed',
    'usage': {{
        'input_tokens': 1250000,
        'cached_input_tokens': 500000,
        'cache_creation_input_tokens': 250000,
        'output_tokens': 750000,
    }},
}}), flush=True)
time.sleep(0.1)
result('## Result\n\nMeasured invocation completed.\n')
"#
        ),
    );
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "codex", "model": "luna", "agent_timeout": "10s" }},
  "agents": {{
    "codex": {{ "command": {}, "prompt_flag": "--prompt", "timeout": "10s" }}
  }},
  "models": {{
    "luna": {{ "provider": {provider:?}, "model": {model:?}, "default_agent": "codex" }}
  }}
}}"#,
            fixture_command(&script)
        ),
    )
    .expect("write agent settings");
}

pub fn invocation_json(root: &Path) -> serde_json::Value {
    let directory = root.join("runtime/accounting/invocations");
    let mut records: Vec<PathBuf> = fs::read_dir(&directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    records.sort();
    assert_eq!(records.len(), 1, "expected one invocation under {}", root.display());
    serde_json::from_str(&fs::read_to_string(&records[0]).expect("read invocation"))
        .expect("parse invocation")
}

pub fn assert_selected_pricing(root: &Path) {
    assert_selected_pricing_from(root, "fixture-luna-2026-09-01");
}

/// The same assertions with the book's id named, for a scenario whose point is
/// *which* of two selected books priced the run. §FS-rhei-cost-accounting.5.1
pub fn assert_selected_pricing_from(root: &Path, price_book_id: &str) {
    let invocation = invocation_json(root);
    assert_eq!(invocation["provider"], "openai");
    assert_eq!(invocation["model"], "gpt-5.6-luna");
    assert_eq!(invocation["tokens"]["input"]["total"]["value"], 1_250_000);
    assert_eq!(invocation["tokens"]["input"]["cached_read"]["value"], 500_000);
    assert_eq!(invocation["tokens"]["input"]["cache_write"]["value"], 250_000);
    assert_eq!(invocation["tokens"]["output"]["total"]["value"], 750_000);
    assert_eq!(invocation["pricing"]["status"], "priced");
    assert_eq!(invocation["pricing"]["currency"], "CHF");
    assert_eq!(invocation["pricing"]["price_book_id"], price_book_id);
    // 1,250,000 input of which 500,000 cached and 250,000 written: 500,000
    // fresh at 2 CHF/M, the cache dimensions once each, 750,000 output at
    // 10 CHF/M. Charging all 1,250,000 at the full rate gave 11,125,000.

    // §FS-rhei-cost-accounting.5
    assert_eq!(invocation["pricing"]["amount_micro"], 9_625_000);
    assert_eq!(invocation["pricing"]["priced_amount_micro"], 9_625_000);
}

pub fn assert_selected_book_copy(root: &Path, expected: &serde_json::Value) {
    let copied: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("runtime/accounting/prices.json"))
            .unwrap_or_else(|err| panic!("read copied book under {}: {err}", root.display())),
    )
    .expect("parse copied price book");
    assert_eq!(&copied, expected);
}
