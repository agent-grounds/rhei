//! A template's bundled `settings.json` that does not load. Materialize moves a
//! template's root `settings.json` into the rendering's
//! `.agent-grounds/rhei/settings.json`, so a refusal of that file names the
//! template's `settings.json` it came from, marks the rendered path with what
//! became of it, and re-checks nothing that is gone. §FS-rhei-errors.4.1

use std::fs;
use std::path::Path;

use super::instantiate_rendered_refusal_support::*;

/// A bound the settings decoder refuses: a count written as a word.
const BAD_DECODE: &str = "{\"defaults\": {\"transition_limit\": \"lots\"}}\n";

/// What the decoder says about it.
const DECODE_REASON: &str = "invalid type: string \"lots\", expected u64";

/// Where the rendered settings land under a rendered or placed rhei.
const RENDERED_SETTINGS: &str = ".agent-grounds/rhei/settings.json";

/// A template `set-bad` whose own files load, shipping `settings` at its root.
fn set_bad(layout: &Layout, settings: &str, extra_manifest: &str) {
    let manifest =
        format!("name: set-bad\nversion: 1.0.0\ndescription: Bad settings\n{extra_manifest}");
    layout.template(
        "set-bad",
        &[
            ("template.yaml", &manifest),
            ("states.yaml", &machine("set-bad")),
            ("index.rhei.md", "# Rhei: Set bad\n"),
            ("tasks/01-set.md", "### Task s: Set thing\n**State:** todo\n"),
            ("settings.json", settings),
        ],
    );
}

/// `rhei instantiate <args>` refused for `reason`, with nothing left behind in
/// scratch and scratch never named.
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

/// A discarded rendering's settings refusal: the template's `settings.json` is
/// named, the rendered path under `rendered_root` is marked in the paragraph that
/// shows it, and the help retries the instantiation instead of validating.
/// §FS-rhei-errors.4.1
fn assert_settings_discarded(layout: &Layout, text: &str, rendered_root: &str) {
    let rendered = format!("{rendered_root}/{RENDERED_SETTINGS}");
    assert!(text.contains(&rendered), "the fixture should reach the rendered settings:\n{text}");
    assert_rendered_marked(text, &rendered);
    assert_names_template_file(layout, text, "set-bad", "settings.json");
    assert_template_paths_exist(layout, text);
    assert_offers_no_validate(text);
    assert!(text.contains("rhei instantiate"), "the help re-runs the instantiation:\n{text}");
}

#[test]
fn instantiate_rendered_settings_dry_run_names_the_template_file() {
    let layout = Layout::standalone("rendered-settings-dry-run");
    set_bad(&layout, BAD_DECODE, "");

    let text = refused(&layout, &["set-bad", "--dry-run"], DECODE_REASON);

    assert_settings_discarded(&layout, &text, "set-bad");
    assert!(!layout.repo.join("set-bad").exists(), "a dry run writes nothing");
}

#[test]
fn instantiate_rendered_settings_real_run_names_the_template_file() {
    let layout = Layout::standalone("rendered-settings-real");
    set_bad(&layout, BAD_DECODE, "");

    let text = refused(&layout, &["set-bad"], DECODE_REASON);

    assert_settings_discarded(&layout, &text, "set-bad");
    assert!(text.contains("removed"), "the rendering says it was removed:\n{text}");
    assert!(!layout.repo.join("set-bad").exists(), "the refused output is removed");
}

/// The placement header stays as it is; the nested refusal marks the member's
/// staged settings. §FS-rhei-errors.4.1 §FS-rhei-library.7.5
#[test]
fn instantiate_rendered_settings_of_a_member_names_the_template_file() {
    let layout = Layout::member("rendered-settings-member");
    set_bad(&layout, BAD_DECODE, "");

    let text = refused(&layout, &["set-bad", "--dry-run"], DECODE_REASON);

    assert!(text.contains("Placement of proposed member"), "the header stays:\n{text}");
    let failed = paragraphs(&text)
        .into_iter()
        .find(|paragraph| paragraph.contains("failed to decode settings"))
        .unwrap_or_else(|| panic!("the fixture should reach the settings' decode:\n{text}"));
    assert!(failed.contains("not written"), "the nested refusal is marked:\n{failed}");
    assert_settings_discarded(&layout, &text, "panta/set-bad");
    assert!(!layout.panta.join("set-bad").exists(), "a dry run writes nothing");
}

/// A rhei `host` beside the repository with no settings of its own.
fn host_rhei(root: &Path) {
    fs::create_dir_all(root.join("tasks")).expect("create the host");
    fs::write(root.join("states.yaml"), machine("host")).expect("write the host machine");
    fs::write(root.join("index.rhei.md"), "# Rhei: Host\n").expect("write the host plan");
    fs::write(root.join("tasks/01-host.md"), "### Task h: Host thing\n**State:** todo\n")
        .expect("write the host task");
}

/// `--into` a rhei copies the template's settings in as a new file of the host's:
/// that file is the rendering, named by the template's `settings.json`.
/// §FS-rhei-errors.4.1 §FS-rhei-library.7.5
#[test]
fn instantiate_rendered_settings_into_rhei_names_the_template_file() {
    let layout = Layout::standalone("rendered-settings-into");
    host_rhei(&layout.repo.join("host"));
    set_bad(&layout, BAD_DECODE, "");

    let text = refused(&layout, &["set-bad", "--into", "host", "--dry-run"], DECODE_REASON);

    assert_settings_discarded(&layout, &text, "host");
    assert!(!layout.repo.join("host/.agent-grounds").exists(), "nothing was placed");
}

/// `--into` a member of a Panta project with no settings of its own hoists the
/// template's settings as the project's new file: the rendering, too.
/// §FS-rhei-errors.4.1 §FS-rhei-templates.6.2
#[test]
fn instantiate_rendered_settings_into_a_member_names_the_template_file() {
    let layout = Layout::member("rendered-settings-into-member");
    host_rhei(&layout.panta.join("m1"));
    set_bad(&layout, BAD_DECODE, "");

    let text = refused(&layout, &["set-bad", "--into", "../panta/m1", "--dry-run"], DECODE_REASON);

    assert_settings_discarded(&layout, &text, "panta");
    assert!(!layout.panta.join(RENDERED_SETTINGS).exists(), "nothing was placed");
}

/// Kept, the rendered settings exist and their re-check stands, while the
/// refusal still names the template's file. §FS-rhei-errors.4.1 §FS-rhei-errors.1.2.1
#[test]
fn instantiate_rendered_settings_kept_keeps_its_re_check() {
    let layout = Layout::standalone("rendered-settings-kept");
    set_bad(&layout, BAD_DECODE, "");

    let text = refused(&layout, &["set-bad", "--keep-on-error"], DECODE_REASON);

    assert!(layout.repo.join("set-bad").join(RENDERED_SETTINGS).is_file(), "the rendering is kept");
    let failed = paragraphs(&text)
        .into_iter()
        .find(|paragraph| paragraph.contains("failed to decode settings"))
        .unwrap_or_else(|| panic!("the fixture should reach the settings' decode:\n{text}"));
    assert!(failed.contains("kept for inspection"), "the kept copy is named kept:\n{failed}");
    assert!(text.contains("rhei validate"), "the kept copy is re-checked:\n{text}");
    assert_names_template_file(&layout, &text, "set-bad", "settings.json");
    assert_template_paths_exist(&layout, &text);
}

/// A profile price the loader refuses before decoding is the template's too.
/// §FS-rhei-errors.4.1 §FS-rhei-agents.1.1.3
#[test]
fn instantiate_rendered_settings_profile_price_names_the_template_file() {
    let layout = Layout::standalone("rendered-settings-price");
    set_bad(&layout, "{\"models\": {\"m\": {\"prices\": 5}}}\n", "");

    let text = refused(&layout, &["set-bad", "--dry-run"], "models.m.prices must be an object");

    assert_settings_discarded(&layout, &text, "set-bad");
}

/// The machine-only key, refused by its presence in the rendered settings,
/// names the template's file. §FS-rhei-errors.4.1 §FS-rhei-agents.1.1.1
#[test]
fn instantiate_rendered_settings_machine_only_key_names_the_template_file() {
    let layout = Layout::standalone("rendered-settings-clamp");
    set_bad(&layout, "{\"defaults\": {\"clamp_projects\": false}}\n", "");

    let text =
        refused(&layout, &["set-bad", "--dry-run"], "may only be set in the machine settings file");

    assert_settings_discarded(&layout, &text, "set-bad");
}

/// An `includes:` union joins its templates' settings into the same file, so the
/// refusal says the setting may be in one of those templates instead.
/// §FS-rhei-errors.4.1 §FS-rhei-library.3.1
#[test]
fn instantiate_rendered_settings_with_includes_points_at_the_included_templates_too() {
    let layout = Layout::standalone("rendered-settings-includes");
    layout.forge_metadata("# Rhei: Forge metadata\n", TASK);
    set_bad(&layout, BAD_DECODE, "includes:\n  - forge-metadata\n");

    let text = refused(&layout, &["set-bad", "--dry-run"], DECODE_REASON);

    assert_settings_discarded(&layout, &text, "set-bad");
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(folded.contains("or a template it includes"), "{text}");
}
