//! Whose ceiling a bound is clamped to, and what every sentence about it names.
//!
//! The machine's value is the ceiling unless the machine delegated a count
//! dimension to the project, in which case the project's declared value takes
//! its place whether higher or lower, and a plan may still lower it but never
//! raise it. Every phrase, row and remedy prints from that one fact.
//! §FS-rhei-budgets.2 §FS-rhei-budgets.2.3 §FS-rhei-budgets.8

use std::path::PathBuf;

use super::*;

fn files() -> SettingsFiles {
    SettingsFiles {
        machine: Some(PathBuf::from("/home/me/.config/rhei/settings.json")),
        project: Some(PathBuf::from("/work/forge/.agent-grounds/rhei/settings.json")),
    }
}

fn plan(value: u64) -> Option<(u64, BoundSource)> {
    Some((value, BoundSource::Plan))
}

fn project(value: u64) -> Option<(u64, BoundSource)> {
    Some((value, BoundSource::Project))
}

/// A delegated ceiling above the machine's is the bound, unclamped.
/// §FS-rhei-budgets.2
#[test]
fn a_delegated_ceiling_above_the_machines_is_the_bound() {
    let bound = Bound::resolve_delegated("transition_limit", 1000, project(1000));
    assert_eq!(
        (bound.effective, bound.source, bound.requested),
        (1000, BoundSource::Project, None)
    );
    assert_eq!(bound.limiter, Limiter::Project);
    assert!(bound.delegated());
    assert_eq!(bound.value_phrase(), "1000 (project)");
}

/// Below the machine's, it is still the ceiling: delegation replaces the
/// machine's number in both directions, so a plan above it is clamped to it
/// and the project is named as the limiter. §FS-rhei-budgets.2 §FS-rhei-budgets.2.2
#[test]
fn a_delegated_ceiling_below_the_machines_clamps_a_plan_and_names_the_project() {
    let bound = Bound::resolve_delegated("transition_limit", 40, plan(60));
    assert_eq!((bound.effective, bound.requested), (40, Some(60)));
    assert_eq!(bound.value_phrase(), "40 (requested 60 by the plan, limited by project settings)");
    // The same 80 / 40 / 60 on a machine that does not delegate is 60.
    let undelegated = Bound::resolve("transition_limit", 80, None, plan(60));
    assert_eq!(undelegated.value_phrase(), "60 (plan)");
}

/// A plan asking exactly the delegated ceiling is honoured, not clamped; one
/// asking less wins. §FS-rhei-budgets.2.2
#[test]
fn a_plan_at_or_below_the_delegated_ceiling_is_honoured() {
    let equal = Bound::resolve_delegated("transition_limit", 40, plan(40));
    assert_eq!((equal.effective, equal.source, equal.requested), (40, BoundSource::Plan, None));
    let lower = Bound::resolve_delegated("transition_limit", 40, plan(20));
    assert_eq!(lower.value_phrase(), "20 (plan)");
    assert_eq!(lower.ceiling, 40);
}

/// Without delegation the ceiling is the machine's and nothing reports a
/// delegated ceiling. §FS-rhei-budgets.2.3
#[test]
fn an_undelegated_bound_has_no_ceiling_row_or_policy() {
    let bound = Bound::resolve("transition_limit", 80, Some(40), project(100)).with_files(files());
    assert_eq!(bound.limiter, Limiter::Machine);
    assert_eq!(bound.ceiling, 40);
    assert_eq!(
        bound.value_phrase(),
        "40 (requested 100 by the project, limited by machine settings)"
    );
    assert_eq!(bound.ceiling_line(), None);
    assert_eq!(bound.ceiling_halt_phrase(None), None);
    assert_eq!(ceiling_policy_line([&bound]), None);
}

/// Spend keeps the machine as its ceiling: its resolution has no delegated
/// form at all. §FS-rhei-budgets.2.1
#[test]
fn spend_resolves_under_the_machine_only() {
    let bound = Bound::resolve_money("spend_per_day", 400, Some(25), project(1000));
    assert_eq!(bound.limiter, Limiter::Machine);
    assert!(!bound.delegated());
}

/// The rows of §FS-rhei-budgets.2.3: one ceiling line per delegated key, and
/// one policy line listing only the delegated keys.
#[test]
fn the_ceiling_and_policy_rows_name_the_files_read() {
    let travel = Bound::resolve_delegated("transition_limit", 40, plan(20)).with_files(files());
    let per_day =
        Bound::resolve_delegated("invocations_per_day", 500, project(500)).with_files(files());
    let undelegated = Bound::resolve("invocations_per_day", 200, None, None).with_files(files());
    assert_eq!(
        travel.ceiling_line().as_deref(),
        Some(
            "transition_limit ceiling: 40 (project, /work/forge/.agent-grounds/rhei/settings.json)"
        )
    );
    assert_eq!(
        ceiling_policy_line([&travel, &per_day]).as_deref(),
        Some(
            "count ceiling policy: delegated transition_limit, invocations_per_day by \
             defaults.clamp_projects=false in /home/me/.config/rhei/settings.json"
        )
    );
    assert_eq!(
        ceiling_policy_line([&travel, &undelegated]).as_deref(),
        Some(
            "count ceiling policy: delegated transition_limit by \
             defaults.clamp_projects=false in /home/me/.config/rhei/settings.json"
        )
    );
}

/// One remedy per limiter, each naming its file by path: the machine file when
/// the machine's ceiling limits, the project file when the project's does or
/// when an unclamped project value is the bound, and the profile otherwise.
/// §FS-rhei-budgets.8
#[test]
fn the_remedy_names_the_file_of_whoever_limits() {
    let machine = Bound::resolve("transition_limit", 80, Some(4), project(6)).with_files(files());
    assert_eq!(
        machine.remedy(),
        "set `defaults.transition_limit` in /home/me/.config/rhei/settings.json"
    );
    let delegated = Bound::resolve_delegated("transition_limit", 6, project(6)).with_files(files());
    assert_eq!(
        delegated.remedy(),
        "set `defaults.transition_limit` in /work/forge/.agent-grounds/rhei/settings.json"
    );
    let clamped_by_project =
        Bound::resolve_delegated("transition_limit", 3, plan(6)).with_files(files());
    assert_eq!(
        clamped_by_project.remedy(),
        "set `defaults.transition_limit` in /work/forge/.agent-grounds/rhei/settings.json"
    );
    let project_below =
        Bound::resolve("transition_limit", 80, None, project(6)).with_files(files());
    assert_eq!(
        project_below.remedy(),
        "set `defaults.transition_limit` in /work/forge/.agent-grounds/rhei/settings.json"
    );
    let profile = Bound::resolve_delegated("transition_limit", 6, plan(3)).with_files(files());
    assert_eq!(
        profile.remedy(),
        "set `transition_limit` on the node's profile in the state machine"
    );
    // A bound that knows no path names the file by its role.
    let unnamed = Bound::resolve("transition_limit", 80, None, None);
    assert_eq!(unnamed.remedy(), "set `defaults.transition_limit` in the machine settings file");
}

/// The halt gains its `ceiling:` row directly after `bound:`, and only where
/// the dimension was delegated. §FS-rhei-budgets.8
#[test]
fn a_delegated_halt_carries_the_ceiling_row_after_the_bound() {
    let spent = Exhaustion {
        dimension: Dimension::Travel,
        subject: "forge.3".into(),
        bound: 20,
        counter: Counter { consumed: 20, reserved: 0 },
        contract: Contract::Window,
        day: "2026-10-06".into(),
        currency: None,
    };
    let none = SpendMarks::default();
    let delegated = Bound::resolve_delegated("transition_limit", 40, plan(20)).with_files(files());
    let text = halt_text(&spent, &delegated, &Remedy::Raise, &none);
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(rows[2], "       bound:       20 (plan)");
    assert_eq!(
        rows[3],
        "       ceiling:     40 (project, /work/forge/.agent-grounds/rhei/settings.json; \
         delegated by defaults.clamp_projects=false in /home/me/.config/rhei/settings.json)"
    );
    assert!(rows[4].starts_with("       consumed:"), "got:\n{text}");

    let machine = Bound::resolve("transition_limit", 80, Some(20), None).with_files(files());
    let text = halt_text(&spent, &machine, &Remedy::Raise, &none);
    assert!(!text.contains("ceiling:"), "nothing was delegated; got:\n{text}");
}
