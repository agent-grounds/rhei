//! The fixture world of a two-arm fanout whose arms the account has to tell
//! apart: two agent profiles under the families that write accounting records,
//! a price book that prices each at a different amount, and a fake agent that
//! orders the two arms' exits by a handshake rather than by sleeping.
//!
//! The handshake is what makes these scenarios deterministic under load. The
//! arm named slow waits until the account holds a `spend` receipt — the fast
//! arm's exit has then been handled through its settle — copies the journal as
//! it stands, and only then exits. So every scenario reads two things: the
//! journal at the end of the run, and the journal as it stood while the slow
//! arm was still running.

// §FS-rhei-budgets.6.2.1

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::budget_edge_support::account_journal;
use super::budget_support::*;
use super::*;

/// The codex arm: 100k input and 100k output tokens at $2.00/$10.00 per 1M.
pub(super) const CODEX: &str = "cdx[xhigh]:openai:gpt-6-astra";
pub(super) const CODEX_COST: u64 = 1_200_000;
/// The claude arm: the same tokens at $5.00/$25.00 per 1M.
pub(super) const CLAUDE: &str = "cld[yolo]:anthropic:claude-opus-5-5";
pub(super) const CLAUDE_COST: u64 = 3_000_000;
/// An arm whose profile declares no family, so it has no accounting extractor
/// and its invocation writes **no record at all**. §FS-rhei-cost-accounting.3.2
pub(super) const UNRECORDED: &str = "plain[yolo]:anthropic:claude-opus-5-5";
/// An arm whose command does not exist, so its spawn fails after admission has
/// recorded its start: an arm that ends with a start and no record.
pub(super) const BROKEN: &str = "broken[yolo]:anthropic:claude-opus-5-5";
/// The flat worst case every neural start reserves. §FS-rhei-budgets.6.2
pub(super) const RESERVE: u64 = 20_000_000;

const PRICES: &str = r#"{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "fan-out-fixture",
  "currency": "USD",
  "entries": [
    { "provider": "anthropic", "model": "claude-opus-5-5", "effective_at": "2026-09-02T00:00:00Z",
      "unit": "1m_tokens", "input_total_micro": 5000000, "input_cached_read_micro": 500000,
      "input_cache_write_micro": 6250000, "output_total_micro": 25000000 },
    { "provider": "openai", "model": "gpt-6-astra", "effective_at": "2026-09-02T00:00:00Z",
      "unit": "1m_tokens", "input_total_micro": 2000000, "input_cached_read_micro": 200000,
      "input_cache_write_micro": 2000000, "output_total_micro": 10000000 }
  ]
}
"#;

const PLAN: &str = r#"# Rhei: Fan-out spend

## Tasks

### Task 1: Review
**State:** review

Both reviewers write their own review.
"#;

/// Stands in for every arm and spends no tokens. Usage goes through the capture
/// contract, which only a profile with a family is handed; the slow arm waits
/// for the first `spend` receipt (30 s at most) before it exits.
/// §FS-rhei-cost-accounting.4
const AGENT: &str = r#"import glob
import json

slug = env('RHEI_TARGET_SLUG')
arm = slug.split('-', 1)[0]
root = pathlib.Path(env('RHEI_ROOT'))
slow = (root / 'slow-arm').read_text(encoding='utf-8').strip()
journals = str(root / '.agent-grounds' / 'rhei' / 'budgets' / '*' / 'journal.jsonl')


def receipts():
    lines = []
    for path in glob.glob(journals):
        try:
            with open(path, encoding='utf-8', newline='') as handle:
                lines.extend(line for line in handle if line.endswith('\n'))
        except OSError:
            pass  # read again on the next poll
    return lines


handshake = 'none'
if arm == slow:
    handshake = 'timed-out'
    deadline = time.time() + 30
    while time.time() < deadline:
        if any('"kind":"spend"' in line for line in receipts()):
            handshake = 'saw-spend'
            break
        time.sleep(0.05)
    write(root / 'runtime' / 'fan-out' / 'journal-at-slow-exit.jsonl', ''.join(receipts()))
capture = env('RHEI_ACCOUNTING_USAGE_PATH')
if capture:
    usage = {'input_tokens': 100000, 'output_tokens': 100000, 'total_tokens': 200000}
    append(capture, json.dumps({'schema': 'rhei.accounting.usage.v1', 'usage': usage}) + '\n')
write(root / 'out' / (slug + '.md'), 'review by {}\n'.format(slug))
report = {
    'arm': arm,
    'handshake': handshake,
    'parent_reservation': os.environ.get('RHEI_BUDGET_PARENT_RESERVATION', ''),
}
append(root / 'runtime' / 'fan-out' / 'arms.jsonl', json.dumps(report) + '\n')
result('Reviewed by {}.\n'.format(slug))
"#;

/// A plan with one ticket whose state fans out over `targets`, in that order,
/// and whose arm named `slow` (the agent id before `[`) exits last.
pub(super) struct FanOut {
    pub dir: TestDir,
    pub plan: PathBuf,
    pub machine: PathBuf,
    prices: PathBuf,
    targets: [&'static str; 2],
}

pub(super) fn fan_out(prefix: &str, targets: [&'static str; 2], slow: &str) -> FanOut {
    fan_out_world(prefix, targets, slow, false)
}

/// [`fan_out`] whose state requires an MCP server that cannot start, so every
/// arm is admitted and then stopped at the tooling gate before any start is
/// recorded, and the state's `mcp_unavailable` edge moves the ticket to
/// `blocked`. §FS-rhei-agents.6.1
pub(super) fn fan_out_without_tooling(prefix: &str, targets: [&'static str; 2]) -> FanOut {
    fan_out_world(prefix, targets, "none", true)
}

fn fan_out_world(prefix: &str, targets: [&'static str; 2], slow: &str, gated: bool) -> FanOut {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", &machine(targets, gated));
    let prices = write_fixture_file(&dir, "prices.json", PRICES);
    write_fixture_file(&dir, "slow-arm", slow);
    let command = fixture_command(&write_python_agent(&dir, "fan-out-agent.py", AGENT));
    // Inside the fixture, so it is missing on every platform.
    let missing = serde_json::json!([dir.join("no-such-agent").display().to_string()]);
    let servers = if gated {
        r#""mcp_servers": { "lookup": { "command": ["rhei-e2e-no-such-mcp-server"] } },"#
    } else {
        ""
    };
    write_machine_settings(
        &dir,
        &format!(
            r#"{{
  "defaults": {{ "agent_timeout": "60s" }},{servers}
  "agents": {{
    "cdx": {{ "command": {command}, "stdin_prompt": true, "intervene_stdin": false,
             "family": "codex", "modes": {{ "xhigh": [] }} }},
    "cld": {{ "command": {command}, "stdin_prompt": true, "intervene_stdin": false,
             "family": "claude-code", "modes": {{ "yolo": [] }} }},
    "plain": {{ "command": {command}, "stdin_prompt": true, "intervene_stdin": false,
               "modes": {{ "yolo": [] }} }},
    "broken": {{ "command": {missing}, "stdin_prompt": true, "intervene_stdin": false,
                "modes": {{ "yolo": [] }} }}
  }}
}}"#
        ),
    );
    FanOut { dir, plan, machine, prices, targets }
}

fn machine(targets: [&str; 2], gated: bool) -> String {
    let (servers, blocked, route) = if gated {
        (
            "\n    mcp_servers: [lookup]",
            "\n  blocked:\n    gating: true",
            "\n  - { from: review, to: blocked, mcp_unavailable: [lookup] }\n  - { from: blocked, to: review }",
        )
    } else {
        ("", "", "")
    };
    format!(
        r#"name: budget-fan-out
version: 1
states:
  review:
    initial: true
    description: Review with both arms
    all_targets:
      - {}
      - {}
    agent_timeout: 60s{servers}
    outputs:
      - name: review
        path: out/{{target.slug}}.md
  done:
    description: Both reviews are in
    final: true{blocked}
transitions:
  - {{ from: review, to: done, description: Reviewed }}{route}
"#,
        targets[0], targets[1]
    )
}

impl FanOut {
    /// `rhei run` over the plan, with `--parallel 2` as the report ran it or with
    /// no `--parallel` at all, which still starts a fanout's arms together.
    pub fn run(&self, parallel: bool) -> CliRun {
        let prices = self.prices.display().to_string();
        let mut args = vec!["--no-tui", "--no-callbacks", "--prices", prices.as_str()];
        if parallel {
            args.extend(["--parallel", "2"]);
        }
        let result = run_at("run", &self.plan, &self.machine, Some(SPEND_DAY), &args);
        assert_success(&result);
        result
    }

    /// `rhei run --parallel 2 --continue-on-error`, the run a failing arm needs
    /// so that its sibling still runs; the run may end non-zero.
    pub fn run_continuing_on_error(&self) -> CliRun {
        let prices = self.prices.display().to_string();
        let args = [
            "--no-tui",
            "--no-callbacks",
            "--prices",
            &prices,
            "--parallel",
            "2",
            "--continue-on-error",
        ];
        run_at("run", &self.plan, &self.machine, Some(SPEND_DAY), &args)
    }

    /// The reservation each arm was admitted on, by agent id. Admission follows
    /// `all_targets`, so the reserve receipts arrive in that order.
    pub fn reservations(&self) -> BTreeMap<String, String> {
        let admitted: Vec<String> = account_journal(&self.dir)
            .iter()
            .filter(|receipt| receipt["kind"] == "reserve")
            .flat_map(|receipt| {
                receipt["payload"]["reservations"].as_array().cloned().unwrap_or_default()
            })
            .map(|row| row["reservation_id"].as_str().expect("a reservation id").to_owned())
            .collect();
        assert_eq!(admitted.len(), 2, "one reservation per arm; got {admitted:?}");
        self.targets.iter().map(|target| arm_of(target)).zip(admitted).collect()
    }

    /// The journal as it stood while the slow arm was still running, after it
    /// saw the first `spend` receipt.
    pub fn journal_at_slow_exit(&self) -> Vec<serde_json::Value> {
        let path = self.dir.join("runtime/fan-out/journal-at-slow-exit.jsonl");
        receipts_in(&fs::read_to_string(path).expect("the slow arm copied the journal"))
    }

    /// What each arm reported about itself, by agent id.
    pub fn arm_reports(&self) -> BTreeMap<String, serde_json::Value> {
        receipts_in(
            &fs::read_to_string(self.dir.join("runtime/fan-out/arms.jsonl")).unwrap_or_default(),
        )
        .into_iter()
        .map(|report| (report["arm"].as_str().expect("an arm").to_owned(), report))
        .collect()
    }

    /// The premise every scenario stands on: each arm with a family wrote a
    /// measured, priced record at its own amount, an arm without one wrote none,
    /// and the slow arm saw the fast arm's settle before it exited.
    pub fn assert_premise(&self, slow: &str) {
        let records = accounting_records(&self.dir);
        for target in self.targets {
            let arm = arm_of(target);
            match (target, records.get(&arm)) {
                (UNRECORDED, record) => {
                    assert!(record.is_none(), "{arm} writes no record; got {record:?}")
                }
                (_, Some(record)) => {
                    let cost = if target == CODEX { CODEX_COST } else { CLAUDE_COST };
                    assert_eq!(
                        (
                            &record["extraction_status"],
                            &record["pricing"]["status"],
                            &record["pricing"]["amount_micro"]
                        ),
                        (&"measured".into(), &"priced".into(), &cost.into()),
                        "the premise is that the {arm} arm is measured and priced; got {record}"
                    );
                }
                (_, None) => panic!("the {arm} arm wrote no accounting record; got {records:?}"),
            }
        }
        let reports = self.arm_reports();
        assert_eq!(
            reports.get(slow).map(|report| report["handshake"].clone()),
            Some("saw-spend".into()),
            "the premise is that the slow arm outlived the fast arm's settle; got {reports:?}"
        );
    }
}

/// The agent id a target names, which is what the fake agent calls its arm.
fn arm_of(target: &str) -> String {
    target.split(['[', ':']).next().expect("a target names its agent").to_owned()
}

fn receipts_in(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("one JSON value per line"))
        .collect()
}

/// Every accounting record the run wrote, by agent id.
fn accounting_records(root: &Path) -> BTreeMap<String, serde_json::Value> {
    let dir = root.join("runtime/accounting/invocations");
    let Ok(entries) = fs::read_dir(dir) else { return BTreeMap::new() };
    entries
        .map(|entry| entry.expect("an invocation record").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| {
            let record: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(path).expect("read a record"))
                    .expect("a record");
            (record["agent"].as_str().expect("a record names its agent").to_owned(), record)
        })
        .collect()
}

/// The `(amount, basis)` of every `spend` receipt on one reservation.
pub(super) fn spends_on(receipts: &[serde_json::Value], reservation: &str) -> Vec<(u64, String)> {
    receipts
        .iter()
        .filter(|receipt| {
            receipt["kind"] == "spend" && receipt["payload"]["reservation_id"] == reservation
        })
        .map(|receipt| {
            let payload = &receipt["payload"];
            (
                payload["amount_micro"].as_u64().expect("an amount"),
                payload["basis"].as_str().expect("a basis").into(),
            )
        })
        .collect()
}

/// Every receipt of `kind` whose payload names `reservation`.
pub(super) fn receipts_on<'a>(
    receipts: &'a [serde_json::Value],
    kind: &str,
    reservation: &str,
) -> Vec<&'a serde_json::Value> {
    receipts
        .iter()
        .filter(|receipt| {
            receipt["kind"] == kind && receipt["payload"]["reservation_id"] == reservation
        })
        .collect()
}

/// `(amount, "measured")`, the one shape a priced arm's settle should have.
pub(super) fn measured(amount: u64) -> Vec<(u64, String)> {
    vec![(amount, "measured".to_owned())]
}
