//! A built-in-machine ticket actually worked by this run must describe that
//! work in Attention, even though an unworked ticket has a manual-only shape.
//! Ports the two-ticket reproducer for agent-grounds/rhei#597; the bound-refused
//! sibling belongs to #587 and is evidence here, not the row under test.
// §FS-rhei-run-report.3.1 §FS-rhei-run.3

use std::fs;

use super::budget_support::{
    agent_defaults, assert_plan_but_for_the_budget_identity, home_for, spawns,
    write_machine_settings,
};
use super::*;

const PLAN: &str = "# Rhei: Manual\n\n## Tasks\n\n\
    ### Task 1: A\n**State:** pending\n\n### Task 2: B\n**State:** pending\n";

const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawn-count.log', '{}\n'.format(env('RHEI_TASK_ID')))
"#;

fn setup(default_agent: bool, bound: bool) -> TestDir {
    let dir = unique_temp_dir("worked-pending-attention");
    write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let settings = if default_agent {
        let agent = write_python_agent(&dir, "mock-agent.py", AGENT);
        agent_defaults(&agent, if bound { r#", "invocations_per_day": 1"# } else { "" })
    } else {
        "{}".to_string()
    };
    write_machine_settings(&dir, &settings);
    dir
}

fn run(dir: &std::path::Path, no_agent: bool) -> CliRun {
    let mut cmd = rhei_command(home_for(dir));
    cmd.current_dir(dir)
        .env("XDG_CONFIG_HOME", home_for(dir).join(".config"))
        .env("RHEI_BUDGET_NOW", "2026-09-24T12:00:00Z")
        .args(["run", "plan.rhei.md", "--no-tui", "--no-callbacks"]);
    if no_agent {
        cmd.arg("--no-agent");
    }
    CliRun::from(&cmd.output().expect("run the built-in machine"))
}

fn attention_row(dir: &std::path::Path, task: &str) -> String {
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    let attention = report.split("## Attention\n").nth(1).expect("Attention section");
    attention
        .split("\n## ")
        .next()
        .expect("Attention body")
        .lines()
        .find(|line| line.starts_with(&format!("| {task} |")))
        .unwrap_or_else(|| panic!("no Attention row for {task}:\n{report}"))
        .to_string()
}

fn assert_worked_pending(bound: bool) {
    let dir = setup(true, bound);
    let result = run(&dir, false);
    assert!(!result.status.success(), "missing results halt the run");
    let expected = if bound { vec!["plan.1"] } else { vec!["plan.1", "plan.2"] };
    assert_eq!(spawns(&dir), expected, "the agent must have run: {}", result.stderr);
    let plan = fs::read_to_string(dir.join("plan.rhei.md")).expect("unchanged task states");
    assert_plan_but_for_the_budget_identity(&plan, PLAN, "both tickets remain pending");
    for task in &expected {
        assert!(!dir.join(format!("runtime/results/{task}.md")).exists(), "mock owes its result");
    }
    let events = fs::read_to_string(dir.join("runtime/events.jsonl")).expect("event journal");
    let halts: Vec<serde_json::Value> = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON event"))
        .filter(|event| event["event"] == "budget_halt")
        .collect();
    assert!(
        halts.iter().all(|event| event["task"] != "plan.1"),
        "plan.1 was worked, not refused: {halts:?}"
    );
    if bound {
        assert!(!halts.is_empty(), "the sibling must have hit the invocation bound");
        assert!(
            halts.iter().all(|event| event["task"] == "plan.2"),
            "only the unspawned sibling hit the bound: {halts:?}"
        );
    } else {
        assert!(halts.is_empty(), "no bound explains either ticket: {halts:?}");
    }
    for task in expected {
        let row = attention_row(&dir, task);
        assert!(
            row.contains("worker exited 0 without result")
                // The row prints the host's native path, so only this comparison ignores the separator.
                && row.replace('\\', "/").contains(&format!("runtime/results/{task}.md")),
            "{task} was spawned, remains pending, owes its result and has no budget halt; expected the missing-result reason: {row}"
        );
        assert!(
            row.contains("write the file(s)") && row.contains("rerun"),
            "missing-output recovery: {row}"
        );
        assert!(
            row.contains("rhei transition") && row.contains("--result"),
            "outcome-recording recovery: {row}"
        );
        for wrong in ["manual-only", "rhei next", "rhei complete", "inspect logs", "cancelled"] {
            assert!(!row.contains(wrong), "worked ticket must not get {wrong:?}: {row}");
        }
    }
}

#[test]
fn issue_597_worked_pending_without_bound_reports_missing_result() {
    assert_worked_pending(false);
}

#[test]
fn issue_597_worked_pending_beside_bound_refused_sibling_reports_missing_result() {
    assert_worked_pending(true);
}

#[test]
fn issue_597_unworked_pending_keeps_manual_refusal() {
    for (default_agent, no_agent) in [(false, false), (true, true)] {
        let dir = setup(default_agent, false);
        let result = run(&dir, no_agent);
        assert!(!result.status.success(), "callback-only must refuse fresh work");
        assert!(spawns(&dir).is_empty(), "no agent may run in this control");
        assert_eq!(fs::read_to_string(dir.join("plan.rhei.md")).expect("plan"), PLAN);
        assert!(result.stderr.contains("manual-only"), "truthful refusal: {}", result.stderr);
        let row = attention_row(&dir, "plan.1");
        for expected in ["manual-only", "rhei next", "rhei complete"] {
            assert!(row.contains(expected), "manual recovery includes {expected:?}: {row}");
        }
    }
}
