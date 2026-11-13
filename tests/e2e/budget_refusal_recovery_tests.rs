//! Refusal help describes the current cause after real CLI recovery.
//! §FS-rhei-run.3.4

use super::budget_admission_halt_support::text;
use super::budget_refusal_recovery_support::Recovery;
use super::budget_support::ledger;
use super::*;

fn final_help(out: &CliRun) -> &str {
    out.stderr.rsplit_once("help:").expect("halt must give help").1
}

fn assert_ordinary_help(out: &CliRun) {
    let help = final_help(out);
    assert!(help.contains("every remaining task is blocked, gated, or assigned"), "{}", text(out));
    assert!(!help.contains("spawning was refused"), "{}", text(out));
}

fn arrival_round_trip(counted: bool) {
    let fixture = Recovery::new("arrival", counted);
    let out = fixture.run();
    assert!(text(&out).contains("no such reservation"), "{}", text(&out));
    let task = fixture.task(1);
    let returned = if counted { "write-2" } else { "write" };
    assert!(task.contains(&format!("**State:** {returned}\n")), "{task}");
    assert!(task.contains("**Assignee:** fixture-holder"), "{task}");
    let moves = ledger(&fixture.root);
    assert_eq!(moves.len(), 3, "only two CLI hops and the repair transition: {moves:?}");
    assert!(moves.iter().any(|line| line == "evidence.1 write@pause"), "{moves:?}");
    assert!(moves.iter().any(|line| line == "evidence.1 pause@write"), "{moves:?}");
    assert!(!fixture.root.join("spawned").exists(), "no agent can have run");
    let facts = fixture.facts();
    assert_eq!(facts["health"], "verified");
    for dimension in ["invocations", "lifetime_invocations", "spend"] {
        assert_eq!(facts[dimension]["consumed"], 0, "{facts}");
        assert_eq!(facts[dimension]["outstanding"], 0, "{facts}");
    }
    assert_ordinary_help(&out);
}

#[test]
fn issue671_uncounted_reentry_discards_previous_arrival_refusal() {
    arrival_round_trip(false);
}

#[test]
fn issue671_counted_reentry_keeps_ordinary_blockage_help() {
    arrival_round_trip(true);
}

/// The repair runs only after a real budget_halt; one local invocation before
/// it consumes the allowance, and one afterwards proves successful recovery.
fn recovered(action: &'static str) -> (Recovery, CliRun) {
    let fixture = Recovery::new(action, false);
    let out = fixture.run();
    let all = text(&out);
    assert!(all.contains("has spent its invocation allowance"), "{all}");
    assert!(all.contains("required outputs are missing"), "{all}");
    if matches!(action, "replace" | "allocation") {
        assert!(fixture.task(1).contains("**Model:** new"));
        assert!(fixture.root.join("runtime/logs/task-evidence.1-write-new.log").exists(), "{all}");
        assert!(!fixture.root.join("runtime/logs/task-evidence.1-write-old.log").exists(), "{all}");
    }
    assert!(fixture.task(1).contains("**State:** write\n"));
    assert_eq!(
        fs::read_to_string(fixture.root.join("spawned")).unwrap(),
        "evidence.0\nevidence.1\n",
        "{all}"
    );
    assert_eq!(fs::read_to_string(fixture.root.join("runtime/prime.txt")).unwrap(), "earned\n");
    let moves = ledger(&fixture.root);
    assert_eq!(moves.len(), 3, "only prime, prepare and repair may move: {moves:?}");
    assert!(!moves.iter().any(|line| line.starts_with("evidence.1 write@")), "{moves:?}");
    let events = fixture.events();
    assert!(events.iter().any(|e| e["event"] == "budget_halt" && e["task"] == "evidence.1"));
    let facts = fixture.facts();
    assert_eq!(facts["health"], "verified");
    let bound = if action == "fanout" { 2 } else { 10 };
    assert_eq!(facts["invocations"]["bound"], bound, "{facts}");
    assert_eq!(facts["invocations"]["consumed"], 2, "{facts}");
    assert_eq!(facts["invocations"]["outstanding"], 0, "{facts}");
    assert_eq!(facts["invocations"]["remaining"], bound - 2, "{facts}");
    (fixture, out)
}

#[test]
fn issue671_replaced_model_drops_obsolete_admission_refusal() {
    let (fixture, out) = recovered("replace");
    assert!(fixture.task(1).contains("**Model:** new"));
    assert!(fixture.root.join("runtime/logs/task-evidence.1-write-new.log").exists());
    assert!(!fixture.root.join("runtime/logs/task-evidence.1-write-old.log").exists());
    assert_ordinary_help(&out);
}

#[test]
fn issue671_same_model_recovery_clears_its_refusal() {
    let (fixture, out) = recovered("same");
    assert!(fixture.task(1).contains("**Model:** old"));
    assert!(fixture.root.join("runtime/logs/task-evidence.1-write-old.log").exists());
    assert_ordinary_help(&out);
}

#[test]
fn issue671_recovery_retains_still_required_fanout_sibling_refusal() {
    let (fixture, out) = recovered("fanout");
    // Both arms were refused before repair. Only old can buy the one new
    // allowance; new remains required and its admission still explains halt.
    let refusals = fixture
        .events()
        .into_iter()
        .filter(|e| e["event"] == "budget_halt" && e["task"] == "evidence.1")
        .count();
    assert!(refusals >= 2);
    assert!(fixture.root.join("runtime/logs/task-evidence.1-write-old.log").exists());
    assert!(!fixture.root.join("runtime/logs/task-evidence.1-write-new.log").exists());
    let help = final_help(&out);
    assert!(help.contains("spawning was refused") && help.contains("evidence.1"), "{}", text(&out));
    assert!(help.contains("has spent its invocation allowance"), "{}", text(&out));
}

#[test]
fn issue671_override_recovery_preserves_independent_allocation_refusal() {
    let (fixture, out) = recovered("allocation");
    let orphan = fixture.root.join("runtime/logs/task-evidence.3-write-blocked.log");
    assert_eq!(fs::read_to_string(orphan).unwrap(), "orphan\n");
    assert!(fixture.task(3).contains("**State:** write\n"));
    assert!(!fixture.task(3).contains("**Result:**"));
    let help = final_help(&out);
    assert!(help.contains("spawning was refused") && help.contains("evidence.3"), "{}", text(&out));
    assert!(!help.contains("evidence.1"), "obsolete model admission must be gone: {}", text(&out));
    assert!(!help.contains("has spent its invocation allowance"), "{}", text(&out));
}
