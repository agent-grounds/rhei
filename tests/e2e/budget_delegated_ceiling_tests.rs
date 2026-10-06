//! A paying machine may hand the two count ceilings to the project.
//!
//! The machine's value is the ceiling by default, and nothing a repository
//! checks in can change that. A machine that sets `defaults.clamp_projects:
//! false` in its own settings file makes each count bound the project declares
//! the ceiling in its place, one key at a time; a plan or profile may still
//! lower it and never raise it. Spend and the lifetime maximum are never
//! delegated, and the switch is refused anywhere but the machine file.

// §FS-rhei-budgets.2 §FS-rhei-budgets.2.1 §FS-rhei-budgets.2.3 §FS-rhei-budgets.8

use super::budget_delegated_support::*;
use super::budget_support::*;
use super::*;

/// The reproducer behind the issue, on a machine that never set the switch:
/// the project asks for more and still halts at the machine's number, and the
/// remedy names the machine file by the path it was read from.
// §FS-rhei-budgets.2 §FS-rhei-budgets.8
#[test]
fn a_machine_without_the_switch_still_halts_a_project_at_its_own_number() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-default",
        &profile_machine(None),
        r#", "transition_limit": 3"#,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 6 } }"#,
    );

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 3, "the machine's three moves, not the project's six");
    assert_halt_mentions(&result, "3 (requested 6 by the project, limited by machine settings)");
    let text = combined(&result);
    assert!(!text.contains("ceiling:"), "nothing was delegated, so no ceiling row; got:\n{text}");
    assert_line(
        &text,
        |_, m| format!("to raise it: set `defaults.transition_limit` in {m}"),
        &plan,
        &machine_file(&dir),
        "a settings remedy names the machine file by its path on every machine",
    );
}

/// The switch raises both count bounds to the project's numbers, and
/// validation says whose ceiling each one is and which machine file
/// delegated them.
// §FS-rhei-budgets.2.3 §FS-rhei-validate.4
#[test]
fn a_delegating_machine_validates_the_projects_numbers_with_ceiling_and_policy_rows() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-validate",
        &profile_machine(None),
        r#", "transition_limit": 3, "clamp_projects": false"#,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 6, "invocations_per_day": 500 } }"#,
    );

    let result = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&result);
    let text = combined(&result);
    for line in
        ["warning: transition_limit: 6 (project)", "warning: invocations_per_day: 500 (project)"]
    {
        assert!(
            text.contains(line),
            "the project's declared value is the bound: {line:?}; got:\n{text}"
        );
    }
    let p = project_file(&dir, ".agent-grounds/rhei");
    let m = machine_file(&dir);
    assert_line(
        &text,
        |p, _| format!("warning: transition_limit ceiling: 6 (project, {p})"),
        &p,
        &m,
        "travel ceiling row",
    );
    assert_line(
        &text,
        |p, _| format!("warning: invocations_per_day ceiling: 500 (project, {p})"),
        &p,
        &m,
        "daily ceiling row",
    );
    assert_line(
        &text,
        |_, m| {
            format!(
                "warning: count ceiling policy: delegated transition_limit, invocations_per_day \
                 by defaults.clamp_projects=false in {m}"
            )
        },
        &p,
        &m,
        "the policy row names both delegated keys and the machine file",
    );
}

/// The run goes past the machine's number up to the project's, the halt
/// carries the `ceiling:` row and names the project file as the remedy, and
/// `budget show` reports the same in text and JSON.
// §FS-rhei-budgets.2 §FS-rhei-budgets.8 §FS-rhei-budgets.10
#[test]
fn a_delegating_machine_runs_a_ticket_up_to_the_projects_bound_and_reports_it() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-run",
        &profile_machine(None),
        r#", "transition_limit": 3, "clamp_projects": false"#,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 6 } }"#,
    );
    let p = project_file(&dir, ".agent-grounds/rhei");
    let m = machine_file(&dir);

    let result = run_plan(&plan, &machine, Some(DAY));

    assert_spawn_count(&dir, 6, "the project's six moves, past the machine's three");
    assert_halt_mentions(&result, "bound:       6 (project)");
    let text = combined(&result);
    assert_line(
        &text,
        |p, m| {
            format!(
                "ceiling:     6 (project, {p}; delegated by defaults.clamp_projects=false in {m})"
            )
        },
        &p,
        &m,
        "the halt says whose ceiling stopped it and which machine delegated it",
    );
    assert_line(
        &text,
        |p, _| format!("to raise it: set `defaults.transition_limit` in {p}"),
        &p,
        &m,
        "the remedy names the project file, which holds the ceiling",
    );

    let shown = budget(&plan, &["show"], Some(DAY));
    assert_success(&shown);
    assert_line(
        &shown.stdout,
        |p, _| format!("transition_limit ceiling: 6 (project, {p})"),
        &p,
        &m,
        "show: ceiling row",
    );
    assert_line(
        &shown.stdout,
        |_, m| {
            format!("count ceiling policy: delegated transition_limit by defaults.clamp_projects=false in {m}")
        },
        &p,
        &m,
        "show: policy row",
    );

    let json = budget_json(&plan, Some(DAY));
    let travel = &json["bounds"]["transition_limit"];
    assert_eq!(travel["effective"], 6, "json: {json:#}");
    assert_eq!(travel["source"], "project", "json: {json:#}");
    assert_eq!(
        travel["ceiling"]["value"], 6,
        "a delegated key carries its ceiling; json: {json:#}"
    );
    assert_eq!(travel["ceiling"]["source"], "project", "json: {json:#}");
    assert!(
        spellings(&p).iter().any(|p| travel["ceiling"]["path"] == p.as_str()),
        "the ceiling names the project file read; json: {json:#}"
    );
    assert!(
        json["bounds"]["invocations_per_day"].get("ceiling").is_none(),
        "an undeclared key is not delegated and carries no ceiling; json: {json:#}"
    );
    assert_eq!(
        json["ceiling_policy"]["delegated"],
        serde_json::json!(["transition_limit"]),
        "json: {json:#}"
    );
    assert_eq!(json["ceiling_policy"]["setting"], "defaults.clamp_projects", "json: {json:#}");
    assert!(
        spellings(&m).iter().any(|m| json["ceiling_policy"]["path"] == m.as_str()),
        "the policy names the machine file; json: {json:#}"
    );
}

/// Under delegation the project's value is the ceiling, so a profile asking
/// above it is clamped and the limiter is the project: 80 / 40 / 60 is 40.
// §FS-rhei-budgets.2.2 §FS-rhei-budgets.2.3
#[test]
fn under_delegation_the_project_caps_a_profile_and_is_named_as_the_limiter() {
    let (_dir, plan, machine) = workspace(
        "budget-delegated-profile-validate",
        &profile_machine(Some(60)),
        DELEGATES,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 40 } }"#,
    );

    let result = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&result);
    assert!(
        result.stdout.contains(
            "transition_limit: 40 (requested 60 by the plan, limited by project settings)"
        ),
        "machine 80, project 40, profile 60 resolves to 40, limited by the project; got:\n{}",
        result.stdout
    );
}

/// The same cap at run time: a profile asking for 6 under a project ceiling
/// of 3 gets 3 moves, where a machine that does not delegate would give it 6.
// §FS-rhei-budgets.2.2
#[test]
fn under_delegation_a_profile_above_the_project_runs_to_the_projects_number() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-profile-run",
        &profile_machine(Some(6)),
        r#", "transition_limit": 8, "clamp_projects": false"#,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 3 } }"#,
    );

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 3, "the project's ceiling of 3 caps the profile's 6");
    assert_halt_mentions(&result, "3 (requested 6 by the plan, limited by project settings)");
}

/// A profile below the project's ceiling is honoured, and the halt still says
/// whose ceiling is in force.
// §FS-rhei-budgets.2.2 §FS-rhei-budgets.8
#[test]
fn under_delegation_a_profile_below_the_project_is_honoured() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-profile-lower",
        &profile_machine(Some(3)),
        r#", "transition_limit": 8, "clamp_projects": false"#,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 6 } }"#,
    );

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 3, "the profile's lower value wins over the project's ceiling");
    assert_halt_mentions(&result, "bound:       3 (plan)");
    assert_line(
        &combined(&result),
        |p, m| {
            format!(
                "ceiling:     6 (project, {p}; delegated by defaults.clamp_projects=false in {m})"
            )
        },
        &project_file(&dir, ".agent-grounds/rhei"),
        &machine_file(&dir),
        "the ceiling row follows the bound row",
    );
}

/// Each key is delegated on its own: a project that declares travel alone
/// leaves the daily bound under the machine's ceiling.
// §FS-rhei-budgets.2 §FS-rhei-budgets.2.3
#[test]
fn only_the_keys_a_project_declares_are_delegated() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-per-key",
        &profile_machine(None),
        DELEGATES,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "transition_limit": 100 } }"#,
    );

    let result = run_at("validate", &plan, &machine, None, &[]);

    assert_success(&result);
    let text = combined(&result);
    assert!(text.contains("warning: transition_limit: 100 (project)"), "got:\n{text}");
    assert!(text.contains("warning: invocations_per_day: 200 (built_in)"), "got:\n{text}");
    assert!(
        !text.contains("invocations_per_day ceiling"),
        "an undeclared key gets no ceiling row; got:\n{text}"
    );
    assert_line(
        &text,
        |_, m| {
            format!("warning: count ceiling policy: delegated transition_limit by defaults.clamp_projects=false in {m}\n")
        },
        &project_file(&dir, ".agent-grounds/rhei"),
        &machine_file(&dir),
        "the policy row lists transition_limit alone",
    );
}

/// The halt names the file rhei actually read, so a project still on the
/// deprecated home is told about that file and not the current one.
// §FS-rhei-budgets.8
#[test]
fn a_delegated_halt_names_the_deprecated_project_file_when_that_is_the_one_read() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-deprecated",
        &profile_machine(None),
        r#", "transition_limit": 8, "clamp_projects": false"#,
        ".agents/rhei",
        r#"{ "defaults": { "transition_limit": 3 } }"#,
    );
    let p = project_file(&dir, ".agents/rhei");
    let m = machine_file(&dir);

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 3, "the deprecated home's ceiling of 3 is in force");
    let text = combined(&result);
    assert_line(
        &text,
        |p, m| {
            format!(
                "ceiling:     3 (project, {p}; delegated by defaults.clamp_projects=false in {m})"
            )
        },
        &p,
        &m,
        "the ceiling row names the deprecated file read",
    );
    assert_line(
        &text,
        |p, _| format!("to raise it: set `defaults.transition_limit` in {p}"),
        &p,
        &m,
        "the remedy names the deprecated file read",
    );
}

/// A delegated daily raise is measured against the same day: the starts
/// already admitted stay consumed, and the raise adds only the difference.
// §FS-rhei-budgets.2 §FS-rhei-budgets.3.3 §REQ-bounded-neural-work.4
#[test]
fn a_mid_day_delegated_raise_is_counted_against_the_same_day() {
    let (dir, plan, machine) = workspace(
        "budget-delegated-mid-day",
        &profile_machine(None),
        r#", "invocations_per_day": 1, "clamp_projects": false"#,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "invocations_per_day": 2 } }"#,
    );

    let halted = run_plan(&plan, &machine, Some(DAY));
    assert_spawn_count(&dir, 2, "the project's delegated two starts, past the machine's one");
    assert_halt_mentions(&halted, "renews at:");

    write_project_settings(
        &dir,
        ".agent-grounds/rhei",
        r#"{ "defaults": { "invocations_per_day": 5 } }"#,
    );
    let json = budget_json(&plan, Some(DAY));
    assert_eq!(json["invocations"]["bound"], 5, "the raised bound is in force; json: {json:#}");
    assert_eq!(
        json["invocations"]["consumed"], 2,
        "the day's two starts stay consumed; json: {json:#}"
    );
    assert_eq!(
        json["invocations"]["remaining"], 3,
        "the raise adds the difference only; json: {json:#}"
    );

    let _ = run_plan(&plan, &machine, Some(DAY));
    assert_spawn_count(&dir, 5, "the next run on the same day admits the three that remain");
}

/// The switch does not reach the lifetime maximum: a project's higher value is
/// still capped by the machine, and its lower value still applies.
// §FS-rhei-budgets.2.1 §FS-rhei-budgets.10
#[test]
fn the_lifetime_maximum_is_not_delegated() {
    for (project_max, granted) in [(9000, 6000), (100, 100)] {
        let (_dir, plan, _machine) = workspace(
            "budget-delegated-lifetime",
            &profile_machine(None),
            DELEGATES,
            ".agent-grounds/rhei",
            &format!(r#"{{ "defaults": {{ "invocation_lifetime_max": {project_max} }} }}"#),
        );
        let result =
            budget(&plan, &["init", "--invocations", "8000", "--reason", "lifetime"], None);
        assert_success(&result);
        assert!(
            result.stdout.contains(&format!("lifetime invocation allowance: {granted}")),
            "project maximum {project_max} grants {granted}; got:\n{}",
            result.stdout
        );
    }
}
