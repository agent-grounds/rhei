//! Agent invocations rhei spawned that left no accounting record: `rhei
//! summary` and `rhei cost` read coverage `partial` over them and count them,
//! rather than reporting `complete` over the records that happen to exist.
//! §FS-rhei-cost-accounting.6.2.1 §FS-rhei-summary.2.1 §FS-rhei-summary.2.3

use super::accounting_support::last_run_id;
use super::accounting_unrecorded_spawn_support::{
    control_workspace, coverage_row, spawn_workspace, SpawnWorkspace, NOFAMILY_MACHINE,
    NOFAMILY_PLAN,
};

/// Two tickets whose `implement` runs on a profile with no family: four agent
/// spawns, two records.
fn nofamily(prefix: &str) -> SpawnWorkspace {
    let workspace = spawn_workspace(prefix, NOFAMILY_PLAN, NOFAMILY_MACHINE);
    workspace.run();
    workspace.assert_shape(4, 2);
    workspace
}

/// The report's shape: a profile that declares no family is allowed to leave no
/// record (§FS-rhei-cost-accounting.3.2), but the run was still not fully seen.
// §FS-rhei-cost-accounting.6.2.1 §FS-rhei-summary.2.3
#[test]
fn a_summary_over_spawns_of_a_familyless_profile_reads_partial_and_counts_them() {
    let workspace = nofamily("unrecorded-nofamily-summary");

    let summary = workspace.summary_details();
    assert_eq!(
        coverage_row(&summary),
        "| coverage | Partial (2 of 4 agent invocations have no accounting record) |",
        "two of the four agent spawns have no record; got:\n{summary}"
    );
}

/// The lead line counts the known agent invocations and names the gap, so the
/// records alone are not presented as the run's invocation count.
// §FS-rhei-summary.2.1 §FS-rhei-summary.2.2
#[test]
fn a_summary_lead_and_steps_name_the_spawns_that_have_no_record() {
    let workspace = nofamily("unrecorded-nofamily-lead");

    let summary = workspace.summary_details();
    assert!(
        summary
            .contains("4 agent invocations across 1 model, 2 of them with no accounting record;"),
        "the lead line must count every known agent invocation; got:\n{summary}"
    );
    let unrecorded_steps = summary
        .lines()
        .filter(|line| {
            line.contains(" implement — cdx — ") && line.ends_with("no accounting record")
        })
        .count();
    assert_eq!(unrecorded_steps, 2, "each unrecorded spawn is a step; got:\n{summary}");
}

/// `rhei cost` reads the same records and must say the same thing, in text and
/// in JSON, while `invocation_count` keeps counting records.
// §FS-rhei-cost-accounting.8 §FS-rhei-cost-accounting.6.2.1
#[test]
fn rhei_cost_over_spawns_of_a_familyless_profile_reads_partial_and_counts_them() {
    let workspace = nofamily("unrecorded-nofamily-cost");

    let text = workspace.cost_text(&[]);
    assert!(
        text.contains(
            "| Coverage Partial (2 of 4 agent invocations have no accounting record) | Invocations 2"
        ),
        "the totals line must name the gap; got:\n{text}"
    );

    let json = workspace.cost_json(&[]);
    let summary = &json["summary"];
    assert_eq!(summary["coverage"], "partial", "summary: {summary:#}");
    assert_eq!(
        summary["invocation_count"], 2,
        "invocation_count still counts records: {summary:#}"
    );
    assert_eq!(summary["unrecorded_agent_invocation_count"], 2, "summary: {summary:#}");
}

/// The plan-tree axis places a spawn by its `task`, and the run axis treats a
/// spawn, which names no run, as it treats an unattributed record.
// §FS-rhei-cost-accounting.6.2.1
#[test]
fn rhei_cost_selections_place_an_unrecorded_spawn_by_its_task_and_doubt_the_run() {
    let workspace = nofamily("unrecorded-nofamily-selections");

    let task = workspace.cost_json(&["--task", "plan.1"]);
    let direct = &task["task"]["direct"];
    assert_eq!(direct["coverage"], "partial", "plan.1's implement has no record: {task:#}");
    assert_eq!(direct["unrecorded_agent_invocation_count"], 1, "direct: {task:#}");

    let run_id = last_run_id(&workspace.root);
    let run = workspace.cost_json(&["--run", &run_id]);
    assert_eq!(
        run["summary"]["coverage"], "partial",
        "the run left two spawns unrecorded: {run:#}"
    );
    assert_eq!(run["summary"]["unrecorded_agent_invocation_count"], 2, "run: {run:#}");
}

/// A supported family whose record write failed: the sentence of
/// §FS-rhei-cost-accounting.6 about supported built-in agents, exactly.
// §FS-rhei-cost-accounting.6.2.1 §FS-rhei-summary.2.3
#[cfg(unix)]
#[test]
fn a_record_write_that_failed_leaves_summary_and_cost_partial() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::accounting_unrecorded_spawn_support::{
        LOSTWRITE_MACHINE, LOSTWRITE_PLAN, LOSTWRITE_SECOND_TICKET,
    };

    let workspace = spawn_workspace("unrecorded-lostwrite", LOSTWRITE_PLAN, LOSTWRITE_MACHINE);
    workspace.run();
    let invocations = workspace.invocations_dir();
    fs::set_permissions(&invocations, fs::Permissions::from_mode(0o555))
        .expect("make the invocations directory read-only");
    let plan = fs::read_to_string(&workspace.plan).expect("read plan");
    fs::write(&workspace.plan, format!("{plan}{LOSTWRITE_SECOND_TICKET}")).expect("append ticket");
    let second = workspace.run();
    fs::set_permissions(&invocations, fs::Permissions::from_mode(0o755))
        .expect("give the invocations directory back");
    assert!(
        format!("{}{}", second.stdout, second.stderr).contains("failed to record accounting"),
        "fixture: the second record write must have failed; got:\n{}\n{}",
        second.stdout,
        second.stderr
    );
    workspace.assert_shape(2, 1);

    let summary = workspace.summary_details();
    assert_eq!(
        coverage_row(&summary),
        "| coverage | Partial (1 of 2 agent invocations have no accounting record) |",
        "the second ticket's record was never written; got:\n{summary}"
    );
    let json = workspace.cost_json(&[]);
    assert_eq!(json["summary"]["coverage"], "partial", "cost: {json:#}");
    assert_eq!(json["summary"]["unrecorded_agent_invocation_count"], 1, "cost: {json:#}");
}

/// The control: every agent spawn has its record and a program spawn has none.
/// Nothing is missing, so nothing is demoted and nothing new is printed.
// §FS-rhei-cost-accounting.6.2.1
#[test]
fn a_workspace_whose_agent_spawns_all_have_records_still_reads_complete() {
    let workspace = control_workspace("unrecorded-control");
    workspace.run();
    workspace.assert_shape(2, 2);

    let summary = workspace.summary_details();
    assert_eq!(coverage_row(&summary), "| coverage | Complete |", "got:\n{summary}");
    assert!(
        summary.contains("2 agent invocations across 1 model; 2 tasks completed."),
        "the lead line is unchanged when nothing is missing; got:\n{summary}"
    );
    assert!(!summary.contains("no accounting record"), "got:\n{summary}");

    let text = workspace.cost_text(&[]);
    assert!(text.contains("| Coverage Complete | Invocations 2"), "got:\n{text}");
    let json = workspace.cost_json(&[]);
    assert_eq!(json["summary"]["coverage"], "complete", "cost: {json:#}");
}

/// The count is on every summary object, `0` when nothing is missing, so a
/// consumer never has to read an absent field as an answer.
// §FS-rhei-cost-accounting.6.2.1
#[test]
fn the_unrecorded_count_is_zero_when_every_agent_spawn_has_a_record() {
    let workspace = control_workspace("unrecorded-control-json");
    workspace.run();
    workspace.assert_shape(2, 2);

    let json = workspace.cost_json(&[]);
    assert_eq!(json["summary"]["unrecorded_agent_invocation_count"], 0, "cost: {json:#}");
}
