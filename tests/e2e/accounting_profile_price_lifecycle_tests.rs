//! Whole-run profile pricing includes later agent states before the first
//! execution surface starts. §FS-rhei-cost-accounting.5.1
//! Invocation provenance remains exact. §FS-rhei-cost-accounting.3

use std::fs;
use std::path::{Path, PathBuf};

use super::accounting_profile_prices_tests::{
    expected_generated_book, invocation_records, profile, profile_prices, read_json,
    write_settings, ONE_TASK_PLAN,
};
use super::*;

fn snapshot_probe(root: &Path, surface: &str) -> String {
    format!(
        r#"root = pathlib.Path({})
with (root / 'started.log').open('a') as marker:
    marker.write('{surface}\n')
snapshot = root / 'runtime/accounting/prices.json'
book = json.loads(snapshot.read_text()) if snapshot.exists() else None
archive = root / 'runtime/accounting/price-books' / (book['price_book_id'] + '.json') if book else None
with (root / 'observed.jsonl').open('a') as observations:
    observations.write(json.dumps({{
        'surface': '{surface}',
        'book': book,
        'archive': json.loads(archive.read_text()) if archive and archive.exists() else None,
    }}) + '\n')
"#,
        serde_json::to_string(root.to_string_lossy().as_ref()).expect("fixture root")
    )
}

struct LifecycleFixture {
    dir: TestDir,
    plan: PathBuf,
    machine: PathBuf,
    rates_a: serde_json::Value,
    rates_b: serde_json::Value,
}

fn lifecycle_fixture(label: &str, program_first: bool, conflict: bool) -> LifecycleFixture {
    let dir = unique_temp_dir(label);
    let program = write_python_agent(
        &dir,
        "program.py",
        &format!(
            "import json\n{}\nresult('Program completed.\\n')\n",
            snapshot_probe(&dir, "program")
        ),
    );
    let start = if program_first { "build" } else { "work-a" };
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &ONE_TASK_PLAN.replace("**State:** work", &format!("**State:** {start}")),
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: profile-price-lifecycle
version: 1
models: [a, b, unused]
states:
  build:
    initial: true
    description: Prepare agent work
    program:
      command: {}
  work-a:
    description: First agent
    agent: codex
    model: a
  work-b:
    description: Later agent
    agent: codex
    model: b
  unreachable:
    description: Unselected work with conflicting prices
    agent: codex
    model: unused
  completed:
    description: Done
    final: true
transitions:
  - from: build
    to: work-a
    exit_code: 0
  - from: work-a
    to: work-b
  - from: work-b
    to: completed
  - from: unreachable
    to: completed
"#,
            fixture_command(&program)
        ),
    );
    let rates_a = profile_prices("CHF", 4_000_000, "first profile");
    let rates_b = profile_prices("CHF", 6_000_000, "later profile");
    write_settings(
        &dir,
        serde_json::json!({
            "a": profile("openai", "model-a", Some(rates_a.clone())),
            "b": profile("openai", if conflict { "model-a" } else { "model-b" }, Some(rates_b.clone())),
            "unused": profile("openai", "model-a", Some(profile_prices("USD", 9_000_000, "unreachable")))
        }),
        None,
        &snapshot_probe(&dir, "agent"),
    );
    LifecycleFixture { dir, plan, machine, rates_a, rates_b }
}

fn assert_snapshots(root: &Path, expected: &serde_json::Value, surfaces: &[&str]) {
    let observations = fs::read_to_string(root.join("observed.jsonl")).expect("snapshot probes");
    let observations = observations
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("probe JSON"))
        .collect::<Vec<_>>();
    assert_eq!(observations.len(), surfaces.len());
    for (observation, surface) in observations.iter().zip(surfaces) {
        assert_eq!(observation["surface"], *surface);
        assert_eq!(observation["book"], *expected, "snapshot before {surface}");
        assert_eq!(observation["archive"], *expected, "archive before {surface}");
    }
    assert_eq!(read_json(&root.join("runtime/accounting/prices.json")), *expected);
    let id = expected["price_book_id"].as_str().expect("book id");
    assert_eq!(
        read_json(&root.join("runtime/accounting/price-books").join(format!("{id}.json"))),
        *expected
    );
}

fn assert_priced_records(root: &Path, book: &serde_json::Value, expected: &[(&str, &str, u64)]) {
    let records = invocation_records(root);
    assert_eq!(records.len(), expected.len());
    for (profile, model, amount) in expected {
        let record = &records
            .iter()
            .find(|(_, record)| record["model_profile"] == *profile)
            .expect("profile invocation")
            .1;
        assert_eq!(record["provider"], "openai");
        assert_eq!(record["model"], *model);
        assert_eq!(record["pricing"]["status"], "priced");
        assert_eq!(record["pricing"]["price_book_id"], book["price_book_id"]);
        assert_eq!(record["pricing"]["currency"], book["currency"]);
        assert_eq!(record["pricing"]["amount_micro"], *amount);
        assert_eq!(record["pricing"]["priced_amount_micro"], *amount);
    }
}

fn assert_later_profiles_are_priced(program_first: bool) {
    let fixture = lifecycle_fixture("profile-prices-later-state", program_first, false);
    let result = run_cli("run", &fixture.plan, &fixture.machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&fixture.plan, &fixture.machine, "1", "completed");
    let expected = expected_generated_book(
        "CHF",
        &[
            ("openai", "model-a", &["a"], fixture.rates_a),
            ("openai", "model-b", &["b"], fixture.rates_b),
        ],
    );
    let surfaces: &[&str] =
        if program_first { &["program", "agent", "agent"] } else { &["agent", "agent"] };
    assert_snapshots(&fixture.dir, &expected, surfaces);
    assert_priced_records(
        &fixture.dir,
        &expected,
        &[("a", "model-a", 18_450_000), ("b", "model-b", 19_450_000)],
    );
}

#[test]
fn program_to_agent_profiles_are_snapshotted_before_the_program_starts() {
    assert_later_profiles_are_priced(true);
}

#[test]
fn agent_to_agent_profiles_share_the_pre_execution_book() {
    assert_later_profiles_are_priced(false);
}

#[test]
fn later_profile_conflicts_fail_before_the_first_program_or_agent() {
    for program_first in [true, false] {
        let fixture = lifecycle_fixture("profile-prices-later-conflict", program_first, true);
        let result =
            run_cli("run", &fixture.plan, &fixture.machine, &["--no-tui", "--no-callbacks"]);
        assert!(!result.status.success(), "later conflicting profile must fail preflight");
        let output = format!("{}{}", result.stdout, result.stderr);
        for text in ["model-a", "profile 'a'", "profile 'b'"] {
            assert!(output.contains(text), "missing {text}: {output}");
        }
        assert!(!fixture.dir.join("started.log").exists(), "execution began before refusal");
        assert!(!fixture.dir.join("runtime/accounting/prices.json").exists());
        assert!(!fixture.dir.join("runtime/accounting/price-books").exists());
    }
}

#[test]
fn later_states_use_task_and_cli_model_precedence() {
    for cli in [false, true] {
        let fixture = lifecycle_fixture("profile-prices-later-overrides", true, true);
        let task_model = if cli { "b" } else { "a" };
        let plan = fs::read_to_string(&fixture.plan).expect("plan");
        fs::write(&fixture.plan, format!("{plan}**Model:** {task_model}\n")).expect("task model");
        let mut args = vec!["--no-tui", "--no-callbacks"];
        if cli {
            args.extend(["--model", "a"]);
        }
        assert_success(&run_cli("run", &fixture.plan, &fixture.machine, &args));
        let expected =
            expected_generated_book("CHF", &[("openai", "model-a", &["a"], fixture.rates_a)]);
        assert_snapshots(&fixture.dir, &expected, &["program", "agent", "agent"]);
        let records = invocation_records(&fixture.dir);
        assert_eq!(records.len(), 2);
        for (_, record) in records {
            assert_eq!(record["model_profile"], "a");
            assert_eq!(record["pricing"]["price_book_id"], expected["price_book_id"]);
            assert_eq!(record["pricing"]["amount_micro"], 18_450_000);
        }
    }
}

#[test]
fn explicit_book_bypasses_later_profile_conflicts() {
    let fixture = lifecycle_fixture("profile-prices-later-explicit", true, true);
    let mut explicit =
        expected_generated_book("CHF", &[("openai", "model-a", &["a"], fixture.rates_a)]);
    explicit["price_book_id"] = serde_json::json!("explicit-lifecycle");
    let path = write_fixture_file(
        &fixture.dir,
        "explicit.json",
        &serde_json::to_string(&explicit).expect("explicit book"),
    );
    assert_success(&run_cli(
        "run",
        &fixture.plan,
        &fixture.machine,
        &["--no-tui", "--no-callbacks", "--prices", path.to_str().expect("book path")],
    ));
    assert_eq!(read_json(&fixture.dir.join("runtime/accounting/prices.json")), explicit);
    assert!(!fixture.dir.join("runtime/accounting/price-books").exists());
    assert_priced_records(
        &fixture.dir,
        &explicit,
        &[("a", "model-a", 18_450_000), ("b", "model-a", 18_450_000)],
    );
}

#[test]
fn node_profile_excludes_disallowed_later_prices() {
    let fixture = lifecycle_fixture("profile-prices-node-profile", false, true);
    let machine = fs::read_to_string(&fixture.machine).expect("machine");
    fs::write(
        &fixture.machine,
        format!(
            "{}\n  - from: work-a\n    to: completed\n\
             profiles:\n  limited:\n    initial: work-a\n    allowed: [work-a, completed]\n\
             node_policy:\n  root: limited\n  default: limited\n",
            machine.replace("    initial: true\n", "")
        ),
    )
    .expect("restricted machine");
    assert_success(&run_cli(
        "run",
        &fixture.plan,
        &fixture.machine,
        &["--no-tui", "--no-callbacks"],
    ));
    let expected =
        expected_generated_book("CHF", &[("openai", "model-a", &["a"], fixture.rates_a)]);
    assert_snapshots(&fixture.dir, &expected, &["agent"]);
    assert_priced_records(&fixture.dir, &expected, &[("a", "model-a", 18_450_000)]);
}

/// Descendants participate even while their parent is program work; a narrowed
/// run must not include another rhei's conflicting profile. §FS-rhei-panta.6.1
#[test]
fn nested_candidates_preserve_selected_rhei_and_accounting_roots() {
    let fixture = lifecycle_fixture("profile-prices-nested-scope", true, true);
    let machine = fs::read_to_string(&fixture.machine).expect("machine");
    fs::write(
        &fixture.machine,
        machine.replace("from: build\n    to: work-a", "from: build\n    to: completed"),
    )
    .expect("parent program goes straight to completion");
    fs::write(fixture.dir.join("index.panta.md"), "# Panta: Scoped lifecycle pricing\n")
        .expect("project");
    let alpha = fixture.dir.join("alpha");
    let beta = fixture.dir.join("beta");
    for root in [&alpha, &beta] {
        fs::create_dir_all(root.join("tasks")).expect("member task directory");
        fs::write(root.join("index.rhei.md"), "# Rhei: Pricing member\n").expect("member");
    }
    fs::write(
        alpha.join("tasks/work.md"),
        "### Task 1: Program parent\n**State:** build\n\n\
         #### Task 1.1: Priced child\n**State:** work-b\n",
    )
    .expect("nested task");
    fs::write(beta.join("tasks/work.md"), "### Task 1: Outside scope\n**State:** work-a\n")
        .expect("out-of-scope task");
    assert_success(&run_cli(
        "run",
        &fixture.dir,
        &fixture.machine,
        &["--no-tui", "--no-callbacks", "--rhei", "alpha"],
    ));
    let expected =
        expected_generated_book("CHF", &[("openai", "model-a", &["b"], fixture.rates_b)]);
    assert_snapshots(&fixture.dir, &expected, &["agent", "program"]);
    assert_priced_records(&alpha, &expected, &[("b", "model-a", 19_450_000)]);
    let id = expected["price_book_id"].as_str().expect("id");
    assert_eq!(read_json(&alpha.join("runtime/accounting/prices.json")), expected);
    assert_eq!(
        read_json(&alpha.join("runtime/accounting/price-books").join(format!("{id}.json"))),
        expected
    );
    assert!(!beta.join("runtime/accounting/prices.json").exists());
    assert!(!beta.join("runtime/accounting/price-books").exists());
}
