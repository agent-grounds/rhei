//! Operator regressions use real initialization without any qualification.
//! §FS-rhei-budgets.2.3 §FS-rhei-budgets.3.2 §FS-rhei-budgets.8

use super::budget_test_support::*;
use super::*;

fn fresh(prefix: &str) -> BudgetCase {
    fresh_case(
        prefix,
        Limits {
            invocations: 3,
            spend_micro: 6000,
            transition_limit: 3,
            threshold_micro: 2000,
            residual_micro: 0,
        },
    )
}

fn command(case: &BudgetCase, operation: &str, target: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(fixture_home(&case.root))
        .arg("budget")
        .arg(operation)
        .arg(target)
        .args(args)
        .output()
        .expect("budget command");
    CliRun::from(&output)
}

fn initialize(case: &BudgetCase, target: &Path) -> CliRun {
    command(
        case,
        "init",
        target,
        &[
            "--invocations",
            "3",
            "--spend-micro",
            "6000",
            "--currency",
            "USD",
            "--reason",
            "bounded persistence fixture",
        ],
    )
}

fn journal(case: &BudgetCase) -> PathBuf {
    let show = command(case, "show", &case.plan, &["--format", "json"]);
    assert_success(&show);
    let value: serde_json::Value = serde_json::from_str(&show.stdout).unwrap();
    let id = value["project_id"].as_str().unwrap().strip_prefix("panta:").unwrap();
    case.root.join(".agent-grounds/rhei/budgets").join(id).join("journal.jsonl")
}

#[test]
fn budget_invalid_identity_never_activates_an_orphan_allowance() {
    let case = fresh("budget-invalid-init");
    let original = fs::read_to_string(&case.plan).unwrap();
    fs::write(
        &case.plan,
        original.replace(
            "## Tasks",
            "---\nmetadata:\n  tasks:\n    1:\n      budgetTicketId: bad-uuid\n---\n\n## Tasks",
        ),
    )
    .unwrap();
    let rejected = initialize(&case, &case.plan);
    assert!(!rejected.status.success());
    assert_stderr_contains(&rejected, "invalid budget identity");
    let budget_dir = case.root.join(".agent-grounds/rhei/budgets");
    assert!(!budget_dir.join("initialization.json").exists());
    assert!(fs::read_dir(&budget_dir).unwrap().all(|entry| !entry.unwrap().path().is_dir()));
    fs::write(&case.plan, original).unwrap();
    assert_success(&initialize(&case, &case.plan));
}

#[test]
fn budget_interrupted_identity_install_resumes_the_original_allowance() {
    let case = fresh("budget-init-resume");
    let before = fs::read_to_string(&case.plan).unwrap();
    let after = before.replace("## Tasks", &format!("---\nbudgetProjectId: {PROJECT_UUID}\nmetadata:\n  tasks:\n    1:\n      budgetTicketId: {TICKET_UUID}\n---\n\n## Tasks"));
    let directory = case.root.join(".agent-grounds/rhei/budgets");
    fs::create_dir_all(&directory).unwrap();
    let mut documents = serde_json::Map::new();
    documents.insert(
        case.plan.to_string_lossy().into_owned(),
        serde_json::json!({"before": before, "after": after}),
    );
    let intent = serde_json::json!({
        "schema": "rhei.budget.initialization.v1", "project_uuid": PROJECT_UUID,
        "root": case.root, "allowance": {"invocations": 3, "spend": {"currency": "USD", "amount_micro": 6000}},
        "audit": {"actor": "rhei-e2e", "written_at": "2026-09-18T12:00:00Z", "reason": "initial intent", "argv": ["budget", "init"]},
        "documents": documents,
    });
    fs::write(directory.join("initialization.json"), serde_json::to_vec(&intent).unwrap()).unwrap();
    // Simulate exit after metadata installation but before ledger activation.
    fs::write(&case.plan, &after).unwrap();
    assert_success(&initialize(&case, &case.plan));
    assert!(!directory.join("initialization.json").exists());
    let shown = command(&case, "show", &case.plan, &["--format", "json"]);
    assert_success(&shown);
    let value: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(value["project_id"], format!("panta:{PROJECT_UUID}"));
    assert_eq!(value["allowance"]["invocations"], 3);
    assert_eq!(value["remaining"]["spend"]["amount_micro"], 6000);
}

#[test]
fn budget_workspace_manifest_preserves_separate_task_files() {
    let case = fresh("budget-workspace-manifest");
    fs::create_dir(case.root.join("tasks")).unwrap();
    let task = "### Task 1: Work\n**State:** pending\n";
    fs::write(case.root.join("tasks/01-work.md"), task).unwrap();
    fs::write(case.root.join("index.rhei.md"), "# Rhei: Directory fixture\n").unwrap();
    assert_success(&initialize(&case, &case.root));
    assert_success(&command(&case, "show", &case.root, &["--format", "json"]));
    assert_eq!(fs::read_to_string(case.root.join("tasks/01-work.md")).unwrap(), task);
    assert!(fs::read_to_string(case.root.join("index.rhei.md"))
        .unwrap()
        .contains("budgetTicketId"));
}

#[test]
fn budget_member_and_basin_use_the_project_allowance() {
    let case = fresh("budget-member-manifest");
    fs::write(case.root.join("index.panta.md"), "# Panta: Shared allowance\n").unwrap();
    fs::create_dir(case.root.join("basin")).unwrap();
    fs::write(case.root.join("basin/01-basin.md"), "### Task 1: Unfiled\n**State:** pending\n")
        .unwrap();
    assert_success(&initialize(&case, &case.plan));
    let member = command(&case, "show", &case.plan, &["--format", "json"]);
    let whole = command(&case, "show", &case.root, &["--format", "json"]);
    assert_success(&member);
    assert_success(&whole);
    assert_eq!(member.stdout, whole.stdout);
    assert!(fs::read_to_string(case.root.join("index.panta.md"))
        .unwrap()
        .contains("budgetProjectId"));
    assert!(!fs::read_to_string(&case.plan).unwrap().contains("budgetProjectId"));
    assert!(fs::read_to_string(case.root.join("index.panta.md"))
        .unwrap()
        .contains("budgetTicketId"));
}

#[test]
fn budget_complete_tail_loss_requires_witnessed_recovery() {
    let case = fresh("budget-tail-recovery");
    assert_success(&initialize(&case, &case.plan));
    let path = journal(&case);
    let initial = fs::read(&path).unwrap();
    assert_success(&command(
        &case,
        "adjust",
        &case.plan,
        &["--invocations", "1", "--spend-micro", "2000", "--reason", "reduce cap"],
    ));
    let trusted = case._dir.join("trusted.jsonl");
    fs::copy(&path, &trusted).unwrap();
    fs::write(&path, &initial).unwrap();
    let refused = command(&case, "show", &case.plan, &["--format", "json"]);
    assert!(!refused.status.success());
    assert_stderr_contains(&refused, "authoritative committed history");
    let stale = case._dir.join("stale.jsonl");
    fs::write(&stale, &initial).unwrap();
    assert!(!command(
        &case,
        "recover",
        &case.plan,
        &["--journal", stale.to_str().unwrap(), "--reason", "stale prefix"]
    )
    .status
    .success());
    assert_success(&command(
        &case,
        "recover",
        &case.plan,
        &["--journal", trusted.to_str().unwrap(), "--reason", "restore witnessed history"],
    ));
    let shown = command(&case, "show", &case.plan, &["--format", "json"]);
    assert_success(&shown);
    let value: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(value["remaining"]["invocations"], 1);
    assert_eq!(value["remaining"]["spend"]["amount_micro"], 2000);
    assert_eq!(fs::read(&path).unwrap(), fs::read(&trusted).unwrap());
    assert!(shown.stdout.contains("restore witnessed history"));
}

#[test]
fn budget_copied_roots_cannot_fork_the_same_allowance() {
    let case = fresh("budget-copy-authority");
    assert_success(&initialize(&case, &case.plan));
    let path = journal(&case);
    let copy = case._dir.join("copy");
    fs::create_dir(&copy).unwrap();
    fs::copy(&case.plan, copy.join("plan.rhei.md")).unwrap();
    let relative = path.strip_prefix(&case.root).unwrap();
    fs::create_dir_all(copy.join(relative).parent().unwrap()).unwrap();
    fs::copy(&path, copy.join(relative)).unwrap();
    assert_success(&command(&case, "show", &copy.join("plan.rhei.md"), &["--format", "json"]));
    assert_success(&command(
        &case,
        "adjust",
        &case.plan,
        &["--spend-micro", "7000", "--reason", "shared cap"],
    ));
    let refused = command(&case, "show", &copy.join("plan.rhei.md"), &["--format", "json"]);
    assert!(!refused.status.success());
    assert_stderr_contains(&refused, "authoritative committed history");
}

#[test]
fn budget_conflicting_live_copy_refuses_allowance_mutation() {
    let case = fresh("budget-conflicting-copy");
    assert_success(&initialize(&case, &case.plan));
    let copy = case._dir.join("conflicting");
    copy_dir_recursive(&case.root, &copy);
    let plan = copy.join("plan.rhei.md");
    let original = fs::read_to_string(&plan).unwrap();
    fs::write(&plan, original.replace("Cycle through two states", "Different work")).unwrap();
    let refused =
        command(&case, "adjust", &plan, &["--spend-micro", "7000", "--reason", "conflicting copy"]);
    assert!(!refused.status.success());
    assert_stderr_contains(&refused, "conflicting live content");
    let shown = command(&case, "show", &case.plan, &["--format", "json"]);
    assert_success(&shown);
    let value: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(value["allowance"]["spend"]["amount_micro"], 6000);
}

#[test]
fn budget_adopting_identical_copy_retains_the_original_live_binding() {
    let case = fresh("budget-retain-original");
    assert_success(&initialize(&case, &case.plan));
    let copy = case._dir.join("adopted-copy");
    copy_dir_recursive(&case.root, &copy);
    let plan = copy.join("plan.rhei.md");
    assert_success(&command(
        &case,
        "adjust",
        &plan,
        &["--spend-micro", "7000", "--reason", "adopt identical copy"],
    ));
    let original = fs::read_to_string(&case.plan).unwrap();
    let changed = original.replace("Cycle through two states", "Conflicting original live work");
    assert_ne!(original, changed, "fixture must change the original work");
    fs::write(&case.plan, changed).unwrap();
    let refused = command(
        &case,
        "adjust",
        &plan,
        &["--spend-micro", "8000", "--reason", "must retain original binding"],
    );
    assert!(!refused.status.success());
    assert_stderr_contains(&refused, "conflicting live content");
    let shown = command(&case, "show", &plan, &["--format", "json"]);
    assert_success(&shown);
    let value: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(value["allowance"]["spend"]["amount_micro"], 7000);

    // A confirmed disappearance permits moving; it never replenishes history.
    fs::remove_file(&case.plan).unwrap();
    assert_success(&command(
        &case,
        "adjust",
        &plan,
        &["--spend-micro", "8000", "--reason", "original source is gone"],
    ));
}

#[test]
fn budget_moved_root_keeps_the_same_history_authority() {
    let mut case = fresh("budget-moved-root");
    assert_success(&initialize(&case, &case.plan));
    let before = command(&case, "show", &case.plan, &["--format", "json"]);
    let moved = case._dir.join("moved");
    fs::rename(&case.root, &moved).unwrap();
    case.root = moved;
    case.plan = case.root.join("plan.rhei.md");
    let shown = command(&case, "show", &case.plan, &["--format", "json"]);
    assert_success(&shown);
    assert_eq!(shown.stdout, before.stdout);
    assert_success(&command(
        &case,
        "adjust",
        &case.plan,
        &["--spend-micro", "7000", "--reason", "moved root"],
    ));
}

#[test]
fn budget_removed_ticket_identity_is_not_assigned_a_new_lifetime() {
    let case = fresh("budget-removed-ticket-id");
    assert_success(&initialize(&case, &case.plan));
    let raw = fs::read_to_string(&case.plan).unwrap();
    let removed =
        raw.lines().filter(|line| !line.contains("budgetTicketId")).collect::<Vec<_>>().join("\n");
    fs::write(&case.plan, removed).unwrap();
    let refused = run_release_case(&case, &[]);
    assert_eq!(spawn_count(&case), 0);
    assert!(combined(&refused).contains("lost its persistent budget identity"));
    assert!(!fs::read_to_string(&case.plan).unwrap().contains("budgetTicketId"));
}

#[test]
fn budget_retained_identity_with_missing_journal_never_initializes_again() {
    let case = fresh("budget-missing-history");
    assert_success(&initialize(&case, &case.plan));
    fs::remove_file(journal(&case)).unwrap();
    let refused = initialize(&case, &case.plan);
    assert!(!refused.status.success());
    assert_stderr_contains(&refused, "untrustworthy_ledger");
}

#[test]
fn budget_release_binary_refuses_fixture_documents_and_environment_switch() {
    let case = fresh("budget-release-bypass");
    assert_success(&initialize(&case, &case.plan));
    write_fixture_qualification(&case.root, [true, true, true, true]);
    let run = run_release_case(&case, &[]);
    assert!(!run.status.success());
    assert_eq!(spawn_count(&case), 0);
    assert!(combined(&run).contains("missing qualification"));
}

#[test]
fn budget_consumed_invocations_survive_complete_receipt_truncation() {
    let case = cycle_case(
        "budget-consumed-tail",
        Limits {
            invocations: 1,
            spend_micro: 2000,
            transition_limit: 3,
            threshold_micro: 2000,
            residual_micro: 0,
        },
        0,
    );
    let path = journal(&case);
    let initial = fs::read(&path).unwrap();
    let reservation = "reservation:00000000-0000-0000-0000-000000000001";
    append_receipt(
        &case.root,
        2,
        "receipt:00000000-0000-0000-0000-000000000002",
        "reserve",
        serde_json::json!({
            "reservation_id": reservation, "attempt_identity": "bounded-ambiguous-fixture",
            "ticket_identity": format!("ticket:{PROJECT_UUID}:{TICKET_UUID}"),
            "parent_reservation": null, "qualification": "rhei-test:fixed-2000:contained",
            "qualification_evidence_hash": "sha256:fixture", "invocation_units": 1,
            "threshold_micro": 2000, "residual_micro": 0, "fwc_micro": 2000, "travel_units": 1,
        }),
    );
    append_receipt(
        &case.root,
        3,
        "receipt:00000000-0000-0000-0000-000000000003",
        "start",
        serde_json::json!({
            "reservation_id": reservation, "status": "ambiguous",
        }),
    );
    let trusted = case._dir.join("ambiguous.jsonl");
    fs::copy(&path, &trusted).unwrap();
    fs::write(&path, initial).unwrap();
    assert!(!command(&case, "show", &case.plan, &["--format", "json"]).status.success());
    assert_success(&command(
        &case,
        "recover",
        &case.plan,
        &["--journal", trusted.to_str().unwrap(), "--reason", "retain ambiguous start"],
    ));
    let shown = command(&case, "show", &case.plan, &["--format", "json"]);
    assert_success(&shown);
    let value: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(value["consumed"]["invocations"], 1);
    assert_eq!(value["reserved"]["spend"]["amount_micro"], 2000);
    assert_eq!(value["remaining"]["invocations"], 0);
    assert_eq!(value["remaining"]["spend"]["amount_micro"], 0);
}

#[test]
fn budget_late_identity_assignment_debits_no_allowance() {
    let case = fresh("budget-late-identity");
    assert_success(&initialize(&case, &case.plan));
    let original = fs::read_to_string(&case.plan).unwrap();
    fs::write(&case.plan, format!("{original}\n### Task 2: Later work\n**State:** a\n")).unwrap();
    let run = run_release_case(&case, &[]);
    assert_eq!(spawn_count(&case), 0);
    assert!(combined(&run).contains("missing qualification"));
    assert_eq!(fs::read_to_string(&case.plan).unwrap().matches("budgetTicketId").count(), 2);
    let shown = command(&case, "show", &case.plan, &["--format", "json"]);
    assert_success(&shown);
    let value: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(value["remaining"]["invocations"], 3);
    assert_eq!(value["remaining"]["spend"]["amount_micro"], 6000);
}

#[test]
fn budget_cost_and_summary_include_lifetime_allowances_without_accounting() {
    let case = fresh("budget-read-surfaces");
    assert_success(&initialize(&case, &case.plan));
    let output = rhei_command(fixture_home(&case.root))
        .args(["cost"])
        .arg(&case.plan)
        .arg("--json")
        .output()
        .unwrap();
    let cost = CliRun::from(&output);
    assert_success(&cost);
    let value: serde_json::Value = serde_json::from_str(&cost.stdout).unwrap();
    assert_eq!(value["budget"]["allowance"]["invocations"], 3);
    assert_eq!(value["budget"]["remaining"]["spend"]["amount_micro"], 6000);
    let output = rhei_command(fixture_home(&case.root))
        .arg("--state-machine")
        .arg(&case.machine)
        .arg("summary")
        .arg(&case.plan)
        .output()
        .unwrap();
    let summary = CliRun::from(&output);
    assert_success(&summary);
    assert!(summary.stdout.contains("Panta lifetime budget"), "{}", summary.stdout);
    assert!(summary.stdout.contains("6000 micro-USD"), "{}", summary.stdout);
}

/// A refusal before any worker starts still owns a structural event log and
/// a retained report naming the budget reason. §FS-rhei-budgets.10
#[test]
fn budget_preflight_refusal_is_retained_in_the_run_event_stream_and_report() {
    let case = fresh("budget-retained-refusal");
    assert_success(&initialize(&case, &case.plan));
    let run = run_release_case(&case, &["--json", "--no-dashboard"]);
    assert!(!run.status.success());
    assert_eq!(spawn_count(&case), 0);
    let records = run
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(records.iter().any(|r| r["event"] == "run_started"));
    let snapshot =
        records.iter().find(|r| r["event"] == "budget_snapshot").expect("typed snapshot");
    assert_eq!(snapshot["budget"]["ledger_health"], "verified");
    assert_eq!(snapshot["budget"]["remaining"]["spend"]["amount_micro"], 6000);
    assert!(records
        .iter()
        .any(|r| r["event"] == "budget_halt" && r["reason_code"] == "missing_qualification"));
    let events = fs::read_to_string(case.root.join("runtime/events.jsonl")).unwrap();
    assert!(events.contains("budget_halt"));
    let report = fs::read_to_string(case.root.join("runtime/run-report.md")).unwrap();
    assert!(report.contains("budget admission halted"));
    assert!(report.contains("missing_qualification"));
    assert!(report.contains("Budget snapshots and receipts"));
}
