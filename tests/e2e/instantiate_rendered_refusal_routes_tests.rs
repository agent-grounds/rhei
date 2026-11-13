//! The other routes a rendering is refused on: a project template's own files,
//! a refusal raised before validation under `--keep-on-error`, a rendered
//! `states.yaml` that does not load, the machine a refused validation names, and
//! `--into` a rhei. Each names the template file the bad content came from and
//! marks a rendered location with what became of it. §FS-rhei-errors.4.1

use std::fs;

use super::instantiate_rendered_refusal_support::*;

/// The issue's own `states.yaml`, which names no machine.
const UNNAMED_MACHINE: &str = "initial: todo\nstates:\n  todo:\n    description: Not started.\n  \
     done:\n    description: Finished.\n    final: true\ntransitions:\n  - {from: todo, to: done}\n";

/// What a load of the issue's `states.yaml` says.
const NAME_REASON: &str = "missing field `name`";

/// `rhei instantiate <args>` refused for `reason`, whatever kind of refusal it
/// is, with nothing left behind in scratch and scratch never named.
fn refused(layout: &Layout, args: &[&str], reason: &str) -> String {
    let mut full = vec!["instantiate"];
    full.extend_from_slice(args);
    let out = layout.rhei(&full);
    let text = out.stderr;
    assert_eq!(out.status.code(), Some(1), "the rendering should be refused:\n{text}");
    assert!(text.contains(reason), "the refusal's reason stays whole:\n{text}");
    let left: Vec<_> = fs::read_dir(&layout.scratch).expect("read scratch").collect();
    assert!(left.is_empty(), "scratch is discarded on refusal: {left:?}");
    let scratch = layout.scratch.display().to_string();
    assert!(!text.contains(&scratch), "a refusal never names scratch:\n{text}");
    text
}

/// A project template `release`, laying `index.panta.md` and its root machine,
/// with the `includes:` entries given.
fn release(layout: &Layout, index: &str, machine_text: &str, includes: &[&str]) {
    let mut manifest = "name: release\nversion: 1.0.0\ndescription: A project\n".to_owned();
    if !includes.is_empty() {
        manifest.push_str("includes:\n");
        for entry in includes {
            manifest.push_str(&format!("  - {entry}\n"));
        }
    }
    layout.template(
        "release",
        &[("template.yaml", &manifest), ("states.yaml", machine_text), ("index.panta.md", index)],
    );
}

/// A project template's `index.panta.md` that does not parse.
const STATES_LINE_MANIFEST: &str = "# Panta: Release\n\n**States:** forge\n";

/// A project template's own `index.panta.md` is rendered from the project
/// template, so that file is named, never `proj/index.panta.md`.
/// §FS-rhei-errors.4.1 §FS-rhei-templates.6.4
fn project_template_own_index(dry_run: bool) {
    let layout = Layout::standalone("rendered-project-own");
    release(&layout, STATES_LINE_MANIFEST, &machine("release"), &[]);
    let mut args = vec!["release", "--output", "proj"];
    if dry_run {
        args.push("--dry-run");
    }

    let text = layout.refused(&args, INDEX_REASON);

    assert_names_template_file(&layout, &text, "release", "index.panta.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "proj/index.panta.md");
    assert_offers_no_validate(&text);
    assert!(text.contains("rhei instantiate"), "the help re-runs the instantiation:\n{text}");
    assert!(!layout.repo.join("proj").exists(), "nothing of the refused project stays");
}

#[test]
fn instantiate_rendered_project_template_own_index_dry_run_names_the_template_file() {
    project_template_own_index(true);
}

#[test]
fn instantiate_rendered_project_template_own_index_real_run_names_the_template_file() {
    project_template_own_index(false);
}

/// With a member to lay, the project's own `index.panta.md` is refused while the
/// member validates in its project, and still resolves to the project template.
/// §FS-rhei-errors.4.1 §FS-rhei-templates.6.4
fn project_template_own_index_in_member_validation(dry_run: bool) {
    let layout = Layout::standalone("rendered-project-own-member");
    layout.forge_metadata("# Rhei: Forge metadata\n", TASK);
    release(&layout, STATES_LINE_MANIFEST, &machine("release"), &["forge-metadata"]);
    let mut args = vec!["release", "--output", "proj"];
    if dry_run {
        args.push("--dry-run");
    }

    let text = layout.refused(&args, INDEX_REASON);

    assert_names_template_file(&layout, &text, "release", "index.panta.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "proj/index.panta.md");
    assert_offers_no_validate(&text);
    assert!(!layout.repo.join("proj").exists(), "nothing of the refused project stays");
}

#[test]
fn instantiate_rendered_project_template_own_index_in_member_validation_dry_run() {
    project_template_own_index_in_member_validation(true);
}

#[test]
fn instantiate_rendered_project_template_own_index_in_member_validation_real_run() {
    project_template_own_index_in_member_validation(false);
}

/// A project template's root machine that does not load names the template's
/// `states.yaml`, and re-checks nothing at the rendered path.
/// §FS-rhei-errors.4.1 §FS-rhei-templates.6.4
#[test]
fn instantiate_rendered_project_template_root_machine_names_the_template_file() {
    let layout = Layout::standalone("rendered-project-machine");
    let versionless = "name: release\ninitial: todo\nstates:\n  todo:\n    description: t\n";
    release(&layout, "# Panta: Release\n", versionless, &[]);

    let text = refused(&layout, &["release", "--output", "proj", "--dry-run"], "missing field");

    assert_names_template_file(&layout, &text, "release", "states.yaml");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "proj/states.yaml");
    assert!(!text.contains("--state-machine"), "no re-check of a path that is gone:\n{text}");
    assert_offers_no_validate(&text);
}

/// The issue's template with an `includes:` entry: its plan is refused while the
/// entry joins, before validation, where `--keep-on-error` keeps nothing.
fn host_with_includes(layout: &Layout) {
    layout.forge_metadata("# Rhei: Forge metadata\n", TASK);
    let manifest =
        "name: host\nversion: 1.0.0\ndescription: A host\nincludes:\n  - forge-metadata\n";
    layout.template(
        "host",
        &[
            ("template.yaml", manifest),
            ("states.yaml", &machine("host")),
            ("index.rhei.md", "# Rhei: Host\n\n**States:** host\n"),
            ("tasks/01-host.md", "### Task h: Host thing\n**State:** todo\n"),
        ],
    );
}

/// Before validation a refused real run removes what it rendered even under
/// `--keep-on-error`, so the refusal says so, and offers no `rhei validate`.
/// §FS-rhei-errors.4.1 §FS-rhei-templates.6.1.2
#[test]
fn instantiate_rendered_kept_flag_before_validation_says_the_rendering_was_removed() {
    let layout = Layout::standalone("rendered-kept-early");
    host_with_includes(&layout);

    let text = layout.refused(&["host", "--keep-on-error"], INDEX_REASON);

    assert!(!layout.repo.join("host").exists(), "the rendering is removed before validation");
    assert!(!text.contains("kept for inspection"), "nothing was kept:\n{text}");
    assert!(text.contains("removed"), "the rendering says it was removed:\n{text}");
    assert_names_template_file(&layout, &text, "host", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "host/index.rhei.md");
    assert_offers_no_validate(&text);
}

/// A member's staging is never published when it is refused before validation,
/// `--keep-on-error` or not. §FS-rhei-errors.4.1 §FS-rhei-templates.6.1.2
#[test]
fn instantiate_rendered_kept_flag_before_validation_of_a_member_says_not_written() {
    let layout = Layout::member("rendered-kept-early-member");
    host_with_includes(&layout);

    let text = layout.refused(&["host", "--keep-on-error"], INDEX_REASON);

    let left: Vec<_> = fs::read_dir(&layout.panta)
        .expect("read the project")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.contains("host"))
        .collect();
    assert!(left.is_empty(), "neither the member nor its staging stays: {left:?}");
    assert!(!text.contains("kept for inspection"), "nothing was kept:\n{text}");
    assert!(text.contains("not written"), "the rendering says it was not written:\n{text}");
    assert_names_template_file(&layout, &text, "host", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
    assert_offers_no_validate(&text);
}

/// The issue's template once its `**States:**` line is deleted: its
/// `states.yaml` names no machine, and that refusal names the template's file.
fn unnamed_machine(layout: &Layout) {
    layout.forge_metadata("# Rhei: Forge metadata\n", TASK);
    layout.template("forge-metadata", &[("states.yaml", UNNAMED_MACHINE)]);
}

/// A rendered `states.yaml` that does not load is the template's, and its
/// re-check is the same instantiation, not `rhei states` of a path that is gone.
/// The refusal's own paragraph is read: a member's placement header marks the
/// proposed machine in its own words. §FS-rhei-errors.4.1 §FS-rhei-library.7.5
fn unnamed_machine_discarded(layout: &Layout, args: &[&str]) -> String {
    unnamed_machine(layout);

    let text = refused(layout, args, NAME_REASON);

    let failed = paragraphs(&text)
        .into_iter()
        .find(|paragraph| paragraph.contains("failed to parse state machine"))
        .unwrap_or_else(|| panic!("the fixture should reach the machine's load:\n{text}"));
    assert_rendered_marked(&failed, "forge-metadata/states.yaml");
    assert!(failed.to_lowercase().contains("render"), "marked as a rendering:\n{failed}");
    assert_names_template_file(layout, &text, "forge-metadata", "states.yaml");
    assert_template_paths_exist(layout, &text);
    assert!(!text.contains("--state-machine"), "no re-check of a path that is gone:\n{text}");
    assert_offers_no_validate(&text);
    assert!(text.contains("rhei instantiate"), "the help re-runs the instantiation:\n{text}");
    text
}

#[test]
fn instantiate_rendered_machine_load_dry_run_names_the_template_file() {
    let layout = Layout::standalone("rendered-machine-dry-run");
    let text = unnamed_machine_discarded(&layout, &["forge-metadata", "--dry-run"]);
    assert_rendered_marked(&text, "forge-metadata/states.yaml");
    assert!(!layout.repo.join("forge-metadata").exists(), "a dry run writes nothing");
}

#[test]
fn instantiate_rendered_machine_load_real_run_names_the_template_file() {
    let layout = Layout::standalone("rendered-machine-real");
    let text = unnamed_machine_discarded(&layout, &["forge-metadata"]);
    assert_rendered_marked(&text, "forge-metadata/states.yaml");
    assert!(!layout.repo.join("forge-metadata").exists(), "the refused output is removed");
}

/// The header already marks the proposed machine; the nested refusal agrees.
/// §FS-rhei-errors.4.1 §FS-rhei-library.7.5
#[test]
fn instantiate_rendered_machine_load_of_a_member_names_the_template_file() {
    let layout = Layout::member("rendered-machine-member");
    unnamed_machine_discarded(&layout, &["forge-metadata", "--dry-run"]);
    assert!(!layout.panta.join("forge-metadata").exists(), "a dry run writes nothing");
}

/// Kept, the rendered machine exists and its re-check stands, while the refusal
/// still names the template's file. §FS-rhei-errors.4.1 §FS-rhei-errors.1.2.1
#[test]
fn instantiate_rendered_machine_load_kept_keeps_its_re_check() {
    let layout = Layout::standalone("rendered-machine-kept");
    unnamed_machine(&layout);

    let text = refused(&layout, &["forge-metadata", "--keep-on-error"], NAME_REASON);

    assert!(layout.repo.join("forge-metadata/states.yaml").is_file(), "the rendering is kept");
    assert!(text.contains("rhei states --state-machine"), "the kept copy is re-checked:\n{text}");
    assert_names_template_file(&layout, &text, "forge-metadata", "states.yaml");
    assert_template_paths_exist(&layout, &text);
}

/// A template whose task names a state its machine lacks: the validation
/// refusal names the machine it validated against.
fn bogus_state(layout: &Layout) {
    layout.forge_metadata(
        "# Rhei: Forge metadata\n",
        "### Task one: Do the thing\n**State:** bogus\n",
    );
}

/// The machine a refused validation names is the template's rendering too: the
/// paragraph that names it says whose rendering it is and that it was not
/// written. §FS-rhei-errors.4.1
fn assert_validated_machine_marked(layout: &Layout, text: &str) {
    let validated = paragraphs(text)
        .into_iter()
        .find(|paragraph| paragraph.contains("I validated this plan using"))
        .unwrap_or_else(|| panic!("the fixture should reach validation:\n{text}"));
    let lower = validated.to_lowercase();
    assert!(
        lower.contains("render") && lower.contains("not written"),
        "the machine is named as a file that exists:\n{validated}\n\nwhole refusal:\n{text}"
    );
    let shown = format!("{TEMPLATE_HOME}/forge-metadata/states.yaml");
    assert!(validated.contains(&shown), "the machine's template file is named:\n{validated}");
    assert_template_paths_exist(layout, text);
    assert_offers_no_validate(text);
}

#[test]
fn instantiate_rendered_validation_names_the_rendered_machine_as_a_rendering() {
    let layout = Layout::standalone("rendered-validation-machine");
    bogus_state(&layout);

    let text = refused(&layout, &["forge-metadata", "--dry-run"], "invalid state 'bogus'");

    assert_validated_machine_marked(&layout, &text);
}

/// In a project the machine is one source of several; the host's stays itself.
/// §FS-rhei-errors.4.1 §FS-rhei-validate.6
#[test]
fn instantiate_rendered_validation_of_a_member_names_its_machine_as_a_rendering() {
    let layout = Layout::member("rendered-validation-machine-member");
    bogus_state(&layout);

    let text = refused(&layout, &["forge-metadata", "--dry-run"], "invalid state 'bogus'");

    assert_validated_machine_marked(&layout, &text);
    assert!(text.contains("the built-in default state machine (project default)"), "{text}");
}

/// A rhei `host` beside the repository, for `--into` to place a template into.
fn host_rhei(layout: &Layout) {
    let host = layout.repo.join("host");
    fs::create_dir_all(host.join("tasks")).expect("create the host");
    fs::write(host.join("states.yaml"), machine("host")).expect("write the host machine");
    fs::write(host.join("index.rhei.md"), "# Rhei: Host\n").expect("write the host plan");
    fs::write(host.join("tasks/01-host.md"), "### Task h: Host thing\n**State:** todo\n")
        .expect("write the host task");
}

/// `--into` a rhei renders the template's fragment as a new task file of the
/// host's: that file is the template's rendering, named by the template file it
/// came from, while the host's own files stay themselves.
/// §FS-rhei-errors.4.1 §FS-rhei-library.7.5
fn into_rhei(dry_run: bool) {
    let layout = Layout::standalone("rendered-into-rhei");
    host_rhei(&layout);
    layout.forge_metadata("# Rhei: Forge metadata\n", STATES_LINE_TASK);
    let mut args = vec!["forge-metadata", "--into", "host"];
    if dry_run {
        args.push("--dry-run");
    }

    let text = layout.refused(&args, TASK_REASON);

    assert!(text.contains("002-task.md"), "the fixture should reach the placed fragment:\n{text}");
    assert_names_template_file(&layout, &text, "forge-metadata", "tasks/01-task.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "host/tasks/002-task.md");
    assert_offers_no_validate(&text);
    assert!(!layout.repo.join("host/tasks/002-task.md").exists(), "nothing was placed");
}

#[test]
fn instantiate_rendered_into_rhei_dry_run_names_the_template_file() {
    into_rhei(true);
}

#[test]
fn instantiate_rendered_into_rhei_real_run_names_the_template_file() {
    into_rhei(false);
}

/// A fragment an `includes:` union renamed falls back to the template that
/// rendered it, and says the line may be in a template it includes.
/// §FS-rhei-errors.4.1
#[test]
fn instantiate_rendered_renamed_fragment_points_at_the_included_templates_too() {
    let layout = Layout::standalone("rendered-renamed-fragment");
    layout.forge_metadata("# Rhei: Forge metadata\n", STATES_LINE_TASK);
    let manifest =
        "name: host\nversion: 1.0.0\ndescription: A host\nincludes:\n  - forge-metadata\n";
    layout.template(
        "host",
        &[
            ("template.yaml", manifest),
            ("states.yaml", &machine("host")),
            ("index.rhei.md", "# Rhei: Host\n"),
            ("tasks/01-host.md", "### Task h: Host thing\n**State:** todo\n"),
        ],
    );

    let text = layout.refused(&["host", "--dry-run"], TASK_REASON);

    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(folded.contains("or a template it includes"), "{text}");
    assert_template_paths_exist(&layout, &text);
}
