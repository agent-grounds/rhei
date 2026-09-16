//! Target and settings-home selection for roster inspection.
//! §FS-rhei-agents.1.1.7 §FS-rhei-panta.6

use std::fs;

use super::roster_support::*;
use super::*;

fn assert_root(result: &CliRun, root: &std::path::Path) {
    assert_eq!(roster_json(result)["project_root"], resolved(root));
}

/// Explicit bare plans, explicit Panta members, and omitted discovery all
/// resolve to the settings root. Task markdown is deliberately malformed:
/// inspecting settings must not parse it. §FS-rhei-agents.1.1.7
#[test]
fn roster_resolves_explicit_member_and_omitted_targets_without_parsing_tasks() {
    let root = unique_temp_dir("roster-scope");
    let home = root.join("home");

    let bare = root.join("bare");
    fs::create_dir_all(&bare).expect("create bare-plan directory");
    let bare_plan = write_plan(
        &bare,
        "solo.rhei.md",
        "# Rhei: Broken body\n\n## Tasks\n\n### Task nope: not a valid task id\n**State:** pending\n",
    );
    assert_root(&run_roster(&home, &bare, Some(&bare_plan), true), &bare);
    assert_root(&run_roster(&home, &bare, None, true), &bare);

    let project = root.join("project");
    fs::create_dir_all(project.join("nested/place")).expect("create project fixture");
    write_panta(&project);
    let member = write_plan(
        &project,
        "alpha.rhei.md",
        "# Rhei: Broken member\n\n## Tasks\n\n### Task nope: not a valid task id\n**State:** pending\n",
    );
    assert_root(&run_roster(&home, &project, Some(&project), true), &project);
    assert_root(&run_roster(&home, &project, Some(&member), true), &project);
    assert_root(&run_roster(&home, &project.join("nested/place"), None, true), &project);
}

/// No target is not permission to merge settings for an arbitrary directory;
/// the shared discovery error must tell the caller what target to provide.
/// §FS-rhei-agents.1.1.7 §FS-rhei-panta.6
#[test]
fn roster_without_a_discoverable_target_fails_with_target_guidance() {
    let root = unique_temp_dir("roster-no-target");
    let home = root.join("home");
    let result = run_roster(&home, &root, None, true);

    assert!(!result.status.success(), "an arbitrary directory must not become a project");
    assert!(result.stdout.is_empty(), "failure must not emit a roster: {}", result.stdout);
    assert!(
        result.stderr.contains("plan")
            && result.stderr.contains("project")
            && result.stderr.contains("rhei roster"),
        "the error must explain how to point roster at an existing target:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("unrecognized subcommand"),
        "the test must fail on discovery rather than an absent roster command:\n{}",
        result.stderr
    );
}

/// Existing paths are not enough: an explicit target must have a recognized
/// plan, workspace, or project shape. §FS-rhei-agents.1.1.7 §FS-rhei-panta.6
#[test]
fn roster_rejects_explicit_unrecognized_directory_and_file_with_json_guidance() {
    let root = unique_temp_dir("roster-unrecognized-target");
    let home = root.join("home");
    let directory = root.join("ordinary-directory");
    fs::create_dir_all(&directory).expect("create ordinary directory");
    let file = write_fixture_file(&root, "notes.md", "not a Rhei plan\n");

    for target in [&directory, &file] {
        let result = run_roster(&home, &root, Some(target), true);
        assert!(!result.status.success(), "roster accepted {}", target.display());
        assert!(result.stdout.is_empty(), "failure emitted a roster: {}", result.stdout);
        let error: serde_json::Value =
            serde_json::from_str(result.stderr.trim()).unwrap_or_else(|why| {
                panic!("stderr must be one JSON error object: {why}\n{}", result.stderr)
            });
        assert!(
            error["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("not a recognized")),
            "error did not classify the target: {error:#}"
        );
        assert!(
            error["error"]["help"]
                .as_str()
                .is_some_and(|help| help.contains("plan") || help.contains("Plan")),
            "error did not guide the caller to a plan target: {error:#}"
        );
    }
}

/// The current project home wins and suppresses the deprecated warning; only
/// after it is absent may the deprecated file supply the entire project layer.
/// §FS-rhei-agents.1.1 §FS-rhei-agents.1.1.7
#[test]
fn roster_selects_current_settings_before_deprecated_fallback() {
    let root = unique_temp_dir("roster-settings-home");
    let home = root.join("home");
    let plan = write_plan(
        &root,
        "plan.rhei.md",
        "# Rhei: Settings homes\n\n## Tasks\n\n### Task 1: Inert\n**State:** completed\n",
    );
    let current = write_settings(
        &root,
        CURRENT_SETTINGS,
        r#"{"agents":{"current-only":{"command":["current"]}}}"#,
    );
    let deprecated = write_settings(
        &root,
        DEPRECATED_SETTINGS,
        r#"{"agents":{"deprecated-only":{"command":["deprecated"]}}}"#,
    );

    let selected = run_roster(&home, &root, Some(&plan), true);
    let current_payload = roster_json(&selected);
    assert_eq!(current_payload["sources"]["project"]["path"], resolved(&current));
    assert_eq!(current_payload["sources"]["project"]["home"], "current");
    assert!(current_payload["agents"].get("current-only").is_some());
    assert!(current_payload["agents"].get("deprecated-only").is_none());
    assert!(selected.stderr.is_empty(), "the current home is not deprecated");

    fs::remove_file(&current).expect("remove current settings to expose fallback");
    let fallback = run_roster(&home, &root, Some(&plan), true);
    let deprecated_payload = roster_json(&fallback);
    assert_eq!(deprecated_payload["sources"]["project"]["path"], resolved(&deprecated));
    assert_eq!(deprecated_payload["sources"]["project"]["home"], "deprecated");
    assert!(deprecated_payload["agents"].get("current-only").is_none());
    assert!(deprecated_payload["agents"].get("deprecated-only").is_some());
    assert!(fallback.stderr.to_lowercase().contains("deprecated"));
    let warning = fallback.stderr.replace('\\', "/");
    assert!(warning.contains(".agents/rhei/settings.json"));
    assert!(warning.contains(".agent-grounds/rhei/settings.json"));
}
