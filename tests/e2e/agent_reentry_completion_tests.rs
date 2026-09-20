//! Focused scheduling cases for the spawn-record proof that makes an existing
//! output eligible for reuse during one visit.

// §FS-rhei-agents.3.2 §FS-rhei-agents.8.4 §FS-rhei-run.3

use std::fs;

use super::agent_reentry_support::*;
use super::*;

fn seeded_reentry(name: &str) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let fixture = setup(name, BASIC_MACHINE, COUNTING_AGENT);
    seed_output(&fixture.0, "runtime/digest.md");
    establish_reentry(&fixture.0);
    fixture
}

fn assert_old_record_spawns(label: &str, extra: &[&str]) {
    let (dir, plan, machine) = seeded_reentry(&format!("agent-old-record-{label}"));
    write_spawn_record(&dir, None, 0, "exited", Some(0));
    let mut args = vec!["--no-tui", "--no-callbacks"];
    args.extend_from_slice(extra);
    assert_success(&run_cli("run", &plan, &machine, &args));
    assert_eq!(
        spawn_lines(&dir),
        ["work"],
        "{label} scheduling must not reuse an older visit's successful record"
    );
}

#[test]
fn sequential_scheduling_requires_current_visit_proof() {
    assert_old_record_spawns("sequential", &[]);
}

#[test]
fn parallel_scheduling_requires_current_visit_proof() {
    assert_old_record_spawns("parallel", &["--parallel", "2"]);
}

#[test]
fn dry_run_predicts_the_fresh_spawn_required_by_execution() {
    let (dir, plan, machine) = seeded_reentry("agent-old-record-dry-run");
    write_spawn_record(&dir, None, 0, "exited", Some(0));
    let dry = run_cli("run", &plan, &machine, &["--dry-run", "--no-tui", "--no-callbacks"]);
    assert_success(&dry);
    assert!(
        dry.stdout.contains("Would spawn:"),
        "dry run must predict the same fresh spawn as execution; got:\n{}{}",
        dry.stdout,
        dry.stderr
    );
    assert!(
        dry.stdout.contains("would transition: Task plan.1  work -> verifying"),
        "dry run must also project the selectable post-work edge; got:\n{}{}",
        dry.stdout,
        dry.stderr
    );
    assert!(spawn_lines(&dir).is_empty(), "dry run executes no worker");
}

#[test]
fn dry_run_predicts_a_pending_spawn_when_no_edge_is_selectable() {
    let machine = r#"name: agent-reentry-no-edge
version: 1
states:
  work:
    initial: true
    description: Produce the digest
    agent: mock
    agent_timeout: 10s
    outputs:
      - name: digest
        path: runtime/digest.md
  completed:
    description: Done
    final: true
transitions:
  - { from: work, to: completed, condition: visitCount > 1 }
"#;
    let (dir, plan, machine) = setup("agent-dry-run-no-edge", machine, COUNTING_AGENT);

    let dry = run_cli("run", &plan, &machine, &["--dry-run", "--no-tui", "--no-callbacks"]);
    assert_success(&dry);
    assert!(dry.stdout.contains("Would spawn:"), "pending work must be shown:\n{}", dry.stdout);
    assert!(
        !dry.stdout.contains("would transition: Task plan.1"),
        "dry run must not invent a post-work edge:\n{}",
        dry.stdout
    );
    assert!(spawn_lines(&dir).is_empty(), "dry run executes no worker");
}

#[test]
fn unsuccessful_current_visit_records_do_not_complete_an_invocation() {
    let mut wrongly_reused = Vec::new();
    for (label, ending, code) in [
        ("failed", "exited", Some(3)),
        ("interrupted", "interrupted", None),
        ("provider-limited", "provider_limited", Some(1)),
        ("timed-out", "timed out", None),
    ] {
        let (dir, plan, machine) = seeded_reentry(&format!("agent-record-{label}"));
        write_spawn_record(&dir, None, 1, ending, code);
        assert_success(&run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]));
        if spawn_lines(&dir).is_empty() {
            wrongly_reused.push(label);
        }
    }
    assert!(
        wrongly_reused.is_empty(),
        "failed, interrupted, provider-limited, and timed-out records are history, not \
         successful current-visit proof; wrongly reused={wrongly_reused:?}"
    );
}

#[test]
fn successful_current_visit_and_legacy_preseed_reuse_outputs() {
    let (current_dir, current_plan, current_machine) = seeded_reentry("agent-current-record");
    write_spawn_record(&current_dir, None, 1, "exited", Some(0));
    assert_success(&run_cli(
        "run",
        &current_plan,
        &current_machine,
        &["--no-tui", "--no-callbacks"],
    ));
    assert!(spawn_lines(&current_dir).is_empty(), "same-visit success remains reusable");

    let (legacy_dir, legacy_plan, legacy_machine) =
        setup("agent-legacy-preseed", BASIC_MACHINE, COUNTING_AGENT);
    seed_output(&legacy_dir, "runtime/digest.md");
    assert_success(&run_cli("run", &legacy_plan, &legacy_machine, &["--no-tui", "--no-callbacks"]));
    assert!(
        spawn_lines(&legacy_dir).is_empty(),
        "without either history source an upgraded workspace retains first-visit reuse"
    );
}

const FANOUT_MACHINE: &str = r#"name: agent-reentry-fanout
version: 1
models: [alpha, beta]
states:
  work:
    initial: true
    description: Produce one digest per identity
    all_targets: ["mock:mock:alpha", "mock:mock:beta"]
    agent_timeout: 10s
    outputs:
      - name: digest
        path: runtime/digest-{model}.md
  verifying:
    description: Inspect both digests
    gating: true
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: verifying, description: Digests ready }
  - { from: verifying, to: work, description: Revise them }
  - { from: "*", to: cancelled, description: Stop }
"#;

#[test]
fn fanout_identities_supply_visit_proof_independently() {
    let dir = unique_temp_dir("agent-reentry-fanout");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", FANOUT_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
model = env('RHEI_MODEL')
append(root / 'runtime' / 'spawn-count.log', model + '\n')
write(root / 'runtime' / ('digest-' + model + '.md'), model + '\n')
"#,
    );
    write_settings(&dir, &agent, true);

    let args = ["--parallel", "2", "--no-tui", "--no-callbacks"];
    assert_success(&run_cli("run", &plan, &machine, &args));
    assert_success(&run_transition(&plan, &machine, "1", "verifying", "work"));
    let current_moves = ledger(&dir).lines().count() as u64;
    let records = dir.join("runtime/spawns");
    let alpha = fs::read_dir(&records)
        .expect("spawn records")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.file_name().is_some_and(|name| name.to_string_lossy().contains("alpha")))
        .expect("alpha spawn record");
    let mut record: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&alpha).expect("read alpha record"))
            .expect("parse alpha record");
    record["moves"] = current_moves.into();
    fs::write(&alpha, serde_json::to_string_pretty(&record).expect("serialize alpha record"))
        .expect("update alpha record");

    assert_success(&run_cli("run", &plan, &machine, &args));
    let spawns = spawn_lines(&dir);
    assert_eq!(spawns.iter().filter(|line| line.as_str() == "alpha").count(), 1);
    assert_eq!(
        spawns.iter().filter(|line| line.as_str() == "beta").count(),
        2,
        "alpha's current-visit proof must not excuse beta's older record; spawns={spawns:?}"
    );
}
