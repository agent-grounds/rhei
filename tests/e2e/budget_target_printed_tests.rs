//! Every budget command line rhei prints for a lone plan runs, and names a target
//! the plan verbs take (`agent-grounds/rhei#577`).
//!
//! A directory target still runs today, so running a printed line would pass before
//! the fix and pin nothing. Each case therefore also shows that `rhei list` takes
//! the printed target, and that is the line each `#[ignore]` below fails on. The
//! printed line is run as printed, with only `<N>` and `<TEXT>` filled in, and
//! its text is not otherwise matched. The path needs quoting so the quoting is
//! tested by the shell that has to read it.

// §FS-rhei-budgets.10.1 §FS-rhei-budgets.5.4.1 §FS-rhei-errors.1.2.1 §FS-rhei-errors.2

use std::fs;
use std::path::PathBuf;

use super::budget_copied_project_support as copied;
use super::budget_target_support::*;
use super::*;

const PLAN: &str = "my project/plan.rhei.md";

/// A lone plan whose path is more than one shell word.
fn lone(prefix: &str) -> World {
    let world = world(prefix);
    world.plain("my project");
    assert_ne!(shell_quote(PLAN), PLAN, "the case is only a case while the path needs quoting");
    world
}

/// The one account directory under the lone plan's root.
fn accounts(world: &World) -> PathBuf {
    world.dir.join("my project/.agent-grounds/rhei/budgets")
}

/// `forget` printed for `world`'s damaged account, run: the journal is gone, so a
/// runnable line retires the root.
#[track_caller]
fn assert_forget_retires(world: &World, printed: &str) {
    world.assert_printed_target_is_listable(printed);
    let run = filled_in(printed);
    let retired = world.paste(&run);
    assert!(retired.status.success(), "`{run}` should run:\n{}", retired.stderr);
    assert!(retired.stdout.contains("retired "), "`{run}` retires:\n{}", retired.stdout);
}

/// `forget` refuses a sound account and sends its reader to `adjust`.
// §FS-rhei-budgets.10.1 §FS-rhei-budgets.5.3
#[test]
#[ignore = "red until #577: the sound-account help names the plan's directory; implement removes this"]
fn budget_target_printed_the_sound_account_adjust_runs() {
    let world = lone("budget-printed-adjust");
    world.init(PLAN);

    let refused = world.run(&["budget", "forget", PLAN, "--reason", "probe"]);

    assert_eq!(refused.status.code(), Some(1), "a sound account refuses:\n{}", said_raw(&refused));
    let printed = printed_command(&raw_stderr(&refused), "rhei budget adjust ");
    world.assert_printed_target_is_listable(&printed);
    let run = filled_in(&printed);
    let adjusted = world.paste(&run);
    assert!(adjusted.status.success(), "`{run}` should run:\n{}", adjusted.stderr);
    assert!(adjusted.stdout.contains("lifetime invocation allowance: 6"), "{}", adjusted.stdout);
}

/// `forget` with nothing to retire sends its reader to `show`.
// §FS-rhei-budgets.10.1
#[test]
#[ignore = "red until #577: the nothing-to-retire help names the plan's directory; implement removes this"]
fn budget_target_printed_the_nothing_to_retire_show_runs() {
    let world = lone("budget-printed-nothing");

    let refused = world.run(&["budget", "forget", PLAN, "--reason", "probe"]);

    assert_eq!(refused.status.code(), Some(1), "nothing to retire:\n{}", said_raw(&refused));
    let printed = printed_command(&raw_stderr(&refused), "rhei budget show ");
    world.assert_printed_target_is_listable(&printed);
    let shown = world.paste(&printed);
    assert!(
        shown.stderr.contains("has no budget account yet"),
        "`{printed}` answers for this project:\n{}",
        shown.stderr
    );
}

/// A verb that could not open the account sends its reader to `show`, the help
/// `budget_result_at` writes.
// §FS-rhei-budgets.10.1 §FS-rhei-budgets.5.4
#[test]
#[ignore = "red until #577: the account-error help names the plan's directory; implement removes this"]
fn budget_target_printed_the_account_error_show_runs() {
    let world = lone("budget-printed-account-error");
    world.init(PLAN);
    let account = fs::read_dir(accounts(&world))
        .expect("the account")
        .next()
        .expect("one account")
        .expect("the entry")
        .path();
    let journal = account.join("journal.jsonl");
    let text = fs::read_to_string(&journal).expect("read the journal");
    let first = text.lines().next().expect("a receipt");
    fs::write(&journal, format!("{first}\n")).expect("cut the journal's tail");

    let refused = world.run(&["budget", "adjust", PLAN, "--invocations", "3", "--reason", "x"]);

    assert_eq!(refused.status.code(), Some(1), "a short journal refuses:\n{}", said_raw(&refused));
    let printed = printed_command(&raw_stderr(&refused), "rhei budget show ");
    world.assert_printed_target_is_listable(&printed);
    let shown = world.paste(&printed);
    assert_eq!(shown.status.code(), Some(1), "the account is damaged:\n{}", shown.stderr);
    assert!(
        shown.stdout.contains("fewer receipts"),
        "`{printed}` reports this account:\n{}{}",
        shown.stdout,
        shown.stderr
    );
}

/// The damaged report's `forget`, in text. §FS-rhei-budgets.10
// §FS-rhei-budgets.10 §FS-rhei-budgets.5.4
#[test]
#[ignore = "red until #577: the damaged report's forget names the resolved root; implement removes this"]
fn budget_target_printed_the_damaged_reports_forget_runs() {
    let world = lone("budget-printed-damaged");
    world.init(PLAN);
    fs::remove_dir_all(accounts(&world)).expect("lose the journal");

    let out = world.run(&["budget", "show", PLAN]);

    assert_eq!(out.status.code(), Some(1), "a lost journal is damaged:\n{}", said_raw(&out));
    assert_forget_retires(&world, &printed_command(&stdout(&out), "rhei budget forget "));
}

/// The same `forget` as the `forget` member of the error object. §FS-rhei-budgets.10
// §FS-rhei-budgets.10 §FS-rhei-errors.5
#[test]
#[ignore = "red until #577: the JSON `forget` member names the resolved root; implement removes this"]
fn budget_target_printed_the_damaged_json_forget_runs() {
    let world = lone("budget-printed-damaged-json");
    world.init(PLAN);
    fs::remove_dir_all(accounts(&world)).expect("lose the journal");

    let out = world.run(&["budget", "show", PLAN, "--format", "json"]);

    assert_eq!(out.status.code(), Some(1), "a lost journal is damaged:\n{}", said_raw(&out));
    let error_line = first_line(&out);
    let parsed: serde_json::Value = serde_json::from_str(&error_line)
        .unwrap_or_else(|error| panic!("the JSON error object ({error}):\n{error_line}"));
    let printed = parsed["error"]["forget"].as_str().expect("the sub-case offers `forget`");
    assert_forget_retires(&world, printed);
}

/// A copy whose first charge was refused, with the holder's history to keep safe.
fn refused_copy(prefix: &str) -> (copied::World, String, Vec<u8>, String) {
    let world = copied::world(prefix);
    world.copy("my copy");
    let uuid = world.account_uuid("original");
    let witness = world.witness_bytes("home", &uuid);
    let plan = world.root("my copy").join("plan.rhei.md").display().to_string();
    (world, uuid, witness, plan)
}

/// What a copy's refusal printed, run: its claim is retired and the holder's history
/// stands exactly as it did. §FS-rhei-budgets.5.4.1
#[track_caller]
fn assert_copy_forget_runs(world: &copied::World, uuid: &str, witness: &[u8], said: &str) {
    let printed = printed_command(said, "rhei budget forget ");
    let cwd = world.root("original");
    assert_printed_target_is_listable_with(&world.rhei("home"), &printed, &cwd);
    let run = filled_in(&printed);
    let retired = paste_with(&world.rhei("home"), &run, &cwd);
    assert!(retired.status.success(), "`{run}` should run:\n{}", retired.stderr);
    assert!(world.witness_bytes("home", uuid) == witness, "the holder's history is untouched");
    assert_eq!(world.roots_index("home").get(&world.canonical("original")), Some(&uuid.to_owned()));
}

/// `rhei run` of a copy, refused as `held_elsewhere`.
// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10.1
#[test]
#[ignore = "red until #577: `rhei run` refusing a copy offers forget of the copy's root; implement removes this"]
fn budget_target_printed_the_held_elsewhere_forget_from_rhei_run_runs() {
    let (world, uuid, witness, plan) = refused_copy("budget-printed-held-run");

    let mut cmd = world.rhei("home");
    cmd.arg("--state-machine")
        .arg(world.root("states.yaml"))
        .arg("run")
        .arg(&plan)
        .args(["--no-tui", "--no-callbacks"]);
    let out = cmd.output().expect("rhei run should run");

    assert!(!out.status.success(), "a copy is refused:\n{}", said_raw(&out));
    assert_copy_forget_runs(&world, &uuid, &witness, &said_raw(&out));
}

/// The copied-account refusal of a hand edge, the sentence §FS-rhei-budgets.5.4.1 writes.
// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10.1
#[test]
#[ignore = "red until #577: a refused `rhei transition` on a copy offers forget of the copy's root; implement removes this"]
fn budget_target_printed_the_copied_account_forget_from_a_hand_edge_runs() {
    let (world, uuid, witness, _) = refused_copy("budget-printed-held-edge");

    let refused = world.edge("my copy", "work", "review");

    assert!(!refused.status.success(), "a copy is refused:\n{}", copied::said(&refused));
    assert_copy_forget_runs(&world, &uuid, &witness, &copied::said(&refused));
}
