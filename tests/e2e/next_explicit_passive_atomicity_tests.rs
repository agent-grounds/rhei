//! Transition and rollback boundaries for an explicit non-initial passive claim.

use std::fs;

use super::*;

fn fixture(
    prefix: &str,
    machine: &str,
) -> (TestDir, std::path::PathBuf, std::path::PathBuf, String) {
    let plan =
        "# Rhei: Explicit claim boundary\n\n## Tasks\n\n### Task 1: Claim me\n**State:** bridge\n"
            .to_string();
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    (dir, plan_path, machine_path, plan)
}

fn machine(edge: &str, bridge_extra: &str, work_extra: &str) -> String {
    format!(
        r#"name: explicit-claim-boundary
version: 1
states:
  start:
    initial: true
    description: Initial
  bridge:
    description: Passive bridge
{bridge_extra}  work:
    description: Work
    agent: codex
{work_extra}  review:
    description: Review
    agent: claude-code
  done:
    description: Done
    final: true
transitions:
  - from: start
    to: bridge
{edge}  - from: work
    to: done
  - from: review
    to: done
"#
    )
}

/// The non-initial path uses the transition executor, so it enforces source
/// outputs instead of treating the operation as an assignee-only rewrite.
// §FS-rhei-next.3.1 §FS-rhei-plan-language.3.10
#[test]
fn issue_286_explicit_passive_claim_checks_source_outputs_before_writing() {
    let machine = machine(
        "  - from: bridge\n    to: work\n",
        "    outputs:\n      - name: brief\n        path: runtime/brief.md\n",
        "",
    );
    let (dir, plan_path, machine_path, original) = fixture("next-explicit-source-output", &machine);

    let refused = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--no-callbacks"]);
    assert!(!refused.status.success(), "missing source output must refuse the claim");
    assert_stderr_contains(&refused, "Missing required output artifact: brief (runtime/brief.md)");
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), original);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// Effective-target inputs are inside the same transaction and a refusal
/// leaves state, ownership, and ledger untouched.
// §FS-rhei-next.3.1 §FS-rhei-plan-language.3.10
#[test]
fn issue_286_explicit_passive_claim_checks_target_inputs_before_writing() {
    let machine = machine(
        "  - from: bridge\n    to: work\n",
        "",
        "    inputs:\n      - name: brief\n        path: runtime/brief.md\n",
    );
    let (dir, plan_path, machine_path, original) = fixture("next-explicit-target-input", &machine);

    let refused = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--no-callbacks"]);
    assert!(!refused.status.success(), "missing target input must refuse the claim");
    assert_stderr_contains(&refused, "Missing required input artifact: brief (runtime/brief.md)");
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), original);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// A callback can redirect the one selected edge, but the claim resolves
/// ownership from that effective non-terminal state and still records one edge.
// §FS-rhei-next.3 §FS-rhei-next.3.1
#[test]
fn issue_286_explicit_passive_claim_honors_one_non_terminal_redirect() {
    let callback = python_callback_yaml(
        "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'review'}))",
    );
    let edge = format!(
        "  - from: bridge\n    to: work\n    on_leave: {callback}\n  - from: bridge\n    to: review\n"
    );
    let machine = machine(&edge, "", "");
    let (dir, plan_path, machine_path, _) = fixture("next-explicit-redirect", &machine);

    let claimed = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--json"]);
    assert_success(&claimed);
    let json: serde_json::Value = serde_json::from_str(&claimed.stdout).expect("next JSON");
    assert_eq!(json["from_state"], "bridge");
    assert_eq!(json["state"], "review");
    assert_eq!(json["claimed_as"], "claude-code");
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** review\n**Assignee:** claude-code"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.1 bridge@review\n"
    );
}

/// A terminal callback redirect is not completion-by-claim; rejection restores
/// the exact task and leaves no ownership or ledger record.
// §FS-rhei-next.3 §FS-rhei-next.3.1
#[test]
fn issue_286_explicit_passive_claim_refuses_terminal_redirect_and_restores() {
    let callback = python_callback_yaml(
        "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'done'}))",
    );
    let edge = format!(
        "  - from: bridge\n    to: work\n    on_leave: {callback}\n  - from: bridge\n    to: done\n"
    );
    let machine = machine(&edge, "", "");
    let (dir, plan_path, machine_path, original) =
        fixture("next-explicit-terminal-redirect", &machine);

    let refused = run_cli("next", &plan_path, &machine_path, &["--task", "1"]);
    assert!(!refused.status.success(), "terminal redirect must refuse the claim");
    assert_stderr_contains(&refused, "cannot enter terminal state 'done' without a result");
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), original);
    assert!(!dir.join("runtime/state-transitions.log").exists());
}

/// Ordinary ledger failure restores the non-initial source bytes; after the
/// obstruction is removed, a retry commits the state, owner, and one entry.
// §FS-rhei-next.3.1
#[test]
fn issue_286_explicit_passive_claim_restores_and_retries_after_ledger_failure() {
    let machine = machine("  - from: bridge\n    to: work\n", "", "");
    let (dir, plan_path, machine_path, original) =
        fixture("next-explicit-ledger-failure", &machine);
    fs::write(dir.join("runtime"), "not a directory\n").expect("write obstruction");

    let refused = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--no-callbacks"]);
    assert!(!refused.status.success(), "ledger obstruction must refuse the claim");
    assert_stderr_contains(&refused, "failed to create runtime directory");
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), original);

    fs::remove_file(dir.join("runtime")).expect("remove obstruction");
    let retried = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--no-callbacks"]);
    assert_success(&retried);
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** work\n**Assignee:** codex"));
    assert_eq!(
        fs::read_to_string(dir.join("runtime/state-transitions.log")).unwrap(),
        "plan.1 bridge@work\n"
    );
}
