//! A template whose rendering does not parse is refused at the template file the
//! bad line came from, never at a rendered copy the reader cannot open: the
//! `--dry-run` scratch, a member's staging, or an output a failed run removed.
//! Built from agent-grounds/rhei#658's own script. §FS-rhei-errors.4.1

use std::fs;

use super::instantiate_rendered_refusal_support::*;

/// The issue's script: a Panta project beside the repository, so the default
/// output is `../panta/forge-metadata`, and a dry run that names a file it never
/// wrote. §FS-rhei-errors.4.1 §FS-rhei-library.7.5
#[test]
#[ignore = "red until #658: the member dry run names ../panta/forge-metadata/index.rhei.md, \
            never the template's; implement removes this"]
fn instantiate_rendered_member_dry_run_names_the_template_file() {
    let layout = Layout::member("rendered-member-dry-run");
    layout.forge_metadata(STATES_LINE_INDEX, TASK);

    let text = layout.refused(&["forge-metadata", "--dry-run"], INDEX_REASON);

    assert!(text.contains("3| **States:** forge-metadata"), "the excerpt stays:\n{text}");
    assert_names_template_file(&layout, &text, "forge-metadata", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "forge-metadata/index.rhei.md");
    assert_offers_no_validate(&text);
    assert!(!layout.panta.join("forge-metadata").exists(), "a dry run writes nothing");
}

/// No project above: the default output is `./forge-metadata`, and the help
/// offers a `rhei validate` with nothing to run it on. §FS-rhei-errors.4.1
#[test]
#[ignore = "red until #658: the standalone dry run names forge-metadata/index.rhei.md and offers \
            rhei validate <plan>; implement removes this"]
fn instantiate_rendered_standalone_dry_run_names_the_template_file() {
    let layout = Layout::standalone("rendered-standalone-dry-run");
    layout.forge_metadata(STATES_LINE_INDEX, TASK);

    let text = layout.refused(&["forge-metadata", "--dry-run"], INDEX_REASON);

    assert_names_template_file(&layout, &text, "forge-metadata", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "forge-metadata/index.rhei.md");
    assert_offers_no_validate(&text);
    assert!(text.contains("rhei instantiate"), "the help re-runs the instantiation:\n{text}");
    assert!(!layout.repo.join("forge-metadata").exists(), "a dry run writes nothing");
}

/// A real run renders in place and removes what it rendered, so the file the
/// refusal names is gone by the time anyone looks. §FS-rhei-errors.4.1
#[test]
#[ignore = "red until #658: the real run names the removed forge-metadata/index.rhei.md as if \
            it existed; implement removes this"]
fn instantiate_rendered_standalone_real_run_names_the_template_not_the_removed_output() {
    let layout = Layout::standalone("rendered-standalone-real");
    layout.forge_metadata(STATES_LINE_INDEX, TASK);

    let text = layout.refused(&["forge-metadata"], INDEX_REASON);

    assert!(!layout.repo.join("forge-metadata").exists(), "the refused output is removed");
    assert_names_template_file(&layout, &text, "forge-metadata", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "forge-metadata/index.rhei.md");
    assert_offers_no_validate(&text);
    assert!(text.contains("rhei instantiate"), "the help re-runs the instantiation:\n{text}");
}

/// The mapping is the template's whole tree, not its index: a task fragment that
/// does not parse, in a member's real run, which renders into hidden staging.
/// §FS-rhei-errors.4.1
#[test]
#[ignore = "red until #658: a staged member's fragment is named at \
            ../panta/forge-metadata/tasks/01-task.md, never the template's; implement removes this"]
fn instantiate_rendered_member_real_run_names_the_template_task_fragment() {
    let layout = Layout::member("rendered-member-fragment");
    layout.forge_metadata("# Rhei: Forge metadata\n", STATES_LINE_TASK);

    let text = layout.refused(&["forge-metadata"], TASK_REASON);

    let left: Vec<_> = fs::read_dir(&layout.panta)
        .expect("read the project")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.contains("forge-metadata"))
        .collect();
    assert!(left.is_empty(), "neither the member nor its staging stays: {left:?}");
    assert_names_template_file(&layout, &text, "forge-metadata", "tasks/01-task.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "forge-metadata/tasks/01-task.md");
    assert_offers_no_validate(&text);
}

/// Kept with `--keep-on-error`, the rendering exists and the help still validates
/// it, but editing it fixes nothing the next instantiation reads, so the refusal
/// still names the template file. §FS-rhei-errors.4.1 §FS-rhei-errors.1.2.1
#[test]
#[ignore = "red until #658: a kept rendering is named without the template file it came from; \
            implement removes this"]
fn instantiate_rendered_kept_refusal_names_the_template_file_too() {
    let layout = Layout::standalone("rendered-kept");
    layout.forge_metadata(STATES_LINE_INDEX, TASK);

    let text = layout.refused(&["forge-metadata", "--keep-on-error"], INDEX_REASON);

    assert!(layout.repo.join("forge-metadata/index.rhei.md").is_file(), "the rendering is kept");
    assert!(text.contains("rhei validate"), "kept output stays repairable:\n{text}");
    assert_names_template_file(&layout, &text, "forge-metadata", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
}

/// A built-in template is extracted into a temporary directory before it renders:
/// the refusal names the template and the file within it, never that directory.
/// The bad line arrives through a value, which the help may name. §FS-rhei-errors.4.1
#[test]
#[ignore = "red until #658: a built-in's refusal names out/index.rhei.md, never \
            analyze-and-dispatch; implement removes this"]
fn instantiate_rendered_builtin_names_the_template_never_its_extraction() {
    let layout = Layout::standalone("rendered-builtin");
    let args = [
        "analyze-and-dispatch",
        "subject=x",
        "analysis_brief=y",
        "plan_title=Broken\n**States:** forge",
        "--output",
        "out",
        "--dry-run",
    ];

    let text = layout.refused(&args, INDEX_REASON);

    assert!(!text.contains("rhei-builtin-template-"), "never the extraction:\n{text}");
    assert!(
        paragraphs(&text)
            .iter()
            .any(|p| p.contains("analyze-and-dispatch") && p.contains("index.rhei.md")),
        "the refusal should name the built-in template and its index.rhei.md:\n{text}"
    );
    assert_rendered_marked(&text, "out/index.rhei.md");
    assert_offers_no_validate(&text);
}

/// A project template's member is rendered from its `includes:` entry's template,
/// so that template's file is named, never a path under the project template.
/// §FS-rhei-errors.4.1 §FS-rhei-templates.6.4
#[test]
#[ignore = "red until #658: a project template's member is named at \
            proj/forge-metadata/index.rhei.md, never its template's; implement removes this"]
fn instantiate_rendered_project_member_names_the_included_template_file() {
    let layout = Layout::standalone("rendered-project-member");
    layout.forge_metadata(STATES_LINE_INDEX, TASK);
    let release =
        "name: release\nversion: 1.0.0\ndescription: A project\nincludes:\n  - forge-metadata\n";
    layout.template(
        "release",
        &[
            ("template.yaml", release),
            ("states.yaml", &machine("release")),
            ("index.panta.md", "# Panta: Release\n"),
        ],
    );

    let text = layout.refused(&["release", "--output", "proj", "--dry-run"], INDEX_REASON);

    assert_names_template_file(&layout, &text, "forge-metadata", "index.rhei.md");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "forge-metadata/index.rhei.md");
    assert_offers_no_validate(&text);
    assert!(!layout.repo.join("proj").exists(), "a dry run writes nothing");
}

/// An `includes:` union renames the part's task file, so the rendered
/// `host/tasks/002-task.md` has no counterpart in `host`: the refusal names no
/// template file that does not exist, and marks the rendering. §FS-rhei-errors.4.1
#[test]
#[ignore = "red until #658: an included fragment is named at host/tasks/002-task.md, unmarked; \
            implement removes this"]
fn instantiate_rendered_included_fragment_names_no_file_that_does_not_exist() {
    let layout = Layout::standalone("rendered-included-fragment");
    layout.forge_metadata("# Rhei: Forge metadata\n", STATES_LINE_TASK);
    let host = "name: host\nversion: 1.0.0\ndescription: A host\nincludes:\n  - forge-metadata\n";
    layout.template(
        "host",
        &[
            ("template.yaml", host),
            ("states.yaml", &machine("host")),
            ("index.rhei.md", "# Rhei: Host\n"),
            ("tasks/01-host.md", "### Task h: Host thing\n**State:** todo\n"),
        ],
    );

    let text = layout.refused(&["host", "--dry-run"], TASK_REASON);

    assert!(text.contains("002-task.md"), "the fixture should reach the renamed fragment:\n{text}");
    assert_template_paths_exist(&layout, &text);
    assert_rendered_marked(&text, "host/tasks/002-task.md");
    assert_offers_no_validate(&text);
}

/// Rendering moved the bad line from the template's line 8 to the rendering's
/// line 4, so the location never presents line 4 as the template file's: it says
/// it is the rendering, or it gives line 8. §FS-rhei-errors.4.1
#[test]
#[ignore = "red until #658: the template file is not named at all, so its line cannot be \
            told from the rendering's; implement removes this"]
fn instantiate_rendered_line_is_not_presented_as_the_template_line() {
    let layout = Layout::standalone("rendered-line-shift");
    let shifted = concat!(
        "{% if false %}\nThis paragraph is left out of the rendering.\n\nSo is this one.\n",
        "{% endif %}\n# Rhei: Forge metadata\n\n**States:** forge-metadata\n",
    );
    let template = layout.forge_metadata(shifted, TASK);
    let source = fs::read_to_string(template.join("index.rhei.md")).expect("read the template");
    assert_eq!(source.lines().nth(7), Some("**States:** forge-metadata"), "line 8 in the template");

    let text = layout.refused(&["forge-metadata", "--dry-run"], INDEX_REASON);

    assert!(text.contains("4| **States:** forge-metadata"), "the rendering's line 4:\n{text}");
    let shown = format!("{TEMPLATE_HOME}/forge-metadata/index.rhei.md");
    assert_names_template_file(&layout, &text, "forge-metadata", "index.rhei.md");
    assert!(!text.contains(&format!("{shown}:4")), "line 4 is not the template's:\n{text}");
    for paragraph in paragraphs(&text) {
        if paragraph.contains("PARSE ERROR") && paragraph.contains(&shown) {
            let lower = paragraph.to_lowercase();
            assert!(
                lower.contains("render") || lower.contains("line 8") || paragraph.contains(":8"),
                "the location names the template file as if line 4 were its own:\n{paragraph}"
            );
        }
    }
}
