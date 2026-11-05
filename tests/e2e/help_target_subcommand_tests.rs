//! A verb with a subcommand names a target the way `rhei list` does, so its lines
//! print the target too and stay whole: `rhei budget` and `rhei snapshot`, each
//! pasted from the directory it was typed in. §FS-rhei-errors.1.2.1
//!
//! `rhei budget` takes no `--state-machine` and `rhei snapshot list` reads the same
//! store without one, and both discover their own target, so what these hold beside
//! the target is that no machine and no placeholder is added.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use super::help_target_paste_tests::{assert_on_one_line, write, OPEN_PLAN};
use super::help_target_tests::{assert_offers, discovered, pasted, Case, WIDER_THAN_THE_RENDER};
use super::*;

/// A Panta project whose settings do not parse, which every budget subcommand reads
/// before its account.
fn broken_settings(case: &Case) -> PathBuf {
    let project = case.root.join("project");
    write(&project, "index.panta.md", "# Panta: Project\n");
    write(&project, "plan.rhei.md", OPEN_PLAN);
    write(&project, ".agent-grounds/rhei/settings.json", "{ not json");
    project
}

/// `command` with the values only the reader chooses filled in, pasted.
fn filled_in(command: &str, cwd: &Path, case: &Case) -> CliRun {
    let filled = command.replace("<N>", "6").replace("<TEXT>", "'set by the reader'");
    pasted(&filled, cwd, &case.home)
}

#[track_caller]
fn assert_reports_the_settings(run: &CliRun, command: &str) {
    assert!(
        run.stderr.contains("settings.json") && !run.stderr.contains("no Rhei plan found"),
        "`{command}` should report the broken settings:\n{}",
        run.stderr
    );
}

/// Inside the project, `rhei budget show` discovers it, and `rhei validate` is
/// told what it found. §FS-rhei-errors.1.2.1
#[test]
fn help_target_budget_with_broken_settings_validates_what_it_discovered() {
    let case = Case::new("help-target-budget-settings-in");
    let project = broken_settings(&case);

    let out = case.rhei(&project, &["budget", "show"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the settings refuse:\n{rendered}");
    let expected =
        format!("rhei validate {}", shell_quote(&discovered(&project).display().to_string()));
    assert_offers(&rendered, &expected);
    assert_reports_the_settings(&pasted(&expected, &project, &case.home), &expected);
}

/// From the parent, the target `rhei budget` was given. §FS-rhei-errors.1.2.1
#[test]
fn help_target_budget_with_broken_settings_validates_the_target_it_was_given() {
    let case = Case::new("help-target-budget-settings");
    broken_settings(&case);

    for subcommand in
        [&["show", "project"][..], &["init", "project", "--invocations", "5", "--reason", "x"]]
    {
        let out = case.rhei(&case.root, &[&["budget"][..], subcommand].concat());

        let rendered = raw_stderr(&out);
        assert_eq!(out.status.code(), Some(1), "the settings refuse:\n{rendered}");
        assert_offers(&rendered, "rhei validate project");
    }
    let again = pasted("rhei validate project", &case.root, &case.home);
    assert_reports_the_settings(&again, "rhei validate project");
}

/// A project with no account is told how to establish one for itself, not for a
/// `<TARGET>` it has to work out. §FS-rhei-errors.1.2.1 §FS-rhei-budgets.5.4
#[test]
fn help_target_budget_with_no_account_establishes_the_one_it_was_given() {
    let case = Case::new("help-target-budget-none");
    write(&case.root.join("project"), "plan.rhei.md", OPEN_PLAN);

    let out = case.rhei(&case.root, &["budget", "show", "project"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "no account yet:\n{rendered}");
    let expected = "rhei budget init project --invocations <N> --reason <TEXT>";
    assert_on_one_line(&rendered, &format!("establish one explicitly with: {expected}"));
    let again = filled_in(expected, &case.root, &case);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
    assert!(again.stdout.contains("lifetime invocation allowance: 6"), "{}", again.stdout);
}

/// A second `init` is sent to `adjust` of the same project, which from the parent a
/// bare `rhei budget adjust` cannot find. §FS-rhei-errors.1.2.1 §FS-rhei-budgets.10
#[test]
fn help_target_budget_already_held_adjusts_the_one_it_was_given() {
    let case = Case::new("help-target-budget-held");
    write(&case.root.join("project"), "plan.rhei.md", OPEN_PLAN);
    let init = ["budget", "init", "project", "--invocations", "5", "--reason", "first"];
    let first = case.rhei(&case.root, &init);
    assert!(first.status.success(), "the first init works:\n{}", raw_stderr(&first));

    let out = case.rhei(&case.root, &init);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the second init refuses:\n{rendered}");
    let expected = "rhei budget adjust project --invocations <N> --reason <TEXT>";
    assert_offers(&rendered, expected);
    let again = filled_in(expected, &case.root, &case);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
    assert!(again.stdout.contains("lifetime invocation allowance: 6"), "{}", again.stdout);
}

/// The subcommand is not the target, so a line naming one past it is not wrapped,
/// inside its quotes or anywhere else. §FS-rhei-errors.1.2.1 §FS-rhei-errors.2
#[test]
fn help_target_budget_target_past_the_wrap_column_stays_whole() {
    let case = Case::new("help-target-budget-wide");
    let typed = format!("{WIDER_THAN_THE_RENDER}/sub dir");
    write(&case.root.join(&typed), "plan.rhei.md", OPEN_PLAN);

    let out = case.rhei(&case.root, &["budget", "show", &typed]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "no account yet:\n{rendered}");
    let expected =
        format!("rhei budget init {} --invocations <N> --reason <TEXT>", shell_quote(&typed));
    assert_on_one_line(&rendered, &expected);
    let again = filled_in(&expected, &case.root, &case);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
}

/// The snapshot store is the target's, and `--plan` defaults to the directory the
/// line is pasted in. §FS-rhei-errors.1.2.1 §FS-rhei-snapshots
#[test]
fn help_target_a_snapshot_refusal_lists_the_store_it_read() {
    let case = Case::new("help-target-snapshot");
    let project = case.root.join("project");
    write(&project, "plan.rhei.md", OPEN_PLAN);
    write(&project, ".rhei/cache/snapshots/broken/manifest.json", "{ not json");
    let plan = "project/plan.rhei.md";

    let out = case
        .rhei(&case.root, &["snapshot", "show", ".rhei/cache/snapshots/broken", "--plan", plan]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the manifest refuses:\n{rendered}");
    let expected = format!("rhei snapshot list --plan {plan}");
    assert_offers(&rendered, &expected);
    let again = pasted(&expected, &case.root, &case.home);
    assert!(
        again.status.success(),
        "`{expected}` should read the project's store:\n{}",
        again.stderr
    );
    assert!(again.stdout.contains("No snapshots found"), "{}", again.stdout);
}
