//! Lifetime allowances report their audited account and an effective next action.
//! Port of the issue-579 two-ticket reproducer; Attention is a separate defect.
//! §FS-rhei-budgets.2.3 §FS-rhei-budgets.3.2 §FS-rhei-budgets.8
//! §FS-rhei-budgets.9 §FS-rhei-run-json.2.1 §FS-rhei-run-report.3.1

use super::budget_delegated_support::{budget, machine_file, project_file, write_project_settings};
use super::budget_support::*;
use super::*;
use rhei_core::platform::shell_quote;
use serde_json::Value;

const MACHINE: &str = "name: lifetime-reporting\nversion: 1\nstates:\n  work:\n    initial: true\n    description: Work once\n    agent: mock\n    agent_timeout: 30s\n  completed:\n    description: Done\n    final: true\ntransitions:\n  - { from: work, to: completed, description: Done }\n";

fn fixture(extra: &str, tickets: usize) -> (TestDir, PathBuf, PathBuf) {
    let (dir, plan, machine) =
        setup_with_agent("rhei-579-lifetime-reporting", MACHINE, FINISHING_AGENT, extra);
    let mut body = "# Rhei: Lifetime allowance\n\n## Tasks\n".to_string();
    for n in 1..=tickets {
        body.push_str(&format!("\n### Task {n}: Ticket {n}\n**State:** work\n"));
    }
    fs::write(&plan, body).unwrap();
    (dir, plan, machine)
}

fn grant(plan: &Path, verb: &str, n: u64) {
    assert_success(&budget(
        plan,
        &[verb, "--invocations", &n.to_string(), "--reason", "issue579"],
        None,
    ));
}

fn records(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("runtime/events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("journal record"))
        .collect()
}

fn invocation_bounds(records: &[Value]) -> Vec<&Value> {
    let mut found = Vec::new();
    for record in records {
        if record["event"] == "budget_snapshot" {
            found.extend(
                record["bounds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|bound| bound["dimension"] == "project invocations"),
            );
        } else if record["event"] == "budget_halt" && record["dimension"] == "project invocations" {
            found.push(record);
        }
    }
    assert!(!found.is_empty(), "must inspect real invocation records");
    found
}

fn halt(records: &[Value]) -> &Value {
    records
        .iter()
        .rev()
        .find(|r| r["event"] == "budget_halt")
        .unwrap_or_else(|| panic!("lifetime halt missing: {records:?}"))
}

fn adjust_remedy(plan: &Path) -> String {
    format!(
        "run `rhei budget adjust {} --invocations <N> --reason <TEXT>`",
        shell_quote(&plan.display().to_string())
    )
}

fn assert_halted(result: &CliRun, records: &[Value], allowance: u64) {
    assert_eq!(result.status.code(), Some(1), "{}", result.stderr);
    let h = halt(records);
    assert_eq!(h["reason_code"], "invocation_exhausted");
    assert_eq!(h["mode"], "lifetime");
    assert_eq!(h["effective"], allowance);
    assert_eq!(h["consumed"], allowance);
    assert_eq!(h["outstanding"], 0);
    assert_eq!(h["remaining"], 0);
    assert_eq!(h["task"], format!("plan.{}", allowance + 1));
}

// §FS-rhei-budgets.3.2 §FS-rhei-budgets.9 §REQ-bounded-neural-work.4
#[test]
fn issue579_lifetime_account_provenance_survives_daily_edit_and_adjustment() {
    let (dir, plan, machine) = fixture("", 2);
    grant(&plan, "init", 1);
    let first = run_plan(&plan, &machine, None);
    let first_records = records(&dir);
    assert_halted(&first, &first_records, 1);
    assert_spawn_count(&dir, 1, "the first ticket spends one invocation");
    let first_report = fs::read_to_string(dir.join("runtime/run-report.md")).unwrap();

    let agent = dir.join("mock-agent.py");
    write_machine_settings(&dir, &agent_defaults(&agent, r#", "invocations_per_day": 50"#));
    let again = run_plan(&plan, &machine, None);
    let again_records = records(&dir);
    assert_halted(&again, &again_records, 1);
    assert_spawn_count(&dir, 1, "a daily edit grants no lifetime capacity");
    let again_report = fs::read_to_string(dir.join("runtime/run-report.md")).unwrap();

    grant(&plan, "adjust", 10);
    assert_success(&run_plan(&plan, &machine, None));
    assert_spawn_count(&dir, 2, "adjustment preserves the consumed first invocation");
    let listed = run_at("list", &plan, &machine, None, &[]);
    assert_success(&listed);
    for n in 1..=2 {
        assert!(
            listed.stdout.contains(&format!("Task plan.{n}: Ticket {n} [completed]")),
            "{}",
            listed.stdout
        );
    }
    for run in [&first_records, &again_records] {
        for bound in invocation_bounds(run) {
            assert_eq!(bound["value_source"], "account", "audited allowance provenance: {bound}");
            assert!(bound["limiting_source"].is_null(), "{bound}");
            assert!(bound.get("ceiling").is_none(), "{bound}");
            assert_eq!(bound["effective"], 1);
            assert_eq!(bound["mode"], "lifetime");
            assert!(bound["window"].is_null(), "{bound}");
            let total = bound["consumed"].as_u64().unwrap()
                + bound["outstanding"].as_u64().unwrap()
                + bound["remaining"].as_u64().unwrap();
            assert_eq!(total, 1, "snapshot amounts: {bound}");
        }
        assert_eq!(
            halt(run)["remedy"],
            adjust_remedy(&plan),
            "daily edit preserves the effective remedy"
        );
    }
    for (result, report) in [(&first, first_report), (&again, again_report)] {
        assert!(result.stderr.contains("1 (account)"), "{}", result.stderr);
        assert!(report.contains("| project invocations | 1 (account) | lifetime |"), "{report}");
        assert!(
            report.contains("was stopped by its project invocations bound of 1 (account);"),
            "{report}"
        );
    }
    let final_records = records(&dir);
    let end = invocation_bounds(&final_records).last().unwrap().to_owned();
    assert_eq!(end["effective"], 10);
    assert_eq!(end["consumed"], 2);
    assert_eq!(end["remaining"], 8);
    assert_eq!(end["value_source"], "account");
}

// §FS-rhei-budgets.8 §FS-rhei-run-report.3.1
#[test]
fn issue579_lifetime_remedy_is_the_target_adjustment_on_all_bounds_surfaces() {
    let (dir, plan, machine) = fixture("", 2);
    grant(&plan, "init", 1);
    let result = run_plan(&plan, &machine, None);
    let rows = records(&dir);
    assert_halted(&result, &rows, 1);
    let expected = adjust_remedy(&plan);
    assert!(result.stderr.contains(&expected), "{}", result.stderr);
    assert_eq!(halt(&rows)["remedy"], expected, "saved lifetime remedy");
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).unwrap();
    let line = report.lines().find(|l| l.starts_with("- `plan.2` was stopped by")).unwrap();
    assert!(line.ends_with(&format!("; {expected}")), "{line}");
}

// §FS-rhei-budgets.2.3 §FS-rhei-run-json.2.1
#[test]
fn issue579_lifetime_records_have_no_delegated_daily_ceiling() {
    let (dir, plan, machine) = fixture(r#", "clamp_projects": false"#, 2);
    write_project_settings(
        &dir,
        ".agent-grounds/rhei",
        r#"{"defaults":{"invocations_per_day":50}}"#,
    );
    grant(&plan, "init", 1);
    let result = run_plan(&plan, &machine, None);
    let rows = records(&dir);
    assert_halted(&result, &rows, 1);
    for bound in invocation_bounds(&rows) {
        assert!(
            bound.get("ceiling").is_none(),
            "daily delegation leaked into lifetime bound: {bound}"
        );
        assert!(bound["limiting_source"].is_null(), "{bound}");
        assert_eq!(bound["value_source"], "account");
    }
    for record in &rows {
        if let Some(keys) = record["ceiling_policy"]["delegated"].as_array() {
            assert!(!keys.iter().any(|key| key == "invocations_per_day"), "{record}");
        }
    }
}

// §FS-rhei-budgets.2.3 §FS-rhei-budgets.3.2
#[test]
fn issue579_lifetime_records_have_no_daily_limiter_after_a_clamped_grant() {
    let (dir, plan, machine) =
        fixture(r#", "invocations_per_day": 1, "invocation_lifetime_max": 2"#, 3);
    write_project_settings(
        &dir,
        ".agent-grounds/rhei",
        r#"{"defaults":{"invocations_per_day":50}}"#,
    );
    grant(&plan, "init", 20); // historical grant clamp: only two were granted
    let result = run_plan(&plan, &machine, None);
    let rows = records(&dir);
    assert_halted(&result, &rows, 2);
    assert_spawn_count(&dir, 2, "the daily setting does not reclamp the lifetime grant");
    for bound in invocation_bounds(&rows) {
        assert!(
            bound["limiting_source"].is_null(),
            "daily limiter leaked into lifetime bound: {bound}"
        );
        assert_eq!(bound["value_source"], "account");
        assert!(bound.get("ceiling").is_none(), "{bound}");
    }
}

// §FS-rhei-budgets.8 §REQ-bounded-neural-work.4
fn ceiling_control(project: bool) {
    let mut failures = Vec::new();
    for allowance in [1, 2] {
        // equal to, then above, the newly lowered ceiling
        let (dir, plan, machine) = fixture(r#", "invocation_lifetime_max": 10"#, allowance + 1);
        grant(&plan, "init", allowance as u64);
        if project {
            write_project_settings(
                &dir,
                ".agent-grounds/rhei",
                r#"{"defaults":{"invocation_lifetime_max":1}}"#,
            );
        } else {
            write_machine_settings(
                &dir,
                &agent_defaults(&dir.join("mock-agent.py"), r#", "invocation_lifetime_max": 1"#),
            );
        }
        let stopped = run_plan(&plan, &machine, None);
        let rows = records(&dir);
        assert_halted(&stopped, &rows, allowance as u64);
        // A smaller present ceiling must not reclamp a historical grant.
        assert_spawn_count(&dir, allowance, "stored allowance remains the admission bound");
        let capped =
            budget(&plan, &["adjust", "--invocations", "10", "--reason", "issue579"], None);
        if allowance == 1 {
            assert_success(&capped);
        } else {
            assert!(!capped.status.success(), "a grant below consumed work is refused");
        }
        assert_eq!(run_plan(&plan, &machine, None).status.code(), Some(1));
        assert_spawn_count(&dir, allowance, "a capped adjustment cannot release the halt");
        let file =
            if project { project_file(&dir, ".agent-grounds/rhei") } else { machine_file(&dir) };
        let expected = format!(
            "set `defaults.invocation_lifetime_max` in {} above {allowance}, then {}",
            file.display(),
            adjust_remedy(&plan)
        );
        let report = fs::read_to_string(dir.join("runtime/run-report.md")).unwrap();
        if halt(&rows)["remedy"] != expected
            || !stopped.stderr.contains(&expected)
            || !report.lines().any(|line| line.starts_with("- `plan.") && line.ends_with(&expected))
        {
            failures.push(format!(
                "allowance {allowance}: expected {expected:?}; event remedy={} ; stderr={}",
                halt(&rows)["remedy"],
                stopped.stderr
            ));
        }
        if project {
            write_project_settings(
                &dir,
                ".agent-grounds/rhei",
                r#"{"defaults":{"invocation_lifetime_max":10}}"#,
            );
        } else {
            write_machine_settings(
                &dir,
                &agent_defaults(&dir.join("mock-agent.py"), r#", "invocation_lifetime_max": 10"#),
            );
        }
        assert_eq!(
            run_plan(&plan, &machine, None).status.code(),
            Some(1),
            "settings alone grant nothing"
        );
        grant(&plan, "adjust", 10);
        assert_success(&run_plan(&plan, &machine, None));
        assert_spawn_count(&dir, allowance + 1, "ceiling then adjustment releases without refund");
    }
    assert!(failures.is_empty(), "ceiling-first remedy:\n{}", failures.join("\n"));
}

#[test]
fn issue579_lifetime_machine_ceiling_remedy_and_control() {
    ceiling_control(false);
}

#[test]
fn issue579_lifetime_project_ceiling_remedy_and_control() {
    ceiling_control(true);
}

// A refusal with zero new starts still reports the current account and all
// three Bounds rows, and all halt surfaces retain the same ordered remedy.
// §FS-rhei-budgets.8 §FS-rhei-budgets.9 §FS-rhei-run-report.3.1
fn assert_co_ceiling_halt(root: &Path, plan: &Path, machine: &Path, expected: &str) {
    let result = run_plan(plan, machine, None);
    let rows = records(root);
    assert_halted(&result, &rows, 1);
    assert_spawn_count(root, 1, "settings and capped adjustments start no remaining work");
    assert!(
        result.stderr.lines().any(|line| line.trim() == format!("to raise it: {expected}")),
        "{}",
        result.stderr
    );
    assert_eq!(halt(&rows)["remedy"], expected);
    let report = fs::read_to_string(root.join("runtime/run-report.md")).unwrap();
    assert!(
        report.contains("| project invocations | 1 (account) | lifetime | 1 | 1 | 0 | 0 |"),
        "{report}"
    );
    assert!(report.contains("| ticket travel |"), "{report}");
    assert!(report.contains("| project spend |"), "{report}");
    assert!(
        report.lines().any(|line| line.starts_with("- `plan.2` was stopped by")
            && line.ends_with(&format!("; {expected}"))),
        "{report}"
    );
    for bound in invocation_bounds(&rows) {
        assert_eq!(bound["value_source"], "account");
        assert!(bound["limiting_source"].is_null(), "{bound}");
        assert!(bound.get("ceiling").is_none(), "{bound}");
    }
}

// Port of review-1/edge_repro.py's two independent, equal grant constraints.
// §FS-rhei-budgets.8 §REQ-bounded-neural-work.4
#[test]
fn issue579_lifetime_co_limiting_ceilings_require_both_edits_then_adjustment() {
    let (dir, plan, machine) = fixture(r#", "invocation_lifetime_max": 1"#, 2);
    let set_project = |maximum| {
        write_project_settings(
            &dir,
            ".agent-grounds/rhei",
            &format!(r#"{{"defaults":{{"invocation_lifetime_max":{maximum}}}}}"#),
        )
    };
    let set_machine = |maximum| {
        write_machine_settings(
            &dir,
            &agent_defaults(
                &dir.join("mock-agent.py"),
                &format!(r#", "invocation_lifetime_max": {maximum}"#),
            ),
        )
    };
    let edit = |file: PathBuf| {
        format!("set `defaults.invocation_lifetime_max` in {} above 1", file.display())
    };
    let project_edit = edit(project_file(&dir, ".agent-grounds/rhei"));
    let machine_edit = edit(machine_file(&dir));
    let adjust = adjust_remedy(&plan);
    set_project(1);
    grant(&plan, "init", 1);
    let first = run_plan(&plan, &machine, None);
    let rows = records(&dir);
    assert_halted(&first, &rows, 1);
    let both = format!("{machine_edit}, {project_edit}, then {adjust}");
    assert_eq!(halt(&rows)["remedy"], both);
    assert!(first.stderr.contains(&format!("to raise it: {both}")), "{}", first.stderr);
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).unwrap();
    assert!(
        report.lines().any(|line| line.starts_with("- `plan.2` was stopped by")
            && line.ends_with(&format!("; {both}"))),
        "{report}"
    );
    assert_co_ceiling_halt(&dir, &plan, &machine, &both);

    set_project(10);
    let machine_only = format!("{machine_edit}, then {adjust}");
    assert_co_ceiling_halt(&dir, &plan, &machine, &machine_only);
    grant(&plan, "adjust", 10);
    assert_co_ceiling_halt(&dir, &plan, &machine, &machine_only);

    set_project(1);
    set_machine(10);
    let project_only = format!("{project_edit}, then {adjust}");
    assert_co_ceiling_halt(&dir, &plan, &machine, &project_only);
    grant(&plan, "adjust", 10);
    assert_co_ceiling_halt(&dir, &plan, &machine, &project_only);

    set_project(10);
    assert_co_ceiling_halt(&dir, &plan, &machine, &adjust);
    grant(&plan, "adjust", 10);
    assert_success(&run_plan(&plan, &machine, None));
    assert_spawn_count(&dir, 2, "both edits then adjustment complete only the remaining ticket");
    let final_records = records(&dir);
    let bounds = invocation_bounds(&final_records);
    let end = bounds.last().unwrap();
    assert_eq!(end["effective"], 10);
    assert_eq!(end["consumed"], 2);
    assert_eq!(end["remaining"], 8);
    assert_eq!(end["value_source"], "account");
}
