//! Rebinding a project that was bound by hand before `--into <project>`
//! existed: its bundle linked to a template's own directory, or those links
//! already removed. A link is refused by a spelling that is the link wherever
//! the command runs from, a member's settings hoist is not written through one
//! either, and a default whose bundle is gone is the defect the rebind replaces
//! rather than a reason to refuse it. §FS-rhei-library.2.2 §FS-rhei-library.2.3

use super::into_project_lay_support::*;
use super::*;

/// A single-file rhei beside the manifest, so its one ticket runs under the
/// project default.
fn write_default_ticket(project: &Path, id: &str, state: &str) {
    write_fixture_file(
        project,
        &format!("{id}.rhei.md"),
        &format!(
            "# Rhei: R\n\n## Tasks\n\n### Task one: Runs under the default\n**State:** {state}\n"
        ),
    );
}

/// The paths the link refusal's `rm` names, in order.
#[cfg(unix)]
fn named_by_rm(run: &CliRun) -> Vec<String> {
    let (_, command) = run
        .stderr
        .split_once("`rm ")
        .unwrap_or_else(|| panic!("the refusal gives an `rm`; got:\n{}", run.stderr));
    let command = command.split_once('`').map_or(command, |(command, _)| command);
    command.split_whitespace().map(str::to_owned).collect()
}

/// Run from a directory a link points into, the refusal still names each link
/// and never what it points to, so its `rm`, run there, removes the links and
/// nothing else. §FS-rhei-library.2.2
#[cfg(unix)]
#[test]
fn a_link_is_named_as_itself_from_the_directory_it_points_into() {
    use std::os::unix::fs::symlink;

    let dir = unique_temp_dir("hand-bound-link-name");
    write_project_template(&dir, "plain", &laid_machine(&[]), "");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&laid_machine(&[])));
    let outside = dir.join("outside");
    std::fs::create_dir_all(outside.join("prompt_templates")).expect("the link targets");
    write_fixture_file(&outside.join("prompt_templates"), "brief.md", "Brief {subject}.\n");
    let script = write_fixture_file(&outside, "collect.sh", "#!/bin/sh\nexit 0\n");
    std::fs::create_dir_all(project.join("scripts")).expect("the project's scripts");
    symlink("../outside/prompt_templates", project.join("prompt_templates")).expect("link");
    symlink("../../outside/collect.sh", project.join("scripts/collect.sh")).expect("link");

    for (cwd, into) in
        [(outside.clone(), "../reports"), (outside.join("prompt_templates"), "../../reports")]
    {
        let refused = rhei_in(&dir, &cwd, &["instantiate", "plain", "--into", into, "--dry-run"]);
        assert!(!refused.status.success(), "the links are refused; got:\n{}", refused.stdout);
        let named = named_by_rm(&refused);
        assert_eq!(named.len(), 2, "both links are named; got:\n{}", refused.stderr);
        for path in &named {
            let linked = std::fs::symlink_metadata(cwd.join(path))
                .is_ok_and(|meta| meta.file_type().is_symlink());
            assert!(linked, "'{path}' is a link from {}; got:\n{}", cwd.display(), refused.stderr);
        }
    }

    let refused = rhei_in(&dir, &outside, &["instantiate", "plain", "--into", "../reports"]);
    for path in named_by_rm(&refused) {
        std::fs::remove_file(outside.join(path)).expect("run the printed `rm`");
    }
    assert!(script.is_file(), "the script the link pointed to is still there");
    assert!(outside.join("prompt_templates/brief.md").is_file(), "and so is the directory");
    assert!(std::fs::symlink_metadata(project.join("prompt_templates")).is_err(), "link gone");
}

/// A member a rebind lays hoists its settings into the project's, so a linked
/// project settings file is refused even when the project template carries no
/// settings of its own: the file it points to and the project are untouched.
/// §FS-rhei-library.2.2
#[cfg(unix)]
#[test]
fn a_members_settings_hoist_is_not_written_through_a_link() {
    use super::into_support::BORROWED_AGENT_SETTINGS;
    use std::os::unix::fs::symlink;

    let dir = unique_temp_dir("hand-bound-member-settings");
    let template = write_lifecycle_templates(&dir);
    std::fs::remove_file(template.join("settings.json")).expect("a template without settings");
    let member = dir.join(".agent-grounds/rhei/templates/intake");
    write_fixture_file(&member, "settings.json", BORROWED_AGENT_SETTINGS);
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&laid_machine(&[])));
    std::fs::create_dir_all(project.join("prompt_templates")).expect("the project's bundle");
    write_fixture_file(&project.join("prompt_templates"), "brief.md", "Brief {subject}.\n");
    std::fs::create_dir_all(dir.join("outside")).expect("the link target's directory");
    let outside = write_fixture_file(&dir.join("outside"), "settings.json", "{}\n");
    std::fs::create_dir_all(project.join(".agent-grounds/rhei")).expect("the project's home");
    symlink(&outside, project.join(".agent-grounds/rhei/settings.json")).expect("link");
    let before = (std::fs::read(&outside).expect("settings"), snapshot(&project));

    for dry_run in [true, false] {
        let mut args = vec!["instantiate", "lifecycle", "--into", "reports"];
        if dry_run {
            args.push("--dry-run");
        }
        let refused = rhei_in(&dir, &dir, &args);
        assert!(
            !refused.status.success(),
            "the linked settings are refused; got:\n{}",
            refused.stdout
        );
        assert_stderr_contains(&refused, "symbolic link");
        assert_stderr_contains(&refused, "reports/.agent-grounds/rhei/settings.json");
    }
    assert_eq!(std::fs::read(&outside).expect("settings"), before.0, "nothing written through");
    assert_same_tree(&before.1, &snapshot(&project), "a refused rebind writes nothing");
}

/// A project bound by hand, its links already removed, runs a default that
/// cannot load the prompt templates it names. That is the defect the rebind
/// replaces: it lays the bundle, and the project validates afterwards.
/// §FS-rhei-library.2.3
#[test]
fn a_default_whose_bundle_is_gone_is_rebound() {
    let dir = unique_temp_dir("hand-bound-bundle-gone");
    write_project_template(&dir, "plain", &laid_machine(&[]), "");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&laid_machine(&[])));
    write_default_ticket(&project, "r", "shipped");
    let unloadable = rhei_in(&dir, &dir, &["validate", "reports"]);
    assert!(!unloadable.status.success(), "the default cannot load as the project stands");

    for dry_run in [true, false] {
        let mut args = vec!["instantiate", "plain", "--into", "reports"];
        if dry_run {
            args.push("--dry-run");
        }
        let rebound = rhei_in(&dir, &dir, &args);
        assert_success(&rebound);
        let copied = rebound
            .stdout
            .lines()
            .find(|line| line.trim_start().starts_with("copied:"))
            .unwrap_or_else(|| panic!("a `copied:` line; got:\n{}", rebound.stdout));
        assert!(copied.contains("prompt_templates/ (1 file)"), "the bundle is laid: {copied}");
    }
    assert_success(&rhei_in(&dir, &dir, &["validate", "reports"]));
}

/// A default whose bundle is gone does not hide a stranding, whether it loads
/// with the bundle the rebind lays or not even then: the ticket is named and
/// nothing is written. §FS-rhei-library.2.3
#[test]
fn a_default_whose_bundle_is_gone_still_refuses_a_stranding() {
    let dir = unique_temp_dir("hand-bound-stranding");
    write_project_template(&dir, "plain", &laid_machine(&[]), "");
    for fragment in ["brief", "gone"] {
        let house = machine("housemachine", "work", "task").replace(
            "    instructions: |\n      Do the work and move on.\n",
            &format!("    prompt_template:\n      name: {fragment}\n      values:\n        subject: it\n"),
        );
        let name = format!("reports-{fragment}");
        let project = write_project(&dir, &name, PLAIN_MANIFEST, Some(&house));
        write_default_ticket(&project, "stranded", "work");
        let before = snapshot(&project);

        let refused = rhei_in(&dir, &dir, &["instantiate", "plain", "--into", &name]);
        assert!(!refused.status.success(), "{fragment}: refused; got:\n{}", refused.stdout);
        assert_stderr_contains(&refused, "stranded.one");
        assert_same_tree(&before, &snapshot(&project), "a refused rebind writes nothing");
    }
}
