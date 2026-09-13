//! Per-name project template discovery across the complete ancestor walk.
//! §FS-rhei-templates.1.2

use std::path::{Path, PathBuf};

use super::agent_grounds_support::run_in;
use super::*;

const TEMPLATES: &str = ".agent-grounds/rhei/templates";
const TARGET: &str = "grounded-ticket";
const WORKSPACE_DESCRIPTION: &str = "WORKSPACE copy";
const USER_DESCRIPTION: &str = "USER copy";

#[derive(Clone, Copy)]
enum NearerHome {
    SettingsOnly,
    EmptyTemplates,
    OtherTemplate,
    InvalidTarget,
    UnreadableTarget,
}

struct DiscoveryFixture {
    _root: TestDir,
    root: PathBuf,
    workspace: PathBuf,
    member: PathBuf,
    home: PathBuf,
    workspace_template: PathBuf,
}

fn write_template(root: &Path, name: &str, description: &str, rendered_marker: &str) -> PathBuf {
    let template = root.join(name);
    std::fs::create_dir_all(&template).expect("create template directory");
    write_fixture_file(
        &template,
        "template.yaml",
        &format!("name: {name}\nversion: 1.0.0\ndescription: {description}\ninputs: []\n"),
    );
    write_fixture_file(
        &template,
        "plan.rhei.md",
        &format!(
            "# Rhei: {rendered_marker}\n\n## Tasks\n\n### Task 1: {rendered_marker}\n**State:** pending\n"
        ),
    );
    template
}

fn fixture(prefix: &str, nearer: NearerHome) -> DiscoveryFixture {
    let root = unique_temp_dir(prefix);
    let root_path = root.to_path_buf();
    let workspace = root.join("workspace");
    let member = workspace.join("member/panta");
    let home = root.join("home");
    std::fs::create_dir_all(&member).expect("create member task store");
    std::fs::create_dir_all(&home).expect("create isolated user home");

    let workspace_template = write_template(
        &workspace.join(TEMPLATES),
        TARGET,
        WORKSPACE_DESCRIPTION,
        "WORKSPACE RENDERED",
    );
    let user_template =
        write_template(&home.join(TEMPLATES), TARGET, USER_DESCRIPTION, "{{ user_only_missing }}");
    std::fs::create_dir_all(user_template.join("prompt_templates"))
        .expect("create stale user prompt directory");
    write_fixture_file(
        &user_template,
        "prompt_templates/wait-dependencies.sh",
        "#!/bin/sh\necho '{{ user_only_missing }}'\n",
    );

    match nearer {
        NearerHome::SettingsOnly => {
            let settings = member.join(".agent-grounds/rhei");
            std::fs::create_dir_all(&settings).expect("create nearer rhei home");
            write_fixture_file(&settings, "settings.json", "{}\n");
        }
        NearerHome::EmptyTemplates => {
            std::fs::create_dir_all(member.join(TEMPLATES))
                .expect("create empty nearer templates directory");
        }
        NearerHome::OtherTemplate => {
            write_template(
                &member.join(TEMPLATES),
                "member-only",
                "MEMBER other copy",
                "MEMBER OTHER RENDERED",
            );
        }
        NearerHome::InvalidTarget => {
            let target = member.join(TEMPLATES).join(TARGET);
            std::fs::create_dir_all(&target).expect("create invalid nearer candidate");
            write_fixture_file(&target, "template.yaml", "name: [\n");
        }
        NearerHome::UnreadableTarget => {
            let manifest = member.join(TEMPLATES).join(TARGET).join("template.yaml");
            std::fs::create_dir_all(&manifest)
                .expect("make the nearer manifest path unreadable as a file");
        }
    }

    DiscoveryFixture { _root: root, root: root_path, workspace, member, home, workspace_template }
}

fn json(result: &CliRun) -> serde_json::Value {
    assert_success(result);
    serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("command should return JSON: {err}\nstdout:\n{}", result.stdout)
    })
}

fn path_tail(root: &Path, path: &Path) -> String {
    format!(
        "{}/{}",
        root.file_name().expect("fixture root name").to_string_lossy(),
        path.strip_prefix(root).expect("fixture path below root").display()
    )
    .replace('\\', "/")
}

fn json_path_ends_with(value: &serde_json::Value, tail: &str) -> bool {
    value["path"].as_str().is_some_and(|path| path.replace('\\', "/").ends_with(tail))
}

/// A settings file is not a template candidate and therefore never bounded the
/// template walk. This is the issue's passing control. §FS-rhei-templates.1.2
#[test]
fn settings_only_nearer_home_keeps_ancestor_detail_and_instantiation() {
    let fixture = fixture("template-ancestors-settings-control", NearerHome::SettingsOnly);
    let detail = run_in(&["templates", TARGET, "--json"], &fixture.member, &fixture.home);
    let detail_json = json(&detail);
    let expected_tail = path_tail(&fixture.root, &fixture.workspace_template);
    assert!(
        detail_json["source"] == "project"
            && detail_json["description"] == WORKSPACE_DESCRIPTION
            && json_path_ends_with(&detail_json, &expected_tail),
        "settings-only control should select the workspace copy at '{expected_tail}'; got:\n{}",
        detail.stdout
    );

    let instantiated =
        run_in(&["instantiate", TARGET, "--dry-run"], &fixture.member, &fixture.home);
    assert_success(&instantiated);
    assert!(
        instantiated.stdout.contains("WORKSPACE RENDERED"),
        "dry-run should render the workspace copy; stdout was:\n{}",
        instantiated.stdout
    );
}

/// An empty nearer templates directory is not an isolation boundary. Named
/// detail from the member must identify the same project copy as detail from
/// the workspace root, ahead of the stale user copy. §FS-rhei-templates.1.2
#[test]
fn empty_nearer_templates_directory_does_not_hide_ancestor_named_detail() {
    let fixture = fixture("template-ancestors-empty", NearerHome::EmptyTemplates);
    let workspace = run_in(&["templates", TARGET, "--json"], &fixture.workspace, &fixture.home);
    let member = run_in(&["templates", TARGET, "--json"], &fixture.member, &fixture.home);
    let workspace_json = json(&workspace);
    let member_json = json(&member);
    let expected_tail = path_tail(&fixture.root, &fixture.workspace_template);

    assert!(
        workspace_json["source"] == "project"
            && member_json["source"] == "project"
            && workspace_json["description"] == WORKSPACE_DESCRIPTION
            && member_json["description"] == WORKSPACE_DESCRIPTION
            && json_path_ends_with(&workspace_json, &expected_tail)
            && json_path_ends_with(&member_json, &expected_tail),
        "workspace-root and member detail must select the same ancestor copy at '{expected_tail}'\nworkspace stdout:\n{}\nmember stdout:\n{}",
        workspace.stdout,
        member.stdout
    );
}

/// A nearer directory containing another name contributes that name and then
/// discovery keeps ascending. The winner for each name is listed once, with
/// project copies ahead of user fallback. §FS-rhei-templates.1.2
/// §FS-rhei-templates.6.3
#[test]
fn unrelated_nearer_template_does_not_hide_ancestor_from_aggregated_listing() {
    let fixture = fixture("template-ancestors-list", NearerHome::OtherTemplate);
    let listing =
        run_in(&["templates", "--source", "project", "--json"], &fixture.member, &fixture.home);
    let entries = json(&listing);
    let entries = entries.as_array().expect("listing should be a JSON array");
    let target = entries.iter().filter(|entry| entry["name"] == TARGET).collect::<Vec<_>>();
    let member_count = entries.iter().filter(|entry| entry["name"] == "member-only").count();
    let expected_tail = path_tail(&fixture.root, &fixture.workspace_template);

    assert!(
        target.len() == 1
            && member_count == 1
            && target[0]["source"] == "project"
            && target[0]["description"] == WORKSPACE_DESCRIPTION
            && json_path_ends_with(target[0], &expected_tail),
        "listing must aggregate member-only and the workspace winner once at '{expected_tail}'; stdout was:\n{}",
        listing.stdout
    );
}

/// Named instantiation uses the same project winner as listing and detail. The
/// stale user copy deliberately cannot render, making wrong fallback visible.
/// §FS-rhei-templates.1.2 §FS-rhei-templates.6.1.2
#[test]
fn member_instantiation_uses_ancestor_copy_instead_of_stale_user_fallback() {
    let fixture = fixture("template-ancestors-instantiate", NearerHome::OtherTemplate);
    let result = run_in(&["instantiate", TARGET, "--dry-run"], &fixture.member, &fixture.home);

    assert_success(&result);
    assert!(
        result.stdout.contains("WORKSPACE RENDERED")
            && !result.stdout.contains("user_only_missing")
            && !result.stderr.contains("user_only_missing"),
        "dry-run must render the ancestor project copy\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// Listing/detail skip a matching candidate whose manifest cannot be read or
/// parsed and may expose the next valid ancestor copy. Named instantiation
/// still stops at that first matching directory and reports its error.
/// §FS-rhei-templates.1.2 §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.3
#[test]
fn invalid_and_unreadable_nearer_copies_preserve_command_specific_errors() {
    let cases =
        [("invalid", NearerHome::InvalidTarget), ("unreadable", NearerHome::UnreadableTarget)];
    let mut failures = Vec::new();

    for (label, nearer) in cases {
        let fixture = fixture(&format!("template-ancestors-{label}"), nearer);
        let listing =
            run_in(&["templates", "--source", "project", "--json"], &fixture.member, &fixture.home);
        let entries: serde_json::Value = json(&listing);
        let selected = entries
            .as_array()
            .and_then(|entries| entries.iter().find(|entry| entry["name"] == TARGET));
        let expected_tail = path_tail(&fixture.root, &fixture.workspace_template);
        let listing_ok = selected.is_some_and(|entry| {
            entry["source"] == "project"
                && entry["description"] == WORKSPACE_DESCRIPTION
                && json_path_ends_with(entry, &expected_tail)
        });

        let instantiated =
            run_in(&["instantiate", TARGET, "--dry-run"], &fixture.member, &fixture.home);
        let nearer_tail = path_tail(
            &fixture.root,
            &fixture.member.join(TEMPLATES).join(TARGET).join("template.yaml"),
        );
        let instantiate_ok = !instantiated.status.success()
            && instantiated.stderr.replace('\\', "/").contains(&nearer_tail)
            && !instantiated.stderr.contains("user_only_missing");

        if !listing_ok || !instantiate_ok {
            failures.push(format!(
                "{label}: expected listing winner '{expected_tail}' and instantiation error at '{nearer_tail}'\nlisting stdout:\n{}\ninstantiate stdout:\n{}\ninstantiate stderr:\n{}",
                listing.stdout, instantiated.stdout, instantiated.stderr
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
