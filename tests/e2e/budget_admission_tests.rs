//! §FS-rhei-budgets.4 §FS-rhei-budgets.5 §FS-rhei-budgets.9

use super::budget_test_support::*;
use super::*;

fn assert_halts_after_permitted_work(
    case: &BudgetCase,
    expected_spawns: u64,
    expected_state: &str,
    reason: &str,
) {
    let run = run_case(case, &[]);
    let output = combined(&run);
    assert_eq!(
        spawn_count(case),
        expected_spawns,
        "the engine must permit work below the bound and refuse the next spawn; got:\n{output}"
    );
    assert!(!run.status.success(), "a budget-halted non-terminal run exits non-zero");
    assert!(output.contains(reason), "the halt must name its exact bound; got:\n{output}");
    assert_task_state(&case.plan, &case.machine, "1", expected_state);
}

/// Port of the triage cycle: attempts reset on every state visit, so only the
/// persistent project invocation allowance can stop spawn three.
#[test]
fn invocation_allowance_halts_before_the_next_forbidden_spawn() {
    let case = cycle_case(
        "budget-invocation-cycle",
        Limits {
            invocations: 2,
            spend_micro: 100_000,
            transition_limit: 20,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    assert_halts_after_permitted_work(
        &case,
        2,
        "a",
        "Panta invocation allowance exhausted (2 consumed + 0 reserved / 2)",
    );
}

/// Invocation and money have ample headroom, so the third cross-state move can
/// be refused only by lifetime ticket travel.
#[test]
fn ticket_travel_halts_before_the_next_forbidden_spawn() {
    let case = cycle_case(
        "budget-travel-cycle",
        Limits {
            invocations: 20,
            spend_micro: 100_000,
            transition_limit: 2,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    assert_halts_after_permitted_work(
        &case,
        2,
        "a",
        "ticket travel exhausted (2 applied + 0 reserved / 2)",
    );
}

/// Two fixed-cost fixture calls fit exactly; invocation and travel limits are
/// deliberately high so neither can conceal missing monetary enforcement.
#[test]
fn provider_spend_halts_before_the_next_forbidden_spawn() {
    let case = cycle_case(
        "budget-spend-cycle",
        Limits {
            invocations: 20,
            spend_micro: 4_000,
            transition_limit: 20,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
        4,
    );
    assert_halts_after_permitted_work(
        &case,
        2,
        "a",
        "Panta spend allowance exhausted (4000 consumed + 0 reserved / 4000 micro-USD)",
    );
}

/// Two scheduler slots race for the last invocation. Exactly one may own the
/// durable reservation; a check without an atomic reservation starts both.
#[test]
fn competing_parallel_admissions_cannot_double_spend_the_last_unit() {
    let case = parallel_case("budget-parallel-atomic", false);
    let run = run_case(&case, &["--parallel", "2"]);
    let output = combined(&run);
    assert_eq!(spawn_count(&case), 1, "only one competitor may start; got:\n{output}");
    assert!(!run.status.success(), "the other non-terminal ticket is budget-halted");
    assert!(output.contains("Panta invocation allowance exhausted"));
}

/// A two-arm fanout cannot start just one reviewer when only one invocation
/// remains: reserving every arm is one all-or-none transaction.
#[test]
fn fanout_reserves_every_arm_or_starts_none() {
    let case = parallel_case("budget-fanout-atomic", true);
    let run = run_case(&case, &["--parallel", "2"]);
    let output = combined(&run);
    assert_eq!(spawn_count(&case), 0, "partial fanout admission is forbidden; got:\n{output}");
    assert!(!run.status.success());
    assert!(output.contains("fanout requires 2 invocation units; only 1 remains"));
    assert_task_state(&case.plan, &case.machine, "1", "work");
}

/// Both scheduler entry points must pass their first reservation/start without
/// reacquiring a lock they still own. The helper kills a stalled fixture.
/// §FS-rhei-budgets.4 §FS-rhei-budgets.12
#[test]
fn budget_first_admission_completes_in_both_scheduler_modes() {
    for parallel in ["1", "2"] {
        let case = cycle_case(
            "budget-first-start",
            Limits {
                invocations: 1,
                spend_micro: 10000,
                transition_limit: 5,
                threshold_micro: 2000,
                residual_micro: 0,
            },
            2,
        );
        let run = run_case(&case, &["--parallel", parallel]);
        assert_eq!(spawn_count(&case), 1, "{}", combined(&run));
        assert!(!run.status.success());
        assert!(combined(&run).contains("invocation"));
        assert_task_state(&case.plan, &case.machine, "1", "b");
    }
}

/// A refused ticket cannot abort the pool before its independent peer runs.
/// §FS-rhei-budgets.9
#[test]
fn budget_ticket_halt_continues_independent_admissible_work() {
    let case = parallel_case("budget-continue-peer", false);
    let task = case.root.join("tasks/01-first.md");
    let raw =
        std::fs::read_to_string(&task).unwrap().replace("**State:** work", "**State:** expensive");
    std::fs::write(task, raw).unwrap();
    let raw = std::fs::read_to_string(&case.machine).unwrap()
        .replace("  completed:\n", "  expensive:\n    description: Too expensive\n    agent: codex\n    model: fixture\n    agent_timeout: 5s\n    concurrent: true\n    budget_threshold:\n      currency: USD\n      amount_micro: 20000\n  completed:\n")
        .replace("allowed: [work, completed]", "allowed: [work, expensive, completed]")
        .replace("transitions:\n", "transitions:\n  - from: expensive\n    to: completed\n");
    std::fs::write(&case.machine, raw).unwrap();
    let run = run_case(&case, &["--parallel", "2"]);
    assert!(!run.status.success());
    assert_eq!(spawn_count(&case), 1, "{}", combined(&run));
    assert_task_state(&case.plan, &case.machine, "1", "expensive");
    assert_task_state(&case.plan, &case.machine, "2", "completed");
}

/// Start failure must release locks while retaining uncertain financial exposure.
/// §FS-rhei-budgets.7
#[test]
fn budget_failed_first_spawn_does_not_strand_the_journal_lock() {
    let case = cycle_case(
        "budget-failed-start",
        Limits {
            invocations: 1,
            spend_micro: 10000,
            transition_limit: 5,
            threshold_micro: 2000,
            residual_micro: 0,
        },
        2,
    );
    let settings = case.root.join(".agent-grounds/rhei/settings.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    value["agents"]["codex"]["command"] =
        serde_json::json!(["rhei-no-such-budget-fixture-executable"]);
    std::fs::write(settings, serde_json::to_vec(&value).unwrap()).unwrap();
    let run = run_case(&case, &[]);
    assert!(!run.status.success());
    assert_eq!(spawn_count(&case), 0);
    let mut show = rhei_command(fixture_home(&case.root));
    show.args(["budget", "show"]).arg(&case.plan).args(["--format", "json"]);
    let shown = CliRun::from(&bounded_budget_output(&mut show));
    assert_success(&shown);
    let budget: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    assert_eq!(budget["consumed"]["invocations"], 1);
    assert_eq!(budget["reserved"]["spend"]["amount_micro"], 2000);
}

/// The normal event sink and frozen report preserve the same primary refusal.
/// §FS-rhei-budgets.10
#[test]
fn budget_fanout_refusal_retains_reason_and_run_finished() {
    let case = parallel_case("budget-fanout-report", true);
    let run = run_case(&case, &["--parallel", "2", "--json", "--no-dashboard"]);
    assert!(!run.status.success());
    assert_eq!(spawn_count(&case), 0);
    let events = run
        .stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|v| v["event"] == "budget_halt" && v["reason_code"] == "invocation_exhausted"));
    assert!(events.iter().any(|v| v["event"] == "run_finished"));
    let report = std::fs::read_to_string(case.root.join("runtime/run-report.md")).unwrap();
    assert!(report.contains("invocation_exhausted"));
    assert!(report.contains("fanout requires 2 invocation units; only 1 remains"));
}

/// All arms complete through the broker; their group consumes one travel unit.
/// §FS-rhei-budgets.5 §FS-rhei-budgets.7
#[test]
fn budget_successful_fanout_has_one_travel_owner() {
    let case = parallel_case("budget-fanout-permitted", true);
    seed_allowance(
        &case.root,
        &Limits {
            invocations: 2,
            spend_micro: 10000,
            transition_limit: 5,
            threshold_micro: 2000,
            residual_micro: 0,
        },
    );
    let run = run_case(&case, &["--parallel", "2"]);
    assert_success(&run);
    assert_eq!(spawn_count(&case), 2);
    let mut show = rhei_command(fixture_home(&case.root));
    show.args(["budget", "show"]).arg(&case.plan).args(["--format", "json"]);
    let shown = CliRun::from(&bounded_budget_output(&mut show));
    assert_success(&shown);
    let budget: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
    let ticket = format!("ticket:{PROJECT_UUID}:{TICKET_UUID}");
    assert_eq!(budget["travel"][&ticket]["consumed"], 1);
    assert_eq!(budget["consumed"]["invocations"], 2);
    assert_eq!(budget["consumed"]["spend"]["amount_micro"], 4000);
}

/// A crash after the central fsync recovers the same edge, then replay is a no-op.
/// §FS-rhei-budgets.7
#[test]
fn budget_transition_side_crash_recovers_without_another_invocation() {
    let case = cycle_case(
        "budget-central-crash",
        Limits {
            invocations: 1,
            spend_micro: 10000,
            transition_limit: 5,
            threshold_micro: 2000,
            residual_micro: 0,
        },
        2,
    );
    let mut command = rhei_process_at(budget_fixture_binary());
    command
        .env("HOME", fixture_home(&case.root))
        .env("XDG_STATE_HOME", fixture_home(&case.root).join("state"))
        .env("RHEI_FIXTURE_CRASH_AFTER_CENTRAL", "1")
        .arg("--state-machine")
        .arg(&case.machine)
        .arg("run")
        .arg(&case.plan)
        .args(["--no-tui", "--no-callbacks"]);
    let crash = CliRun::from(&bounded_budget_output(&mut command));
    assert_eq!(crash.status.code(), Some(95));
    for _ in 0..2 {
        let run = run_case(&case, &[]);
        assert!(!run.status.success());
        assert_eq!(spawn_count(&case), 1);
        let mut show = rhei_command(fixture_home(&case.root));
        show.args(["budget", "show"]).arg(&case.plan).args(["--format", "json"]);
        let shown = CliRun::from(&bounded_budget_output(&mut show));
        assert_success(&shown);
        let budget: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
        let ticket = format!("ticket:{PROJECT_UUID}:{TICKET_UUID}");
        assert_eq!(budget["travel"][&ticket]["consumed"], 1);
        assert_eq!(budget["travel"][&ticket]["reserved"], 0);
    }
}
