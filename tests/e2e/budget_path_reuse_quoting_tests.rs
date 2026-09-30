//! A project at a path an operator plausibly has: one with a space in it.
//!
//! Its own file rather than a ninth case beside the others, because what it pins
//! is a different claim. `budget_path_reuse_tests.rs` pins what the refusal
//! *says*; this pins that what it *offers* is a command line, for a path that is
//! not one shell word. Nothing here is about the classification, so none of that
//! fixture's sequence assertions are repeated.
//!
//! The defect it closes was not a dead end but a command that ran and wrote in
//! the wrong place: unquoted, `mkdir -p <root>/my projects/ss/…` makes `mkdir`
//! read two words, so it creates `<root>/my` and then builds the rest of the
//! tree **relative to the operator's working directory** before the copy fails.
//! A remedy that writes six directory levels somewhere it never named is worse
//! than the refusal this ticket was filed about.

// §FS-rhei-budgets.5.4 §FS-rhei-budgets.10

use super::budget_path_reuse_support::*;
use super::*;

/// Every path the damaged report interpolates into a command is one shell word,
/// and the restore it offers still runs.
// §FS-rhei-budgets.10
#[test]
fn the_commands_offered_for_a_path_with_a_space_are_runnable() {
    let world = world_named("budget-path-reuse-spaced", "my project");
    let (uuid, _) = reuse_the_path(&world);

    let result = world.show();

    assert!(!result.status.success(), "a damaged account still exits non-zero");
    let report = said(&result);
    let target = world.canonical_project();
    assert!(
        target.contains(' ') && shell_quote(&target) != target,
        "the case is only a case while its project path needs quoting; got {target}"
    );
    assert!(
        report.contains(&format!("rhei budget forget {} --reason <TEXT>", shell_quote(&target))),
        "the retirement is offered as one argument, which is what `clap` has to \
         receive; got:\n{report}"
    );
    assert!(
        report.contains(&rhei_core::platform::copy_command(
            &world.witness(&uuid),
            &world.account_dir(&uuid).join("journal.jsonl"),
        )),
        "the copy names both paths as one word each; got:\n{report}"
    );

    let offered = restore_line(&world, &report, &uuid);
    let restored = world.shell(&offered);

    assert!(
        restored.status.success(),
        "the offered restore runs for this path too; `{offered}` said:\n{}",
        stderr(&restored)
    );
    // The proof that it wrote where it said: the account it was offered for now
    // verifies. An unquoted line exits non-zero having built its directories
    // under whatever the working directory was, and leaves this failing.
    let after = world.show();
    assert_success(&after);
    assert!(
        after.stdout.contains("[verified]"),
        "the restore is the whole remedy here as well; got:\n{}",
        after.stdout
    );
}

/// The one line of the report naming both paths, which is the restore — found
/// that way rather than by a leading word, since the word differs per shell.
fn restore_line(world: &World, report: &str, uuid: &str) -> String {
    let witness = world.witness(uuid).display().to_string();
    let journal = world.account_dir(uuid).join("journal.jsonl").display().to_string();
    report
        .lines()
        .map(str::trim)
        .find(|line| line.contains(&witness) && line.contains(&journal))
        .unwrap_or_else(|| panic!("the report offers a restore command; got:\n{report}"))
        .to_string()
}
