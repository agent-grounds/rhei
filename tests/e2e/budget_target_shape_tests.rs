//! `rhei budget` takes what the plan verbs take (`agent-grounds/rhei#577`).
//!
//! Each refusal is compared with what `rhei list` says of the same target. The
//! reproducer's own disagreement was that `init` wrote an account in a directory
//! every plan verb refuses.

// §FS-rhei-budgets.10.1 §FS-rhei-errors.3.2

use std::fs;

use super::budget_target_support::*;
use super::*;

/// The issue's first case: nothing is written and every verb says what `list` says.
// §FS-rhei-budgets.10.1
#[test]
fn budget_target_a_plain_directory_is_refused_by_every_verb() {
    let world = world("budget-target-plain");
    world.plain("dir");

    let refusal = world.list_refusal(&world.dir, "dir");

    world.assert_every_verb_refuses(&world.dir, "dir", &refusal, "dir");
}

/// The same directory spelled from inside it, and a plain directory inside a
/// project, which `rhei list` refuses though a project encloses it.
// §FS-rhei-budgets.10.1 §FS-rhei-panta.6
#[test]
fn budget_target_a_plain_directory_is_refused_however_it_is_reached() {
    let world = world("budget-target-plain-reached");
    world.plain("dir");
    world.project("proj", &["m"]);
    world.write("proj/sub/notes.md", "notes\n");

    let inside = world.dir.join("dir");
    let refusal = world.list_refusal(&inside, ".");
    world.assert_every_verb_refuses(&inside, ".", &refusal, "dir");

    let refusal = world.list_refusal(&world.dir, "proj/sub");
    world.assert_every_verb_refuses(&world.dir, "proj/sub", &refusal, "proj");
}

/// A file whose text parses reaches the rule `rhei list` applies to a file's name,
/// beside a plan, in a workspace's `tasks/` and in a project.
// §FS-rhei-budgets.10.1
#[test]
fn budget_target_a_non_plan_file_is_refused_as_rhei_list_refuses_it() {
    let world = world("budget-target-file");
    world.plain("lone");
    world.workspace("ws");
    world.project("proj", &["m"]);

    for (target, watched) in
        [("lone/README.md", "lone"), ("ws/tasks/02-b.md", "ws"), ("proj/README.md", "proj")]
    {
        world.write(target, PLAN_TEXT);
        let refusal = world.list_refusal(&world.dir, target);
        world.assert_every_verb_refuses(&world.dir, target, &refusal, watched);
    }
}

/// A real README and a real task file hold no plan's text. `rhei list` reports the
/// parse first; `rhei budget` parses no task body, so it says the name's refusal.
// §FS-rhei-budgets.10.1
#[test]
fn budget_target_a_non_plan_file_is_refused_without_parsing_it() {
    let world = world("budget-target-file-text");
    world.plain("lone");
    world.workspace("ws");
    world.write("lone/README.md", "# A readme, and no plan\n");

    for (target, watched) in [("lone/README.md", "lone"), ("ws/tasks/01-a.md", "ws")] {
        let parse = world.run(&["list", target]);
        assert_eq!(
            parse.status.code(),
            Some(1),
            "`rhei list {target}` refuses a file of this text"
        );
        let refusal = world.list_refusal_for_name(target);
        world.assert_every_verb_refuses(&world.dir, target, &refusal, watched);
    }
}

/// A name that gives no rhei id is refused on a file and on a workspace, in `rhei list`'s
/// words, because neither a parse error nor a project stands in front of it here.
// §FS-rhei-budgets.10.1 §AR-rhei-panta.3
#[test]
fn budget_target_a_name_that_gives_no_id_is_refused_by_every_verb() {
    let world = world("budget-target-name");
    world.write("lone/1bad.rhei.md", PLAN_TEXT);
    world.write("lone/basin.rhei.md", PLAN_TEXT);
    world.workspace("Bad.Name");
    world.workspace("basin");

    for (target, watched) in [
        ("lone/1bad.rhei.md", "lone"),
        ("lone/basin.rhei.md", "lone"),
        ("Bad.Name", "Bad.Name"),
        ("basin", "basin"),
    ] {
        let refusal = world.list_refusal(&world.dir, target);
        world.assert_every_verb_refuses(&world.dir, target, &refusal, watched);
    }
}

/// The same names as members of a project, which resolving widens to the project:
/// the member's own name is what is judged. `rhei list` reports these as a rhei it
/// could not load, so budget's own words are held instead of its first line.
// §FS-rhei-budgets.10.1 §AR-rhei-panta.3 §FS-rhei-panta.6
#[test]
fn budget_target_a_project_member_whose_name_gives_no_id_is_refused() {
    let world = world("budget-target-member-name");
    let init = world.run(&["init"]);
    assert!(init.status.success(), "`rhei init`:\n{}", raw_stderr(&init));
    world.write("panta/1bad.rhei.md", PLAN_TEXT);
    world.write("panta/basin.rhei.md", PLAN_TEXT);
    world.workspace("panta/Bad.Name");

    for (target, said) in [
        ("panta/1bad.rhei.md", "rhei id '1bad'"),
        ("panta/basin.rhei.md", "`basin` is reserved"),
        ("panta/Bad.Name", "rhei id 'Bad.Name'"),
    ] {
        let listed = world.run(&["list", target]);
        assert_eq!(listed.status.code(), Some(1), "`list {target}`:\n{}", raw_stderr(&listed));
        let before = world.bytes_under("panta");
        for verb in every_verb(target) {
            let out = world.run(&verb);
            assert_eq!(out.status.code(), Some(1), "`{verb:?}`:\n{}", raw_stderr(&out));
            assert!(first_line(&out).contains(said), "`{verb:?}`:\n{}", raw_stderr(&out));
            assert!(world.bytes_under("panta") == before, "`{verb:?}` changed something");
        }
    }
}

/// An account already there is out of reach, not altered: the reproducer's seeding,
/// so this still means something once `init` refuses.
// §FS-rhei-budgets.10.1
#[test]
fn budget_target_an_account_under_a_lost_manifest_is_refused_and_untouched() {
    let world = world("budget-target-seeded");
    world.workspace("seeded");
    world.init("seeded");
    fs::remove_file(world.dir.join("seeded/index.rhei.md")).expect("remove the manifest");

    let refusal = world.list_refusal(&world.dir, "seeded");

    world.assert_every_verb_refuses(&world.dir, "seeded", &refusal, "seeded");
}

/// Shape outranks settings, so the help cannot offer a `rhei validate` that refuses
/// the target it names: the issue's other complaint.
// §FS-rhei-budgets.10.1 §FS-rhei-errors.1.2.1
#[test]
fn budget_target_the_refusal_comes_before_settings_are_read() {
    let world = world("budget-target-settings");
    world.plain("broken");
    world.write("broken/.agent-grounds/rhei/settings.json", "{ not json");

    let refusal = world.list_refusal(&world.dir, "broken");

    world.assert_every_verb_refuses(&world.dir, "broken", &refusal, "broken");
}

// ---- Positive controls: these pass today and hold what the refusal must not take away.

/// Every shape `rhei list` takes is taken here too, and all four verbs run on it.
// §FS-rhei-budgets.10.1 §FS-rhei-panta.6
#[test]
fn budget_target_control_the_accepted_shapes_stay_accepted() {
    let world = world("budget-target-accepted");
    world.plain("lone");
    world.workspace("ws");
    world.project("proj", &["m"]);

    for target in ["lone/plan.rhei.md", "ws", "proj"] {
        let listed = world.run(&["list", target]);
        assert!(listed.status.success(), "`list {target}`:\n{}", raw_stderr(&listed));
        let [init, show, adjust, forget] = every_verb(target);
        for verb in [init, show, adjust] {
            let out = world.run(&verb);
            assert!(out.status.success(), "`{verb:?}`:\n{}", raw_stderr(&out));
        }
        let sound = world.run(&forget);
        assert!(
            said_raw(&sound).contains("retiring a sound account would recreate capacity"),
            "`forget` reaches the account of {target}:\n{}",
            said_raw(&sound)
        );
    }
}

/// A manifest named by its file is the directory it stands for: one account.
// §FS-rhei-budgets.10.1 §FS-rhei-panta.6
#[test]
fn budget_target_control_a_manifest_file_is_its_directory() {
    let world = world("budget-target-manifest");
    world.workspace("ws");
    world.project("proj", &["m"]);

    for (directory, manifest) in [("ws", "ws/index.rhei.md"), ("proj", "proj/index.panta.md")] {
        world.init(manifest);
        assert_eq!(world.project_id(directory), world.project_id(manifest), "{manifest}");
    }
}

/// A project's `basin/` holds no manifest of its own and is the project's, as
/// `rhei list` takes it, unlike a workspace or a file claiming that name.
// §FS-rhei-budgets.10.1 §FS-rhei-panta.6
#[test]
fn budget_target_control_a_projects_basin_reaches_the_projects_account() {
    let world = world("budget-target-basin");
    world.project("proj", &["m"]);
    fs::create_dir_all(world.dir.join("proj/basin")).expect("create the basin");
    world.init("proj");

    let listed = world.run(&["list", "proj/basin"]);
    assert!(listed.status.success(), "`list proj/basin`:\n{}", raw_stderr(&listed));
    assert_eq!(world.project_id("proj/basin"), world.project_id("proj"));
}

/// With no target the verbs still discover, from a project and from a plain
/// directory inside it, though that directory named outright is refused.
// §FS-rhei-budgets.10.1 §FS-rhei-panta.6
#[test]
fn budget_target_control_an_omitted_target_still_discovers_the_project() {
    let world = world("budget-target-omitted");
    world.project("proj", &["m"]);
    world.write("proj/sub/notes.md", "notes\n");
    world.init("proj");
    let id = world.project_id("proj");

    for cwd in [world.dir.join("proj"), world.dir.join("proj/sub")] {
        let out = world.run_in(&cwd, &["budget", "show", "--format", "json"]);
        assert!(out.status.success(), "from {}:\n{}", cwd.display(), raw_stderr(&out));
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
        assert_eq!(report["project_id"], id.as_str(), "from {}", cwd.display());
    }
}

/// A plan that does not parse is still inspected: `show` is where a refusal sends
/// its reader, and recognizing the target reads no task body.
// §FS-rhei-budgets.10.1 §FS-rhei-budgets.10
#[test]
fn budget_target_control_a_malformed_task_body_is_still_inspectable() {
    let world = world("budget-target-malformed");
    world.write("lone/plan.rhei.md", "not a plan at all\n");
    world.project("proj", &[]);
    world.write("proj/m.rhei.md", "not a plan at all\n");

    for target in ["lone/plan.rhei.md", "proj"] {
        let checked = world.run(&["validate", target]);
        assert_eq!(checked.status.code(), Some(1), "`validate {target}` refuses what it parses");
        world.init(target);
        world.project_id(target);
    }
}

/// One member named is the whole project's one account, and `--rhei` narrows display
/// only.
// §FS-rhei-budgets.10 §FS-rhei-panta.6
#[test]
fn budget_target_control_a_member_reaches_the_whole_projects_account() {
    let world = world("budget-target-member");
    world.project("proj", &["a", "b"]);
    world.init("proj/a.rhei.md");

    assert_eq!(world.project_id("proj/a.rhei.md"), world.project_id("proj/b.rhei.md"));
    assert_eq!(world.project_id("proj/a.rhei.md"), world.project_id("proj"));
    let adjust = ["budget", "adjust", "proj/b.rhei.md", "--invocations", "9", "--reason", "x"];
    assert!(world.run(&adjust).status.success(), "the other member changes the one allowance");
    let shown = world.run(&["budget", "show", "proj/a.rhei.md", "--rhei", "b"]);
    assert!(stdout(&shown).contains("one account, one balance"), "{}", stdout(&shown));
}

/// The way back from a lost manifest: a plan in that directory reaches the account
/// with its identity intact, and so does putting the manifest back.
// §FS-rhei-budgets.10.1
#[test]
fn budget_target_control_a_lost_manifest_has_a_way_back_to_its_account() {
    let world = world("budget-target-way-back");
    world.workspace("seeded");
    world.init("seeded");
    let id = world.project_id("seeded");
    fs::remove_file(world.dir.join("seeded/index.rhei.md")).expect("remove the manifest");

    world.write("seeded/plan.rhei.md", PLAN_TEXT);
    assert_eq!(world.project_id("seeded/plan.rhei.md"), id, "a plan in the same root");

    fs::remove_file(world.dir.join("seeded/plan.rhei.md")).expect("remove the plan");
    world.write("seeded/index.rhei.md", "# Rhei: W\n");
    assert_eq!(world.project_id("seeded"), id, "the manifest put back");
}

/// On a target the plan verbs take, settings are read as ever, and the help's
/// `rhei validate` runs on the target it names.
// §FS-rhei-budgets.10.1 §FS-rhei-errors.1.2.1
#[test]
fn budget_target_control_broken_settings_on_a_plan_keep_their_runnable_help() {
    let world = world("budget-target-plan-settings");
    world.plain("lone");
    world.write("lone/.agent-grounds/rhei/settings.json", "{ not json");

    let out = world.run(&["budget", "show", "lone/plan.rhei.md"]);

    assert_eq!(out.status.code(), Some(1), "the settings refuse:\n{}", raw_stderr(&out));
    let validate = printed_command(&raw_stderr(&out), "rhei validate ");
    let again = world.paste(&validate);
    assert!(
        again.stderr.contains("settings.json") && !again.stderr.contains("not a recognized"),
        "`{validate}` reports the settings:\n{}",
        again.stderr
    );
}
