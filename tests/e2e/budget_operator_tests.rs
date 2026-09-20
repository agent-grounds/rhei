//! §FS-rhei-budgets.7 §FS-rhei-budgets.8 §FS-rhei-budgets.10

use std::fs;
use std::path::Path;

use super::budget_test_support::*;
use super::*;

fn budget(case: &BudgetCase, operation: &str, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(fixture_home(&case.root));
    cmd.arg("budget").arg(operation).arg(&case.plan);
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("budget command should execute");
    CliRun::from(&output)
}

fn uninitialized_case(prefix: &str) -> BudgetCase {
    fresh_case(
        prefix,
        Limits {
            invocations: 3,
            spend_micro: 6_000,
            transition_limit: 3,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
    )
}

#[test]
fn init_and_show_publish_the_persistent_allowance() {
    let case = uninitialized_case("budget-init-show");
    let init = budget(
        &case,
        "init",
        &[
            "--invocations",
            "3",
            "--spend-micro",
            "6000",
            "--currency",
            "USD",
            "--reason",
            "approved test allowance",
        ],
    );
    assert_success(&init);
    let show = budget(&case, "show", &["--format", "json"]);
    assert_success(&show);
    let value: serde_json::Value =
        serde_json::from_str(&show.stdout).expect("budget show should emit JSON");
    assert_eq!(value["allowance"]["invocations"], 3);
    assert_eq!(value["allowance"]["spend"]["amount_micro"], 6000);
    assert_eq!(value["remaining"]["invocations"], 3);
}

#[test]
fn adjustment_is_audited_and_cannot_decrease_below_exposure() {
    let case = cycle_case(
        "budget-adjust",
        Limits {
            invocations: 1,
            spend_micro: 2_000,
            transition_limit: 5,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        2,
    );
    let _ = run_case(&case, &[]);
    let refused =
        budget(&case, "adjust", &["--spend-micro", "1000", "--reason", "unsafe decrease"]);
    assert!(!refused.status.success());
    assert_stderr_contains(&refused, "below settled plus outstanding exposure");

    let raised =
        budget(&case, "adjust", &["--spend-micro", "4000", "--reason", "approved continuation"]);
    assert_success(&raised);
    let show = budget(&case, "show", &["--format", "json"]);
    assert!(show.stdout.contains("approved continuation"));
}

#[test]
fn unknown_history_cannot_be_initialized_as_a_fresh_balance() {
    let case = uninitialized_case("budget-history");
    let accounting = case.root.join("runtime/accounting/invocations");
    fs::create_dir_all(&accounting).expect("create accounting history");
    fs::write(
        accounting.join("unknown.json"),
        r#"{"schema":"rhei.accounting.invocation.v1","invocation_id":"old","task_id":"plan.1","extraction_status":"no-usage-emitted","pricing":{"status":"not-applicable"}}"#,
    )
    .expect("write unknown historical record");

    let init = budget(
        &case,
        "init",
        &[
            "--invocations",
            "3",
            "--spend-micro",
            "6000",
            "--currency",
            "USD",
            "--reason",
            "history import",
        ],
    );
    assert!(!init.status.success());
    assert_stderr_contains(&init, "historical exposure requires complete provider evidence");
}

#[test]
fn corrupt_journal_refuses_admission_until_audited_recovery() {
    let case = cycle_case(
        "budget-corrupt-journal",
        Limits {
            invocations: 4,
            spend_micro: 8_000,
            transition_limit: 4,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        0,
    );
    let journal =
        case.root.join(".agent-grounds/rhei/budgets").join(PROJECT_UUID).join("journal.jsonl");
    fs::write(&journal, "{truncated\n").expect("corrupt fixture journal");

    let run = run_case(&case, &[]);
    assert_eq!(spawn_count(&case), 0, "corrupt persistence must fail closed");
    assert!(combined(&run).contains("budget journal is untrustworthy"));

    let recovered = budget(
        &case,
        "recover",
        &[
            "--journal",
            Path::new("trusted-journal.jsonl").to_str().unwrap(),
            "--reason",
            "restore verified receipts",
        ],
    );
    assert!(!recovered.status.success(), "a missing trusted copy cannot erase exposure");
}

#[test]
fn dry_run_reports_admission_without_debiting_it() {
    let case = cycle_case(
        "budget-dry-run",
        Limits {
            invocations: 2,
            spend_micro: 4_000,
            transition_limit: 5,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        5,
    );
    let dry = run_case(&case, &["--dry-run"]);
    assert!(dry.stdout.contains("would reserve"));
    assert_eq!(spawn_count(&case), 0);

    let run = run_case(&case, &[]);
    assert_eq!(spawn_count(&case), 2, "dry-run must not consume either invocation");
    assert!(combined(&run).contains("Panta invocation allowance exhausted"));
}

#[test]
fn missing_usage_retains_full_exposure_instead_of_becoming_zero_cost() {
    let case = cycle_case(
        "budget-unknown-usage",
        Limits {
            invocations: 4,
            spend_micro: 2_000,
            transition_limit: 4,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    write_python_agent(
        &case.root,
        "bounded-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'fixture-invocations.txt'
count = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(count) + '\n')
fixture_provider_call()
if count > 2:
    sys.exit(1)
"#,
    );

    let qualification =
        case.root.join(".agent-grounds/rhei/qualifications/rhei-test-fixed-2000.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&qualification).unwrap()).unwrap();
    value["missing_usage"] = true.into();
    fs::write(qualification, serde_json::to_vec(&value).unwrap()).unwrap();
    let contained = run_case(&case, &[]);
    assert!(!contained.status.success());
    let shown = budget(&case, "show", &["--format", "json"]);
    assert_success(&shown);
    let balance: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(balance["reserved"]["spend"]["amount_micro"], 2000);
    assert_eq!(balance["consumed"]["spend"]["amount_micro"], 0);
    let run = run_case(&case, &[]);
    assert_eq!(spawn_count(&case), 1, "unknown usage keeps the first FWC charged");
    assert!(combined(&run).contains("spend allowance exhausted"));
}

#[test]
fn ambiguous_start_retains_the_invocation_and_full_exposure() {
    let case = cycle_case(
        "budget-ambiguous-start",
        Limits {
            invocations: 1,
            spend_micro: 2_000,
            transition_limit: 4,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        0,
    );
    let reservation = "reservation:00000000-0000-0000-0000-000000000001";
    append_receipt(
        &case.root,
        2,
        "receipt:00000000-0000-0000-0000-000000000002",
        "reserve",
        serde_json::json!({
            "reservation_id": reservation,
            "attempt_identity": "plan.1::a::fixture::visit-1::run-fixture::move-0::attempt-1",
            "ticket_identity": format!("ticket:{PROJECT_UUID}:{TICKET_UUID}"),
            "parent_reservation": null,
            "qualification": "rhei-test:fixed-2000:contained",
            "qualification_evidence_hash": "sha256:fixture",
            "invocation_units": 1,
            "threshold_micro": 2_000,
            "residual_micro": 0,
            "fwc_micro": 2_000,
            "travel_units": 1
        }),
    );
    append_receipt(
        &case.root,
        3,
        "receipt:00000000-0000-0000-0000-000000000003",
        "start",
        serde_json::json!({
            "reservation_id": reservation,
            "status": "ambiguous"
        }),
    );

    let run = run_case(&case, &[]);
    assert_eq!(spawn_count(&case), 0, "ambiguous prior work consumes the only invocation");
    assert!(combined(&run).contains("Panta invocation allowance exhausted"));
}

#[test]
fn poll_wait_releases_travel_but_consumes_its_outer_invocation() {
    let case = cycle_case(
        "budget-poll-composition",
        Limits {
            invocations: 1,
            spend_micro: 10_000,
            transition_limit: 1,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    fs::write(
        &case.machine,
        r#"name: bounded-poll
version: 1
models: [fixture]
states:
  a:
    description: A bounded poll
    agent: codex
    model: fixture
    agent_timeout: 5s
    budget_threshold: {currency: USD, amount_micro: 2000}
    poll: {interval: 0s, max_attempts: 3}
  completed:
    final: true
    description: Done
profiles:
  work:
    initial: a
    allowed: [a, completed]
    transition_limit: 1
node_policy:
  root: work
  default: work
transitions:
  - from: a
    to: a
    condition: pollAttempts < pollMaxAttempts
  - from: a
    to: completed
    condition: pollAttempts >= pollMaxAttempts
"#,
    )
    .expect("write poll machine");

    let run = run_case(&case, &[]);
    assert_eq!(
        spawn_count(&case),
        1,
        "the poll wait releases travel but its first neural start consumes the outer invocation"
    );
    assert!(combined(&run).contains("Panta invocation allowance exhausted"));
    assert_task_state(&case.plan, &case.machine, "1", "a");
}

#[test]
fn restart_and_reset_never_replenish_an_invocation_or_retry_refund() {
    let case = cycle_case(
        "budget-reset-persistence",
        Limits {
            invocations: 1,
            spend_micro: 10_000,
            transition_limit: 10,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    write_python_agent(
        &case.root,
        "bounded-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'fixture-invocations.txt'
count = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(count) + '\n')
fixture_provider_call()
sys.exit(1)
"#,
    );

    let first = run_case(&case, &[]);
    assert!(!first.status.success());
    assert_eq!(spawn_count(&case), 1, "the one authorized invocation must be permitted");

    let reset = run_cli("reset", &case.plan, &case.machine, &["--yes"]);
    assert_success(&reset);
    let second = run_case(&case, &[]);
    assert_eq!(
        spawn_count(&case),
        1,
        "reset, restart, and fresh inner attempt headroom must not mint an outer invocation"
    );
    assert!(combined(&second).contains("Panta invocation allowance exhausted"));
}
