//! A count of unpriced invocations names what it counts and the book that
//! could not price it, on every surface that reports the count.
//!
//! The case is a machine whose price book predates its models: the book lists
//! `anthropic/claude-opus-5` and `anthropic/claude-sonnet-5`, the states target
//! the `-5-5` ids, and every invocation is measured, none is priced, and each is
//! charged the $20.00 reserve until the day's ceiling halts the run. Price
//! entries match exactly, so the pair and the book are the fix, and a surface
//! that says only "20 unpriced" leaves the reader to find them by hand.
//!
//! The fake agent reports usage through the capture contract, so no model is
//! ever called; HOME, the state directory and the project are the test's own.
//! §REQ-test-isolation.5 §REQ-test-isolation.6 §REQ-cross-platform.4

// §FS-rhei-budgets.8.1 §FS-rhei-budgets.5.2 §FS-rhei-budgets.10
// §FS-rhei-validate.4.6 §FS-rhei-run-json.2.1 §FS-rhei-run-report.3.1

use super::budget_support::*;
use super::*;

const OPUS: &str = "claude-opus-5-5";
const SONNET: &str = "claude-sonnet-5-5";
const BOOK_ID: &str = "foundation-site-2026-09-02";

/// A book written before the models the machine now targets: an entry for each
/// predecessor and none for either target. §FS-rhei-cost-accounting.5.1
const STALE_BOOK: &str = r#"{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "foundation-site-2026-09-02",
  "currency": "USD",
  "entries": [
    { "provider": "anthropic", "model": "claude-opus-5",
      "effective_at": "2026-09-02T00:00:00Z", "unit": "1m_tokens",
      "input_total_micro": 15000000, "input_cached_read_micro": 1500000,
      "input_cache_write_micro": 18750000, "output_total_micro": 75000000 },
    { "provider": "anthropic", "model": "claude-sonnet-5",
      "effective_at": "2026-09-02T00:00:00Z", "unit": "1m_tokens",
      "input_total_micro": 3000000, "input_cached_read_micro": 300000,
      "input_cache_write_micro": 3750000, "output_total_micro": 15000000 }
  ]
}"#;

/// The issue's ping-pong: `work` on one model, `review` on the other, so the
/// unpriced count splits evenly between two pairs.
const TWO_MODEL_PING_PONG: &str = r#"name: unpriced-ping-pong
version: 1
states:
  work:
    initial: true
    description: Do a round of work
    target: claude-code[yolo]:anthropic:claude-opus-5-5
    agent_timeout: 60s
    outputs:
      - name: work
        path: runtime/work.md
  review:
    description: Send it back for another round
    target: claude-code[yolo]:anthropic:claude-sonnet-5-5
    agent_timeout: 60s
    outputs:
      - name: review
        path: runtime/review.md
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: review, description: Round done }
  - { from: review, to: work, description: Another round }
  - { from: work, to: cancelled, description: Stop }
  - { from: review, to: cancelled, description: Stop }
"#;

/// A ceiling four reserves fill: `$60.00 + $20.00` is admitted and `$80.00 +
/// $20.00` is not, so a run spends two invocations on each model and halts.
const FOUR_SPAWNS: &str = r#", "spend_per_day": 80.00"#;

/// A ceiling below one reserve, so the first admission is refused and nothing
/// is ever charged. §FS-rhei-budgets.6.2
const NO_SPAWN: &str = r#", "spend_per_day": 10.00"#;

/// The workspace: the stale book, named by `defaults.prices` in the machine
/// settings, and a `claude-code` profile whose fake reports measured usage.
fn setup_unpriced(prefix: &str, extra: &str) -> (TestDir, PathBuf, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", TWO_MODEL_PING_PONG);
    let prices = write_fixture_file(&dir, "prices.json", STALE_BOOK);
    let agent = write_python_agent(&dir, "fake-claude.py", PRICED_AGENT);
    let prices_json = serde_json::to_string(&prices.to_string_lossy()).expect("path as JSON");
    write_machine_settings(
        &dir,
        &format!(
            r#"{{
  "defaults": {{ "agent_timeout": "60s", "prices": {prices_json}{extra} }},
  "agents": {{
    "claude-code": {{
      "command": {command},
      "timeout": "60s",
      "stdin_prompt": true,
      "intervene_stdin": false,
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "{OPUS}": {{ "provider": "anthropic", "model": "{OPUS}", "default_agent": "claude-code" }},
    "{SONNET}": {{ "provider": "anthropic", "model": "{SONNET}", "default_agent": "claude-code" }}
  }}
}}"#,
            command = fixture_command(&agent)
        ),
    );
    (dir, plan, machine, prices)
}

/// `rhei run` as the issue ran it: the book also passed as `--prices`.
fn run_with_book(plan: &Path, machine: &Path, prices: &Path, json: bool) -> CliRun {
    let prices = prices.to_string_lossy().into_owned();
    let mut args = vec!["--no-tui", "--no-callbacks", "--prices", prices.as_str()];
    if json {
        args.push("--json");
    }
    run_at("run", plan, machine, Some(SPEND_DAY), &args)
}

/// The continuation the halt and `budget show` print for the two pairs, up to
/// the book's location, which is the path the book was read from.
fn named(each: usize) -> String {
    format!(
        "anthropic/{OPUS} ({each}), anthropic/{SONNET} ({each}) have no entry in price book \
         {BOOK_ID} ("
    )
}

/// The line directly after `first` in `text`, if `first` is there at all.
fn line_after<'a>(text: &'a str, first: &str) -> Option<&'a str> {
    let mut lines = text.lines();
    lines.by_ref().find(|line| line.contains(first))?;
    lines.next()
}

/// Assert the continuation line: aligned under the value column at `indent`,
/// naming both pairs, and ending in the book's file.
fn assert_continuation(text: &str, first: &str, indent: usize, each: usize, surface: &str) {
    let next = line_after(text, first)
        .unwrap_or_else(|| panic!("{surface} printed no {first:?} line; got:\n{text}"));
    let expected = format!("{}{}", " ".repeat(indent), named(each));
    assert!(
        next.starts_with(&expected) && next.ends_with("prices.json)"),
        "{surface}: beneath {first:?} comes one line, aligned under the value column, that \
         names each unpriced pair with its count and the book that has no entry for it \
         (§FS-rhei-budgets.8.1);\nexpected a line starting {expected:?} and ending \
         \"prices.json)\"\ngot {next:?} in:\n{text}"
    );
}

fn records(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("every --json stdout line is a record"))
        .collect()
}

/// Assert one `unpriced_details` array: an entry per pair, in order, each the
/// six context members plus `count`, and no digest. §FS-rhei-budgets.10
fn assert_details(details: &serde_json::Value, each: u64, surface: &str) {
    let entries = details.as_array().unwrap_or_else(|| {
        panic!("{surface}: `unpriced_details` is an array on the spend entry; got {details}")
    });
    let models: Vec<&str> = entries.iter().filter_map(|e| e["model"].as_str()).collect();
    assert_eq!(models, [OPUS, SONNET], "{surface}: one entry per pair, in order: {details}");
    for entry in entries {
        assert_eq!(entry["provider"], "anthropic", "{surface}: {entry}");
        assert_eq!(entry["price_book_id"], BOOK_ID, "{surface}: {entry}");
        assert_eq!(entry["price_book_source"], "file", "{surface}: {entry}");
        assert_eq!(entry["missing_entry"], true, "{surface}: {entry}");
        assert_eq!(entry["count"], each, "{surface}: {entry}");
        assert!(
            entry["price_book_path"].as_str().is_some_and(|p| p.ends_with("prices.json")),
            "{surface}: the path the book was read from: {entry}"
        );
        assert!(entry.get("price_book_sha256").is_none(), "{surface}: no digest: {entry}");
    }
}

/// The issue's reproducer, whole: the built-in $400.00 ceiling, twenty
/// invocations at the $20.00 reserve, ten on each model, and a halt whose
/// `estimated:` row says which pairs and which book.
// §FS-rhei-budgets.8.1 §REQ-bounded-neural-work.5
#[test]
#[ignore = "red until #515: the spend halt names the unpriced pairs and the book; implement removes this"]
fn a_spend_halt_on_unpriced_models_names_them_and_the_book_that_lacks_them() {
    let (dir, plan, machine, prices) = setup_unpriced("unpriced-naming-halt", "");

    let result = run_with_book(&plan, &machine, &prices, false);

    assert_spawn_count(&dir, 20, "twenty $20.00 reserves fill the built-in $400.00 ceiling");
    assert!(!result.status.success(), "a run halted on spend exits non-zero");
    assert_halt_mentions(&result, "bound:       $400.00 (built_in)");
    assert_halt_mentions(&result, "renews at:   2026-09-27T00:00:00Z");
    let first = "       estimated:   20 unpriced (charged at $20.00 each)";
    assert_continuation(&result.stderr, first, 20, 10, "the spend halt");
    assert!(
        !result.stderr.contains("pricing:") && !result.stderr.contains("to price it:"),
        "the halt keeps exactly one remedy, the renewal instant; got:\n{}",
        result.stderr
    );
}

/// `rhei budget show` names them beneath its `estimated:` line, in the halt's
/// words, aligned under that line's value column. §FS-rhei-budgets.10
#[test]
#[ignore = "red until #515: budget show names the unpriced pairs and the book; implement removes this"]
fn budget_show_names_the_unpriced_models_beneath_its_estimated_line() {
    let (_dir, plan, machine, prices) = setup_unpriced("unpriced-naming-show", FOUR_SPAWNS);
    run_with_book(&plan, &machine, &prices, false);

    let shown = budget_show(&plan, Some(SPEND_DAY));

    assert_success(&shown);
    let first = "  estimated:   4 unpriced (charged at $20.00 each)";
    assert_continuation(&shown.stdout, first, 15, 2, "rhei budget show");
}

/// `rhei budget show --format json` carries the same groups as
/// `spend.unpriced_details`, summing to `unpriced`. §FS-rhei-budgets.10
#[test]
#[ignore = "red until #515: budget show JSON carries spend.unpriced_details; implement removes this"]
fn budget_show_json_carries_the_unpriced_details() {
    let (dir, plan, machine, prices) = setup_unpriced("unpriced-naming-show-json", FOUR_SPAWNS);
    run_with_book(&plan, &machine, &prices, false);

    let mut cmd = rhei_command(home_for(&dir));
    cmd.arg("budget").arg("show").arg(&plan).arg("--format").arg("json");
    cmd.env("RHEI_BUDGET_NOW", SPEND_DAY);
    let shown = CliRun::from(&cmd.output().expect("rhei budget show should run"));

    assert_success(&shown);
    let report: serde_json::Value = serde_json::from_str(&shown.stdout).expect("one JSON report");
    assert_eq!(report["spend"]["unpriced"], 4, "the premise: four unpriced: {report}");
    assert_details(&report["spend"]["unpriced_details"], 2, "budget show --format json");
}

/// The run report's Bounds cell names them in one line, because a table cell
/// cannot hold the halt's continuation lines. §FS-rhei-run-report.3.1
#[test]
#[ignore = "red until #515: the run report's Bounds cell names the unpriced pairs; implement removes this"]
fn the_run_reports_bounds_cell_names_the_unpriced_models_in_one_line() {
    let (dir, plan, machine, prices) = setup_unpriced("unpriced-naming-report", FOUR_SPAWNS);
    run_with_book(&plan, &machine, &prices, false);

    let report =
        fs::read_to_string(dir.join("runtime/run-report.md")).expect("read the run report");
    let row = report
        .lines()
        .find(|line| line.starts_with("| project spend |"))
        .unwrap_or_else(|| panic!("the report has no project spend row:\n{report}"));
    let expected = format!("$80.00 (4 unpriced (charged at $20.00 each): {}", named(2));
    assert!(
        row.contains(&expected) && row.contains("prices.json))"),
        "the consumed-at-end cell is the marks row, a colon, and each book group, all in \
         one parenthesis;\nexpected it to contain {expected:?} … \"prices.json))\"\ngot:\n{row}"
    );
}

/// The JSON stream carries the groups on a spend `budget_halt` and on the spend
/// entry of `budget_snapshot.bounds`, and `null` on every count entry.
/// §FS-rhei-run-json.2.1
#[test]
#[ignore = "red until #515: budget_halt and budget_snapshot carry unpriced_details; implement removes this"]
fn the_json_stream_carries_unpriced_details_and_null_on_count_entries() {
    let (_dir, plan, machine, prices) = setup_unpriced("unpriced-naming-json", FOUR_SPAWNS);

    let result = run_with_book(&plan, &machine, &prices, true);

    let records = records(&result.stdout);
    let halt = records
        .iter()
        .find(|r| r["event"] == "budget_halt")
        .unwrap_or_else(|| panic!("the run emitted no budget_halt:\n{}", result.stdout));
    assert_eq!(halt["unpriced"], 4, "the premise: four unpriced: {halt}");
    assert_details(&halt["unpriced_details"], 2, "budget_halt");

    let snapshot = records
        .iter()
        .rev()
        .find(|r| r["event"] == "budget_snapshot")
        .expect("the run emitted a budget_snapshot");
    for bound in snapshot["bounds"].as_array().expect("bounds is an array") {
        if bound["dimension"] == "project spend" {
            assert_details(&bound["unpriced_details"], 2, "budget_snapshot spend entry");
        } else {
            assert_eq!(
                bound.get("unpriced_details"),
                Some(&serde_json::Value::Null),
                "a count entry carries `unpriced_details: null`, as it carries `unpriced`: {bound}"
            );
        }
    }
}

/// Each unpriced `spend` receipt carries its pricing context, so the journal
/// alone answers the question, and its charge is still the reserve.
/// §FS-rhei-budgets.5.2 §FS-rhei-budgets.6.2
#[test]
#[ignore = "red until #515: an unpriced spend receipt carries pricing_context; implement removes this"]
fn an_unpriced_spend_receipt_carries_its_pricing_context() {
    let (dir, plan, machine, prices) = setup_unpriced("unpriced-naming-receipt", FOUR_SPAWNS);
    run_with_book(&plan, &machine, &prices, false);

    let budgets = dir.join(".agent-grounds/rhei/budgets");
    let mut spends = Vec::new();
    for entry in fs::read_dir(&budgets).expect("the project has an account") {
        let journal = entry.expect("account entry").path().join("journal.jsonl");
        for line in fs::read_to_string(&journal).unwrap_or_default().lines() {
            let receipt: serde_json::Value = serde_json::from_str(line).expect("a receipt");
            if receipt["kind"] == "spend" {
                spends.push(receipt["payload"].clone());
            }
        }
    }
    assert_eq!(spends.len(), 4, "the premise: four settles: {spends:?}");
    let mut models = Vec::new();
    for payload in &spends {
        assert_eq!(payload["basis"], "unpriced", "{payload}");
        assert_eq!(payload["amount_micro"], 20_000_000u64, "the charge is unchanged: {payload}");
        let context = &payload["pricing_context"];
        assert_eq!(context["provider"], "anthropic", "the receipt names its pair: {payload}");
        assert_eq!(context["price_book_id"], BOOK_ID, "{payload}");
        assert_eq!(context["price_book_source"], "file", "{payload}");
        assert_eq!(context["missing_entry"], true, "{payload}");
        assert!(context.get("price_book_sha256").is_none(), "no digest: {payload}");
        models.push(context["model"].as_str().unwrap_or_default().to_owned());
    }
    models.sort();
    assert_eq!(models, [OPUS, OPUS, SONNET, SONNET], "two settles per model");
}

/// `rhei validate` warns once for the book, listing every targeted pair it has
/// no entry for, after the bound lines. §FS-rhei-validate.4.6
#[test]
#[ignore = "red until #515: validate warns once per book about uncovered targets; implement removes this"]
fn validate_warns_once_per_book_about_the_targeted_models_it_cannot_price() {
    let (_dir, plan, machine, _prices) = setup_unpriced("unpriced-naming-validate", "");

    let validated = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&validated);
    let prefix = format!("warning: price book {BOOK_ID} (");
    let warnings: Vec<&str> =
        validated.stdout.lines().filter(|line| line.starts_with(&prefix)).collect();
    assert_eq!(
        warnings.len(),
        1,
        "one warning for the book, never one per pair; got:\n{}",
        validated.stdout
    );
    let pairs = format!(
        "has no entry for anthropic/{OPUS}, anthropic/{SONNET}: their invocations will be \
         measured but unpriced, and each is charged the spend reserve."
    );
    assert!(warnings[0].contains(&pairs), "the warning lists both pairs; got {:?}", warnings[0]);
    let lines: Vec<&str> = validated.stdout.lines().collect();
    let spend = lines.iter().position(|l| l.starts_with("warning: spend_per_day:"));
    let book = lines.iter().position(|l| l.starts_with(&prefix));
    assert!(spend < book, "it follows the bound lines; got:\n{}", validated.stdout);
}

/// `rhei run` says the same before its first reserve: under a ceiling below one
/// reserve nothing is admitted, and the warning is already on stderr ahead of
/// the halt. §FS-rhei-validate.4.6
#[test]
#[ignore = "red until #515: run warns about uncovered targets before its first reserve; implement removes this"]
fn run_warns_about_the_uncovered_models_before_its_first_reserve() {
    let (dir, plan, machine, prices) = setup_unpriced("unpriced-naming-run-warning", NO_SPAWN);

    let result = run_with_book(&plan, &machine, &prices, false);

    assert_spawn_count(&dir, 0, "a $10.00 ceiling admits no $20.00 reserve");
    let warning = format!("warning: price book {BOOK_ID} (");
    let warned = result.stderr.find(&warning);
    let halted = result.stderr.find("has spent today's measured budget");
    assert!(
        warned.is_some() && warned < halted,
        "the coverage warning precedes the first admission; got:\n{}",
        result.stderr
    );
    assert_eq!(
        result.stderr.matches(&warning).count(),
        1,
        "once per book; got:\n{}",
        result.stderr
    );
}
