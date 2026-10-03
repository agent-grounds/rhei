//! A project copied with `cp -r` presents the original's account uuid from a
//! second root on the same machine.
//!
//! Today rhei takes that uuid as the copy's own: the copy's first charge is
//! appended to the original's witness, and the untouched original is then
//! refused as untrustworthy and offered a restore that would import the copy's
//! receipts (`agent-grounds/rhei#411`). A copy is detectable — the original
//! still holds the uuid — so it is refused at its first charge, naming both
//! roots, and a move, where the old root no longer holds it, keeps its account
//! ([§FS-rhei-budgets.5.4.1](../../docs/functional-spec/rhei-budgets.spec.md)).
//!
//! Two cases here pass before the fix and must keep passing after it: an
//! adopted journal and a copy under its own state directory. They are the guard
//! against a fix that refuses every root the index has not seen.

// §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10

use std::fs;

use super::budget_copied_project_support::*;
use super::*;

/// The issue's first half: the copy's first charge is refused before anything
/// is appended to the original's authority, and the refusal names both roots
/// and the command that gives the copy an account of its own.
// §FS-rhei-budgets.5.4.1
#[test]
fn a_copy_is_refused_at_its_first_charge_naming_both_roots() {
    let world = world("budget-copy-refused");
    let uuid = world.account_uuid("original");
    world.copy("copy");
    let witness = world.witness_bytes("home", &uuid);
    let plan = fs::read(world.root("copy").join("plan.rhei.md")).expect("read the copy's plan");

    let refused = world.edge("copy", "work", "review");

    assert!(
        !refused.status.success(),
        "a copy presenting the original's account is refused at its first charge:\n{}",
        said(&refused)
    );
    let refusal = said(&refused);
    assert!(
        refusal.contains(&world.canonical("copy"))
            && refusal.contains(&world.canonical("original")),
        "the refusal names this root and the root that holds the account; got:\n{refusal}"
    );
    assert!(
        refusal.contains("rhei budget forget"),
        "the refusal says how to give the copy an account of its own; got:\n{refusal}"
    );
    assert!(
        world.witness_bytes("home", &uuid) == witness,
        "nothing was appended to the original's witness"
    );
    assert_eq!(
        fs::read(world.root("copy").join("plan.rhei.md")).expect("read the copy's plan"),
        plan,
        "no edge applied: the copy's ticket stays where it was"
    );
}

/// The issue's second half: the original's next charge proceeds as if the copy
/// had never been made.
// §FS-rhei-budgets.5.4.1
#[test]
fn the_original_keeps_running_after_its_copy_tried_to_charge() {
    let world = world("budget-copy-original");
    world.copy("copy");
    let _ = world.edge("copy", "work", "review");

    let original = world.edge("original", "work", "review");

    assert_success(&original);
}

/// `rhei budget forget` in the copy retires the copy's claim, keeps its old
/// journal under `budgets/retired/`, and leaves the original's history alone;
/// the copy's next charge opens a fresh account.
// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10
#[test]
fn forget_in_the_copy_gives_it_a_fresh_account_and_keeps_its_old_claim() {
    let world = world("budget-copy-forget");
    let uuid = world.account_uuid("original");
    world.copy("copy");
    let witness = world.witness_bytes("home", &uuid);
    let _ = world.edge("copy", "work", "review");

    let forget = world.budget_in(
        "home",
        &["forget", &world.root("copy").display().to_string()],
        "a copy of original",
    );

    assert_success(&forget);
    let retired: Vec<String> =
        fs::read_dir(world.root("copy").join(".agent-grounds/rhei/budgets/retired"))
            .expect("the copy's claim is kept under budgets/retired/")
            .map(|entry| entry.expect("retired entry").file_name().to_string_lossy().into_owned())
            .collect();
    assert!(
        retired.len() == 1 && retired[0].starts_with(&format!("{uuid}-")),
        "the old claim is kept as <uuid>-<stamp>; got {retired:?}"
    );
    assert_success(&world.edge("copy", "work", "review"));
    assert_ne!(
        world.account_uuid("copy"),
        uuid,
        "the copy's next charge minted an account of its own"
    );
    assert!(
        world.witness_bytes("home", &uuid) == witness,
        "neither the refused charge nor the retirement wrote into the original's witness"
    );
    assert_success(&world.edge("original", "work", "review"));
}

/// `rhei budget show --format json` in the copy reports the sub-case and its
/// holder as members, and exits non-zero.
// §FS-rhei-budgets.10
#[test]
fn show_json_in_a_copy_reports_held_elsewhere_and_the_holder() {
    let world = world("budget-copy-show");
    world.copy("copy");

    let result = world.show_json("copy");

    assert!(!result.status.success(), "a copy's account is damaged, so show exits non-zero");
    let line = raw_stderr(&result);
    let parsed: serde_json::Value = serde_json::from_str(line.trim()).unwrap_or_else(|error| {
        panic!("stderr should be the JSON error object ({error}):\n{line}")
    });
    let error = &parsed["error"];
    assert_eq!(error["health"], "damaged", "health stays two-valued:\n{line}");
    assert_eq!(error["damage"], "held_elsewhere", "the sub-case is named:\n{line}");
    assert_eq!(
        error["held_by"],
        world.canonical("original"),
        "the holding root is a member:\n{line}"
    );
}

/// `mv` is not a copy: the old root no longer holds the uuid, so the project
/// keeps its account, says so once, and the index follows it.
// §FS-rhei-budgets.5.4.1
#[test]
fn a_moved_project_keeps_its_account_with_one_warning() {
    let world = world("budget-move");
    let uuid = world.account_uuid("original");
    let old_root = world.canonical("original");
    world.move_to("moved");

    let moved = world.edge("moved", "work", "review");

    assert_success(&moved);
    assert_eq!(world.account_uuid("moved"), uuid, "the account travelled with the project");
    let warning = format!(
        "warning: budget account panta:{uuid} moved to {}; it was held at {old_root}, which no longer holds it",
        world.canonical("moved")
    );
    assert_eq!(
        moved.stderr.matches(&warning).count(),
        1,
        "the move is reported once on stderr; got:\n{}",
        moved.stderr
    );
    let index = world.roots_index("home");
    assert_eq!(
        index.get(&world.canonical("moved")),
        Some(&uuid),
        "the index binds the new root: {index:?}"
    );
    assert!(!index.contains_key(&old_root), "the stale root is dropped: {index:?}");
}

/// An adopted journal — this machine has never seen the account — proceeds.
/// Passes before the fix; it guards against a check that refuses every root
/// the index does not name. §FS-rhei-budgets.5.4
// §FS-rhei-budgets.5.4
#[test]
fn an_adopted_journal_proceeds_under_a_fresh_state_directory() {
    let world = world("budget-adopted");

    assert_success(&world.edge_in("other", "original", "work", "review"));
}

/// Adoption records the root, so a copy made after it is caught on the
/// adopting machine too.
// §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.1
#[test]
fn adoption_records_the_root_so_a_later_copy_is_refused() {
    let world = world("budget-adopted-copy");
    let uuid = world.account_uuid("original");
    assert_success(&world.edge_in("other", "original", "work", "review"));
    assert_eq!(
        world.roots_index("other").get(&world.canonical("original")),
        Some(&uuid),
        "adoption recorded the adopting root in the index"
    );
    world.copy("copy");

    let refused = world.edge_in("other", "copy", "review", "work");

    assert!(
        !refused.status.success(),
        "the copy is refused on the adopting machine:\n{}",
        said(&refused)
    );
}

/// A copy under its own `XDG_STATE_HOME` sees no witness, is adopted and
/// proceeds, and the first machine's witness is untouched. Passes before the
/// fix; the workaround the issue names must stay a working one.
// §FS-rhei-budgets.5.4
#[test]
fn a_copy_under_its_own_state_directory_proceeds() {
    let world = world("budget-copy-separate-state");
    let uuid = world.account_uuid("original");
    world.copy("copy");
    let witness = world.witness_bytes("home", &uuid);

    assert_success(&world.edge_in("other", "copy", "work", "review"));
    assert!(
        world.witness_bytes("home", &uuid) == witness,
        "the first machine's witness is untouched"
    );
    assert_success(&world.edge("original", "work", "review"));
}
