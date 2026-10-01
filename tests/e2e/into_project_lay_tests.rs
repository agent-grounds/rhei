//! Laying a **Panta project** from a template, and placing a template `--into`
//! a project: the third template layout and the third `--into` target form.
//! §FS-rhei-templates.6.4 §FS-rhei-library.2.2
//!
//! A project template carries `index.panta.md` and a machine where a plan
//! template carries its plan. `--output` lays a whole project with it; `--into`
//! lays the same parts onto a project that exists, keeping that project's
//! manifest and laying only the members it does not have yet. Laying the default
//! writes the project root's `states.yaml` and never a `**States:**` line,
//! because that file's presence is what makes it the default and the line is
//! deprecated. A plan template `--into` a project is a member, exactly as the
//! default `--output` inside the project would have laid it.

use super::into_project_lay_support::*;
use super::*;

/// `--output` on a project template lays a whole project that validates: the
/// manifest, the machine as the root `states.yaml`, the bundle **copied** beside
/// it, the settings hoisted to the project, and one member per `includes:`
/// entry. Nothing writes a `**States:**` line. §FS-rhei-templates.6.4
/// §FS-rhei-templates.2
#[test]
fn a_project_template_lays_a_project_standalone() {
    let dir = unique_temp_dir("lay-standalone");
    let template = write_lifecycle_templates(&dir);

    let laid = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--output", "laid"]);
    assert_success(&laid);

    let project = dir.join("laid");
    let manifest = std::fs::read_to_string(project.join("index.panta.md")).expect("the manifest");
    assert!(
        !manifest.contains("**States:**"),
        "laying the default writes states.yaml, never the deprecated line; got:\n{manifest}"
    );
    assert_eq!(
        std::fs::read(project.join("states.yaml")).expect("the default machine"),
        std::fs::read(template.join("states.yaml")).expect("the template's machine"),
        "the template's machine is the project default, at the project root"
    );
    for bundled in ["prompt_templates/brief.md", "scripts/collect.sh"] {
        let meta = std::fs::symlink_metadata(project.join(bundled))
            .unwrap_or_else(|err| panic!("{bundled} is laid beside the manifest: {err}"));
        assert!(meta.is_file(), "{bundled} is copied, never symlinked");
    }
    let settings = std::fs::read_to_string(project.join(".agent-grounds/rhei/settings.json"))
        .expect("the template's settings are the project's");
    assert!(settings.contains("borrowed"), "got:\n{settings}");
    assert!(!project.join("settings.json").exists(), "nothing is left where nothing reads it");
    assert!(!project.join("template.yaml").exists(), "the manifest is never laid");
    for member_file in ["index.rhei.md", "states.yaml", "tasks/001-record.md"] {
        assert!(
            project.join("intake").join(member_file).is_file(),
            "the includes: entry lands as a member rhei: intake/{member_file}"
        );
    }

    assert_success(&rhei_in(&dir, &dir, &["validate", "laid"]));
}

/// A project template is listed, never dropped, and its JSON entry says which
/// layout it is, so a caller can tell what `--output` will produce.
/// §FS-rhei-templates.6.3
#[test]
fn rhei_templates_lists_a_project_template_with_its_layout() {
    let dir = unique_temp_dir("lay-listing");
    write_lifecycle_templates(&dir);

    let table = rhei_in(&dir, &dir, &["templates", "--source", "project"]);
    assert_success(&table);
    assert!(
        table.stdout.lines().any(|line| line.starts_with("intake ")),
        "control: the member template is listed; got:\n{}",
        table.stdout
    );
    assert!(
        table.stdout.lines().any(|line| line.starts_with("lifecycle ")),
        "the project template is listed; got:\n{}",
        table.stdout
    );

    let json = rhei_in(&dir, &dir, &["templates", "--source", "project", "--json"]);
    assert_success(&json);
    let entries: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON listing");
    let layout_of = |name: &str| {
        entries
            .as_array()
            .and_then(|all| all.iter().find(|entry| entry["name"] == name))
            .map(|entry| entry["layout"].clone())
    };
    assert_eq!(layout_of("lifecycle"), Some(serde_json::json!("project")), "{}", json.stdout);
    assert_eq!(layout_of("intake"), Some(serde_json::json!("workspace")), "{}", json.stdout);
}

/// `--into .` from inside an existing project lays the default and the member,
/// leaves the project's own manifest byte-identical, and produces the same
/// default, bundle, settings and member as `--output` produces standalone.
/// §FS-rhei-library.2.2 §FS-rhei-templates.6.4
#[test]
fn into_a_project_lays_the_default_and_its_members() {
    let dir = unique_temp_dir("lay-into");
    write_lifecycle_templates(&dir);
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, None);
    assert_success(&rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--output", "laid"]));

    let placed = rhei_in(&dir, &project, &["instantiate", "lifecycle", "--into", "."]);
    assert_success(&placed);

    assert_eq!(
        std::fs::read_to_string(project.join("index.panta.md")).expect("the manifest"),
        PLAIN_MANIFEST,
        "the project manifest is never edited"
    );
    let mut laid = snapshot(&dir.join("laid"));
    let mut into = snapshot(&project);
    laid.remove("index.panta.md");
    into.remove("index.panta.md");
    assert_same_tree(&laid, &into, "--into a project lays what --output lays");
    assert_eq!(machine_name(&project.join("states.yaml")), "laidmachine");

    assert_success(&rhei_in(&dir, &dir, &["validate", "reports"]));
}

/// A plan template `--into` a project is a member, and the documented
/// equivalent of `--output <project>/<template-name>`: the two invocations
/// leave identical projects. The bare id `pa` resolves to the project because
/// what is on disk there is a project. §FS-rhei-library.2.2
#[test]
fn a_rhei_template_into_a_project_equals_the_default_output() {
    let dir = unique_temp_dir("lay-member-equivalence");
    write_intake_template(&dir);
    let into = write_project(&dir, "pa", PLAIN_MANIFEST, None);
    let output = write_project(&dir, "pb", PLAIN_MANIFEST, None);

    assert_success(&rhei_in(&dir, &dir, &["instantiate", "intake", "--into", "pa"]));
    assert_success(&rhei_in(&dir, &dir, &["instantiate", "intake", "--output", "pb/intake"]));

    assert!(into.join("intake/index.rhei.md").is_file(), "the member is laid at pa/intake");
    assert_same_tree(&snapshot(&output), &snapshot(&into), "--into equals the default output");
}

/// A rebind lays a member once and never touches it again: run twice more, it
/// is skipped and reported both times, its tickets exist once, and the project
/// is byte-identical to what the first lay left. §FS-rhei-library.2.2
#[test]
fn a_rebind_leaves_existing_members_alone() {
    let dir = unique_temp_dir("lay-rebind-members");
    write_lifecycle_templates(&dir);
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, None);
    assert_success(&rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports"]));
    let first = snapshot(&project);

    for round in ["second", "third"] {
        let again = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports"]);
        assert_success(&again);
        assert!(
            again.stdout.lines().any(|line| line.contains("intake") && line.contains("left as")),
            "the {round} run names the member it left as it is; got:\n{}",
            again.stdout
        );
        assert_same_tree(&first, &snapshot(&project), "a rebind appends nothing to a member");
    }
    let tickets = std::fs::read_dir(project.join("intake/tasks")).expect("the member's tasks");
    assert_eq!(tickets.count(), 1, "one copy of the member's ticket, however often it is rebound");
}

/// A bare id that finds a rhei at one candidate root and a project at another
/// is refused naming both, and nothing is written into either. Before the
/// project form existed the rhei was taken silently. §FS-rhei-library.2.2
#[test]
fn an_ambiguous_into_target_names_both() {
    let dir = unique_temp_dir("lay-ambiguous");
    write_intake_template(&dir);
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, None);
    let outer = write_project(&dir, "panta", PLAIN_MANIFEST, None);
    write_member(
        &outer,
        "reports",
        "# Rhei: Reports\n\n## Overview\n\nA member that shares the project's name.\n",
        Some(&machine("reportsmachine", "work", "task")),
        "### Task 1: Read the reports\n**State:** work\n",
    );
    let before = (snapshot(&project), snapshot(&outer));

    let placed = rhei_in(&dir, &dir, &["instantiate", "intake", "--into", "reports"]);
    assert!(
        !placed.status.success(),
        "an id naming a rhei and a project is refused, not guessed; got:\nstdout:\n{}\nstderr:\n{}",
        placed.stdout,
        placed.stderr
    );
    assert_stderr_contains(&placed, "reports/index.rhei.md");
    assert_stderr_contains(&placed, "reports/index.panta.md");
    assert_same_tree(&before.0, &snapshot(&project), "nothing is written into the project");
    assert_same_tree(&before.1, &snapshot(&outer), "nothing is written into the rhei");
}

/// `under:` on an entry of a project template is refused naming the entry,
/// whether the project is being laid or bound: there is no host task tree to
/// place under, and no second placement mechanism. §FS-rhei-library.6.1
#[test]
fn under_in_a_project_template_is_refused() {
    let dir = unique_temp_dir("lay-nested-entry");
    write_intake_template(&dir);
    write_project_template(
        &dir,
        "split-lifecycle",
        &laid_machine(&[]),
        "includes:\n  - { template: intake, under: ticket }\n",
    );
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, None);
    let before = snapshot(&project);

    let laid = rhei_in(&dir, &dir, &["instantiate", "split-lifecycle", "--output", "laid"]);
    assert!(!laid.status.success(), "under: is refused; got:\n{}", laid.stdout);
    assert_stderr_contains(&laid, "under:");
    assert_stderr_contains(&laid, "intake");
    assert!(!dir.join("laid").exists(), "a refused lay leaves no project behind");

    let bound = rhei_in(&dir, &dir, &["instantiate", "split-lifecycle", "--into", "reports"]);
    assert!(!bound.status.success(), "under: is refused; got:\n{}", bound.stdout);
    assert_stderr_contains(&bound, "under:");
    assert_stderr_contains(&bound, "intake");
    assert_same_tree(&before, &snapshot(&project), "a refused bind writes nothing");
}
