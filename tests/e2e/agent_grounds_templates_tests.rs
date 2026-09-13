//! Rhei's project-local template home moved to `.agent-grounds/rhei/templates`,
//! and `.agents/rhei/templates` is the deprecated fallback.
//! §FS-rhei-templates.1
//!
//! `.agents/` is where agent *instructions* live and an agent runtime may mount
//! it read-only inside a checkout, so rhei's own templates — product files an
//! agent has to be able to edit — cannot live there. §FS-rhei-templates.1.1
//!
//! The `.agents/` cases here are permanent coverage of the deprecated path, not
//! leftovers from before the move: deleting them would leave the fallback
//! unexercised the first time someone sweeps the suite onto the new name.

use std::path::Path;

use super::agent_grounds_support::{
    assert_deprecation_warning, assert_names_path, assert_silent_about_the_deprecated_home, run_in,
    DEPRECATED, GROUNDS,
};
use super::*;

const GROUNDS_TEMPLATES: &str = ".agent-grounds/rhei/templates";
const DEPRECATED_TEMPLATES: &str = ".agents/rhei/templates";

/// Run with a home of its own, so the user tier only holds what a test put there.
fn run_project(args: &[&str], dir: &Path) -> CliRun {
    run_in(args, dir, &dir.join("home"))
}

fn write_template(templates_root: &Path, name: &str, description: &str) {
    let template_dir = templates_root.join(name);
    std::fs::create_dir_all(&template_dir).expect("create template directory");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        &format!("name: {name}\nversion: 1.0.0\ndescription: {description}\ninputs: []\n"),
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Home fixture

## Tasks

### Task 1: Work
**State:** pending
"#,
    );
}

/// §FS-rhei-templates.1: the new project-local home is a search root, and
/// §FS-rhei-templates.1.3: reading it is silent.
#[test]
fn project_template_resolves_from_agent_grounds_without_a_warning() {
    let dir = unique_temp_dir("grounds-templates-project");
    write_template(&dir.join(GROUNDS_TEMPLATES), "ground-only", "Lives in the new home");

    let result = run_project(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("ground-only"),
        "the template must be discovered; stdout was:\n{}",
        result.stdout
    );
    assert_silent_about_the_deprecated_home(&result);
}

/// §FS-rhei-templates.1: the deprecated home is still read, and
/// §FS-rhei-templates.1.3: reading it warns, naming both paths.
#[test]
fn project_template_under_the_deprecated_home_is_found_and_warned() {
    let dir = unique_temp_dir("grounds-templates-fallback");
    write_template(&dir.join(DEPRECATED_TEMPLATES), "legacy-only", "Lives in the old home");

    let result = run_project(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("legacy-only"),
        "the deprecated home must keep working; stdout was:\n{}",
        result.stdout
    );
    assert_deprecation_warning(&result, &dir, DEPRECATED_TEMPLATES, GROUNDS_TEMPLATES);
}

/// §FS-rhei-templates.1.1: within a tier the new name wins, the shadowed copy
/// contributes nothing, and nothing is read from it to warn about.
#[test]
fn agent_grounds_shadows_the_deprecated_home_and_lists_the_template_once() {
    let dir = unique_temp_dir("grounds-templates-shadow");
    write_template(&dir.join(GROUNDS_TEMPLATES), "both", "Chosen from the new home");
    write_template(&dir.join(DEPRECATED_TEMPLATES), "both", "Shadowed in the old home");

    let result = run_project(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("Chosen from the new home"),
        "the new home must win; stdout was:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("Shadowed in the old home"),
        "the shadowed copy must contribute nothing; stdout was:\n{}",
        result.stdout
    );
    assert_eq!(
        result.stdout.matches("both  1.0.0").count(),
        1,
        "the template must be listed once; stdout was:\n{}",
        result.stdout
    );
    assert_silent_about_the_deprecated_home(&result);
}

/// §FS-rhei-templates.1.2: the walk checks both names at each level before
/// ascending, so a nearer deprecated copy beats a farther current copy of the
/// same name. Two full walks — one per home name — would reverse this order.
#[test]
fn template_ancestor_discovery_nearer_deprecated_shadows_farther_current() {
    let dir = unique_temp_dir("grounds-templates-walk");
    write_template(&dir.join(GROUNDS_TEMPLATES), "shared", "Farther current copy");
    let child = dir.join("child");
    std::fs::create_dir_all(&child).expect("create child directory");
    write_template(&child.join(DEPRECATED_TEMPLATES), "shared", "Nearer legacy copy");

    let result = run_in(&["templates", "--source", "project"], &child, &dir.join("home"));
    assert_success(&result);
    assert!(
        result.stdout.contains("Nearer legacy copy"),
        "the nearest copy wins even under the deprecated name; stdout was:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("Farther current copy"),
        "a farther current copy must stay shadowed; stdout was:\n{}",
        result.stdout
    );
    assert_eq!(result.stdout.matches("shared  1.0.0").count(), 1, "stdout was:\n{}", result.stdout);
    assert_deprecation_warning(&result, &child, DEPRECATED_TEMPLATES, GROUNDS_TEMPLATES);
}

/// §FS-rhei-templates.1.2: distinct names aggregate across ancestor levels and
/// both home names. Reading the farther deprecated copy warns even though a
/// nearer current templates directory exists.
#[test]
fn template_ancestor_discovery_aggregates_distinct_names_from_both_homes() {
    let dir = unique_temp_dir("grounds-templates-walk-new");
    write_template(&dir.join(DEPRECATED_TEMPLATES), "parent-template", "Held by the parent");
    let child = dir.join("child");
    std::fs::create_dir_all(&child).expect("create child directory");
    write_template(&child.join(GROUNDS_TEMPLATES), "child-template", "Held by the child");

    let result = run_in(&["templates", "--source", "project"], &child, &dir.join("home"));
    assert_success(&result);
    assert!(
        result.stdout.contains("child-template"),
        "the nearest home wins; stdout was:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("parent-template"),
        "the walk must continue and aggregate the ancestor's distinct name; stdout was:\n{}",
        result.stdout
    );
    assert_deprecation_warning(&result, &dir, DEPRECATED_TEMPLATES, GROUNDS_TEMPLATES);
}

/// §FS-rhei-templates.1: the tool states its own search path, so the new home
/// appears in the `Searched:` listing when nothing is found.
#[test]
fn the_searched_listing_names_the_agent_grounds_root() {
    let dir = unique_temp_dir("grounds-templates-searched");

    let result = run_project(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("No templates found."),
        "the fixture has no templates; stdout was:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.replace('\\', "/").contains(GROUNDS_TEMPLATES),
        "the new home must be named as a search root; stdout was:\n{}",
        result.stdout
    );
}

/// §FS-rhei-templates.1.3: once per distinct deprecated path per process, not
/// once per lookup. `rhei templates <name> --source <tier>` runs discovery
/// twice in one process — once for the tier, once for the detail view.
#[test]
fn the_deprecation_warning_fires_once_per_path_per_process() {
    let dir = unique_temp_dir("grounds-templates-once");
    write_template(&dir.join(DEPRECATED_TEMPLATES), "twice-looked-up", "Resolved more than once");

    let result = run_project(&["templates", "twice-looked-up", "--source", "project"], &dir);
    assert_success(&result);
    assert_deprecation_warning(&result, &dir, DEPRECATED_TEMPLATES, GROUNDS_TEMPLATES);
    assert_eq!(
        result.stderr.to_lowercase().matches("deprecated").count(),
        1,
        "a warning per lookup would bury the command's own output; stderr was:\n{}",
        result.stderr
    );
}

/// §FS-rhei-templates.1: the user tier has both names too, the new one first.
#[test]
fn user_templates_resolve_from_agent_grounds_before_the_deprecated_home() {
    let dir = unique_temp_dir("grounds-templates-user");
    let home = dir.join("home");
    write_template(&home.join(GROUNDS_TEMPLATES), "user-both", "Chosen from the user's new home");
    write_template(
        &home.join(DEPRECATED_TEMPLATES),
        "user-both",
        "Shadowed in the user's old home",
    );

    let result = run_in(&["templates", "--source", "user"], &dir, &home);
    assert_success(&result);
    assert!(
        result.stdout.contains("Chosen from the user's new home"),
        "the user tier must prefer the new home; stdout was:\n{}",
        result.stdout
    );
    assert_eq!(
        result.stdout.matches("user-both  1.0.0").count(),
        1,
        "the template must be listed once; stdout was:\n{}",
        result.stdout
    );
    assert_silent_about_the_deprecated_home(&result);
}

/// §FS-rhei-templates.1.3: the user tier's fallback warns the same way.
#[test]
fn user_template_under_the_deprecated_home_is_found_and_warned() {
    let dir = unique_temp_dir("grounds-templates-user-fallback");
    let home = dir.join("home");
    write_template(&home.join(DEPRECATED_TEMPLATES), "user-legacy", "The user's old home");

    let result = run_in(&["templates", "--source", "user"], &dir, &home);
    assert_success(&result);
    assert!(
        result.stdout.contains("user-legacy"),
        "the user tier's deprecated home must keep working; stdout was:\n{}",
        result.stdout
    );
    assert_deprecation_warning(
        &result,
        &dir,
        &format!("home/{DEPRECATED}/templates"),
        &format!("home/{GROUNDS}/templates"),
    );
}

/// §FS-rhei-templates.1.2: a level holding either directory contributes both
/// names, whether or not both are directories. The `Searched:` listing is where
/// an author about to write their first project template reads where it goes,
/// and a level found by its `.agents` directory alone must not advertise the
/// deprecated home as the only place.
#[test]
fn the_searched_listing_names_both_homes_of_the_level_it_resolved() {
    let dir = unique_temp_dir("grounds-templates-searched");
    std::fs::create_dir_all(dir.join(DEPRECATED_TEMPLATES)).expect("create the deprecated home");

    let result = run_project(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("No templates found."),
        "the fixture holds no templates; stdout was:\n{}",
        result.stdout
    );
    for home in [GROUNDS_TEMPLATES, DEPRECATED_TEMPLATES] {
        assert_names_path(&result.stdout, &dir, home);
    }
    assert!(
        result.stdout.contains("(does not exist)"),
        "the absent name is marked, as the no-home branch already marks it; stdout was:\n{}",
        result.stdout
    );
    // Nothing was read from the deprecated home, so nothing is warned about.
    assert_silent_about_the_deprecated_home(&result);
}
