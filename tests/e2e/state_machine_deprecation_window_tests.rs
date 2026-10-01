//! The one-release window: the `**States:**` declaration and the cross-root
//! `name:` match still resolve, still win wherever they resolve, and warn where
//! they disagree with the new resolution.
//!
//! Each case asserts the resolution and the warning **separately**, because they
//! are two different claims: that the resolution is the previous release's is the
//! regression control for every laid plan, and that one `warning:` line appears
//! beside it is the deprecation. A tree in the shape every instantiated template
//! ships asserts the absence of both.

// §FS-rhei-states-deprecation

use super::new_tests::{assert_failure, flattened_output};
use super::state_machine_resolution_support::*;
use super::*;

/// A member restating the project default with some other `states.yaml` in its
/// own root keeps running under the project default for this release, and is
/// told that its own file takes over in the next one. The two machines share no
/// state, so the resolution assertion cannot pass by coincidence.
/// §FS-rhei-states-deprecation.2.2
#[test]
fn a_still_resolving_declaration_defers_the_own_root_file() {
    let dir = unique_temp_dir("window-deferred-own-root");
    let home = dir.join(".home");
    let root = project(&dir, Some("proj-machine"));
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    let rhei = member(&root, "m1", Some("proj-machine"), "surveying");
    member_machine(&rhei, &machine("leftover", "drafting", "filed"));
    let project_arg = root.display().to_string();

    // The resolution: the previous release's, unchanged.
    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));
    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &default_source(&root.join("states.yaml")));
    assert!(
        !states.stdout.contains("leftover") && !states.stdout.contains("drafting"),
        "the own-root file must not take over this release; got:\n{}",
        states.stdout
    );

    // The deprecation: one line saying the file is deferred, and until when.
    assert_one_warning(
        &states,
        &[
            "m1",
            "proj-machine",
            &rhei.join("states.yaml").display().to_string(),
            "leftover",
            "next release",
        ],
    );
}

/// A member restating the project's effective built-in `rhei` default keeps the
/// built-in machine this release even though its own root now holds a file named
/// `rhei`: a declaration that resolves to the built-in machine counts as
/// resolving. This is `agent-grounds/rhei#244`'s contract, carried through the
/// window with a warning rather than broken by it.
/// §FS-rhei-states-deprecation.1 §FS-rhei-states-deprecation.2.2
#[test]
fn restating_the_builtin_default_keeps_the_builtin_for_one_release() {
    let dir = unique_temp_dir("window-restated-builtin");
    let home = dir.join(".home");
    let root = project(&dir, Some("rhei"));
    let rhei = member(&root, "m1", Some("rhei"), "drafting");
    member_machine(&rhei, &machine("rhei", "drafting", "filed"));
    let project_arg = root.display().to_string();

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert!(
        states.stdout.contains("Source: the built-in default state machine")
            && !states.stdout.contains(&rhei.join("states.yaml").display().to_string()),
        "the member-root file must not replace the built-in default; got:\n{}",
        states.stdout
    );

    let validation = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_failure(&validation, "drafting");
    let said = flattened_output(&validation);
    assert!(
        said.contains("built-in default state machine")
            && said.contains("invalid state 'drafting'"),
        "the member's own state stays invalid under the built-in machine; got:\n{said}"
    );

    assert_one_warning(&states, &["m1", &rhei.join("states.yaml").display().to_string()]);
}

/// The manifest's own default declaration resolved from a member root still
/// resolves, and warns once for the whole run however many rheis inherit it —
/// the subject of the warning is the declaration, not the lookup.
/// §FS-rhei-states-deprecation.2.3 §FS-rhei-states-deprecation.3
#[test]
fn a_cross_root_match_resolves_the_project_default_and_warns_once() {
    let dir = unique_temp_dir("window-cross-root-default");
    let home = dir.join(".home");
    let root = project(&dir, Some("alpha"));
    member(&root, "m1", None, "surveying");
    let m2 = member(&root, "m2", None, "surveying");
    member_machine(&m2, &machine("alpha", "surveying", "signed-off"));
    member(&root, "m3", None, "surveying");
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));
    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &default_source(&m2.join("states.yaml")));

    assert_one_warning(
        &states,
        &["alpha", &m2.join("states.yaml").display().to_string(), "next release"],
    );
}

/// The sharper crossing: a member declares `mach-x`, its own root holds the
/// stale `mach-y`, and a sibling root holds the definitive `mach-x`. The
/// sibling's file still resolves, not the local one. Both the crossing and the
/// deferral are true of this one declaration and only the crossing is printed,
/// because it names the file that actually resolved.
/// §FS-rhei-states-deprecation.2
#[test]
fn a_member_declaration_resolved_from_another_root_warns_about_the_crossing() {
    let dir = unique_temp_dir("window-cross-root-member");
    let home = dir.join(".home");
    let root = project(&dir, Some("alpha"));
    project_machine(&root, &machine("alpha", "surveying", "signed-off"));
    let m1 = member(&root, "m1", Some("mach-x"), "crossing");
    member_machine(&m1, &machine("mach-y", "stale", "gone"));
    let m2 = member(&root, "m2", Some("mach-x"), "crossing");
    member_machine(&m2, &machine("mach-x", "crossing", "crossed"));
    let project_arg = root.display().to_string();

    assert_validates(&rhei_in(&dir, &home, &["validate", &project_arg]));
    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &rhei_source(&m2.join("states.yaml"), "m1, m2"));
    assert!(
        !states.stdout.contains("mach-y") && !states.stdout.contains("stale"),
        "the stale local file must not win; got:\n{}",
        states.stdout
    );

    assert_one_warning(
        &states,
        &["m1", "mach-x", &m2.join("states.yaml").display().to_string(), "another rhei's root"],
    );
}

/// The shape every instantiated template ships — a rhei declaring `custom` with
/// `custom` in its own root, beside a rhei with no file of its own that inherits
/// the project default — prints no warning on any command. Every laid plan has
/// this shape, and this is the test that keeps a `warning:` line off all of them.
/// §FS-rhei-states-deprecation.2
#[test]
fn the_template_shape_prints_no_warning() {
    let dir = unique_temp_dir("window-template-shape");
    let home = dir.join(".home");
    let root = project(&dir, Some("alpha"));
    project_machine(&root, &machine("alpha", "surveying", "signed-off"));
    let m1 = member(&root, "m1", Some("custom"), "drafting");
    member_machine(&m1, &machine("custom", "drafting", "filed"));
    member(&root, "m2", None, "surveying");
    let project_arg = root.display().to_string();

    let validation = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_validates(&validation);
    assert_silent(&validation, "`rhei validate` on the template shape");

    let states = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&states);
    assert_source(&states, &rhei_source(&m1.join("states.yaml"), "m1"));
    assert_silent(&states, "`rhei states` on the template shape");

    let list = rhei_in(&dir, &home, &["list", &project_arg]);
    assert_success(&list);
    assert_silent(&list, "`rhei list` on the template shape");
}

/// Several candidate roots holding one custom default's name is still the
/// ambiguity error, with its candidates and both of its fixes: a stale copy
/// silently driving tickets is worse than asking once.
/// §FS-rhei-states-deprecation.1
#[test]
fn the_cross_root_ambiguity_is_still_an_error() {
    let dir = unique_temp_dir("window-cross-root-ambiguous");
    let home = dir.join(".home");
    let root = project(&dir, Some("alpha"));
    for id in ["m1", "m2"] {
        let rhei = member(&root, id, None, "surveying");
        member_machine(&rhei, &machine("alpha", "surveying", "signed-off"));
    }
    let project_arg = root.display().to_string();

    let result = rhei_in(&dir, &home, &["validate", &project_arg]);
    assert_failure(&result, "alpha");
    let said = flattened_output(&result);
    for fragment in ["more than one rhei root", "m1", "m2", "project root", "--state-machine"] {
        assert!(
            said.contains(fragment),
            "the ambiguity error should name {fragment:?}; got:\n{said}"
        );
    }
}

/// The warning's contract: a `warning:` line on stderr, so `--json` stdout stays
/// parseable. §FS-rhei-states-deprecation.3
#[test]
fn the_warning_is_on_stderr_and_leaves_json_parseable() {
    let dir = unique_temp_dir("window-json-stdout");
    let home = dir.join(".home");
    let root = project(&dir, Some("proj-machine"));
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    let rhei = member(&root, "m1", Some("proj-machine"), "surveying");
    member_machine(&rhei, &machine("leftover", "drafting", "filed"));
    let project_arg = root.display().to_string();

    let states = rhei_in(&dir, &home, &["states", &project_arg, "--json"]);
    assert_success(&states);
    serde_json::from_str::<serde_json::Value>(&states.stdout)
        .expect("`--json` stdout should parse with the warning on stderr");
    assert!(
        !states.stdout.contains("warning:"),
        "the warning must not reach stdout; got:\n{}",
        states.stdout
    );
    assert_one_warning(&states, &["m1", "leftover"]);
}

/// A completion request is answered by a fresh process per Tab press, so the
/// once-per-process guard cannot hold there and the warning is suppressed
/// instead. The control is the same tree under an ordinary command, so the
/// silence above is the suppression and not a quiet fixture.
/// §FS-rhei-states-deprecation.3
#[test]
fn shell_completion_is_silent_about_the_deprecated_resolution() {
    let dir = unique_temp_dir("window-completion-quiet");
    let home = dir.join(".home");
    let root = project(&dir, Some("proj-machine"));
    project_machine(&root, &machine("proj-machine", "surveying", "signed-off"));
    let rhei = member(&root, "m1", Some("proj-machine"), "surveying");
    member_machine(&rhei, &machine("leftover", "drafting", "filed"));
    let project_arg = root.display().to_string();

    let mut cmd = rhei_command(&home);
    cmd.current_dir(&dir).env("COMPLETE", "fish").args(["--", "rhei", "states", ""]);
    let completion = CliRun::from(&cmd.output().expect("rhei command should run"));
    assert_silent(&completion, "a Tab press");

    let ordinary = rhei_in(&dir, &home, &["states", &project_arg]);
    assert_success(&ordinary);
    assert_one_warning(&ordinary, &["m1", "leftover"]);
}
