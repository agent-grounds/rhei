//! A project whose budget journal is there but zero bytes long, with no witness
//! history for its uuid (`agent-grounds/rhei#540`).
//!
//! That is the lawful **absent** state, and the first charge establishes it by
//! taking the empty journal as the account's own
//! ([§FS-rhei-budgets.5.4](../../docs/functional-spec/rhei-budgets.spec.md)).
//! Before the fix every charge on it failed with `File exists (os error 17)`
//! and left both files as they were, while `rhei budget show` called the account
//! untrustworthy and sent the operator back to `rhei budget show`.

// §FS-rhei-budgets.5.4 §FS-rhei-budgets.10

use std::fs;

use super::budget_torn_witness_support::*;
use super::*;

/// An initialized and charged project whose journal was then emptied from
/// outside rhei and whose witness is gone, the state the ticket reaches by
/// hand. The witness's directory and the index entry stay, as they did there.
fn emptied(prefix: &str) -> World {
    let world = world(prefix);
    let (journal, witness) = (world.journal(), world.witness());
    fs::write(&journal, b"").expect("empty the journal");
    fs::remove_file(&witness).expect("delete the witness");
    world
}

/// The journal and the witness after a charge established the account: the
/// same receipts on both sides, `at_least` of them.
fn assert_established(world: &World, at_least: usize) {
    let (journal, witness) = (read(&world.journal()), read(&world.witness()));
    assert!(
        receipts(&journal) >= at_least,
        "the journal holds the new account's receipts: {} of them",
        receipts(&journal)
    );
    assert!(journal == witness, "the witness holds the journal's receipts, byte for byte");
}

/// `rhei budget init`, the command that establishes an account, does so on an
/// empty journal: the `initialize` receipt and the allowance land in it.
// §FS-rhei-budgets.5.4
#[test]
#[ignore = "red until #540: budget init on an empty journal fails with 'File exists \
            (os error 17)'; implement removes this"]
fn budget_init_establishes_an_account_whose_journal_is_empty() {
    let world = emptied("budget-empty-init");
    let plan = world.plan().display().to_string();

    let init = world.budget(&["init", &plan, "--invocations", "10", "--reason", "establish again"]);

    assert!(init.status.success(), "budget init establishes the account:\n{}", said(&init));
    assert!(said(&init).contains("lifetime invocation allowance: 10"), "{}", said(&init));
    assert_established(&world, 2);
    assert_success(&world.edge("work", "review"));
}

/// An applied edge, the other first charge, establishes the account on an empty
/// journal and moves the task.
// §FS-rhei-budgets.5.4
#[test]
#[ignore = "red until #540: an applied edge on an empty journal fails with 'File exists \
            (os error 17)'; implement removes this"]
fn an_applied_edge_establishes_an_account_whose_journal_is_empty() {
    let world = emptied("budget-empty-edge");

    let edge = world.edge("work", "review");

    assert!(edge.status.success(), "the edge establishes the account:\n{}", said(&edge));
    assert!(
        fs::read_to_string(world.plan()).expect("read the plan").contains("**State:** review"),
        "the edge applied"
    );
    assert_established(&world, 2);
}

/// `rhei budget show` reports an empty journal as an account not yet
/// established, in text and in JSON, and writes nothing.
// §FS-rhei-budgets.10
#[test]
#[ignore = "red until #540: budget show calls an empty journal untrustworthy, 'missing \
            account initialization', with help naming rhei budget show; implement removes this"]
fn budget_show_reports_an_empty_journal_as_no_account_yet() {
    let world = emptied("budget-empty-show");
    let (journal, witness) = (world.journal(), world.witness());

    assert_reports_no_account(&world);

    assert_eq!(fs::metadata(&journal).expect("the journal is still there").len(), 0);
    assert!(!witness.exists(), "show wrote no witness");
}

/// The ticket's control, the journal deleted with its lock file left beside it,
/// is the same absent state and gets the same report.
// §FS-rhei-budgets.10
#[test]
#[ignore = "red until #540: budget show calls a bare lock file untrustworthy, 'missing \
            account initialization', with help naming rhei budget show; implement removes this"]
fn budget_show_reports_a_journal_deleted_beside_its_lock_as_no_account_yet() {
    let world = emptied("budget-empty-lock");
    let journal = world.journal();
    fs::remove_file(&journal).expect("delete the journal, keeping its lock");

    assert_reports_no_account(&world);

    assert!(!journal.exists(), "show created no journal");
}

/// A refusal `show` raises for an account it cannot read does not send the
/// operator back to `show`: here the journal is unreadable.
// §FS-rhei-budgets.10
// A journal no one may read is a Unix mode; Windows has no such permission bit to clear.
#[cfg(unix)]
#[test]
#[ignore = "red until #540: budget show's own refusal on an unreadable journal says 'inspect \
            the project's account with: rhei budget show'; implement removes this"]
fn budget_show_never_sends_the_operator_back_to_budget_show() {
    use std::os::unix::fs::PermissionsExt;

    let world = world("budget-show-unreadable");
    let journal = world.journal();
    let readable = fs::metadata(&journal).expect("journal metadata").permissions();
    fs::set_permissions(&journal, fs::Permissions::from_mode(0o000)).expect("seal the journal");
    if fs::read(&journal).is_ok() {
        fs::set_permissions(&journal, readable).expect("give the journal back");
        panic!(
            "{} is still readable at mode 0000, so this test cannot stage the refusal; \
             do not run the suite as root",
            journal.display()
        );
    }

    let text = world.budget(&["show", &world.project.display().to_string()]);
    let json = show_error(&world);
    fs::set_permissions(&journal, readable).expect("give the journal back");

    assert!(!text.status.success(), "show refuses an unreadable journal:\n{}", said(&text));
    assert!(!said(&text).contains("rhei budget show"), "text help:\n{}", said(&text));
    let help = json["help"].as_str().unwrap_or_default();
    assert!(!help.contains("rhei budget show"), "JSON help: {json}");
}

/// `show`'s report on an absent account, in both forms: the project has no
/// budget account yet, and `rhei budget init` establishes one. Nothing calls it
/// untrustworthy, and no help names the command just run.
fn assert_reports_no_account(world: &World) {
    let text = world.budget(&["show", &world.project.display().to_string()]);
    let said = said(&text);
    assert!(!text.status.success(), "show exits non-zero on an absent account:\n{said}");
    assert!(said.contains("has no budget account yet"), "text report:\n{said}");
    assert!(said.contains("rhei budget init"), "text help names budget init:\n{said}");
    assert!(!said.contains("missing account initialization"), "text report:\n{said}");
    assert!(!said.contains("rhei budget show"), "text help names show itself:\n{said}");

    let json = show_error(world);
    let message = json["message"].as_str().unwrap_or_default();
    let help = json["help"].as_str().unwrap_or_default();
    assert!(message.contains("has no budget account yet"), "JSON report: {json}");
    assert!(help.contains("rhei budget init"), "JSON help names budget init: {json}");
    assert!(!message.contains("missing account initialization"), "JSON report: {json}");
    assert!(!help.contains("rhei budget show"), "JSON help names show itself: {json}");
    assert!(json.get("damage").is_none(), "an absent account is not damaged: {json}");
}

/// The error object `rhei budget show --format json` emits on stderr for an
/// account it does not show as verified. §FS-rhei-budgets.10
fn show_error(world: &World) -> serde_json::Value {
    let mut cmd = world.rhei();
    cmd.arg("budget").arg("show").arg(&world.project).args(["--format", "json"]);
    let out = cmd.output().expect("rhei budget show should run");
    let raw = raw_stderr(&out);
    assert!(!out.status.success(), "show exits non-zero:\n{raw}");
    // The fixture's `mock` declares no family, so its warning precedes the
    // error on stderr. §FS-rhei-agents.1.1.2
    let line: String = raw
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.contains("declares no family"))
        .collect();
    let parsed: serde_json::Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("stderr is the JSON error object ({error}):\n{raw}"));
    parsed["error"].clone()
}
