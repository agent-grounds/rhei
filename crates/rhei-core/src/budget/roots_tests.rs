//! A second root presenting an account: the four readings of the witness index,
//! and what a copy may and may not do with the account it presents.
//!
//! The end-to-end proof is `tests/e2e/budget_copied_project_tests.rs`; what is
//! pinned here is the "holds" test that tells a copy from a move, including the
//! two moves a reader could mistake for copies.
//! §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4.1

use super::diagnosis::{Damage, Inspection};
use super::roots::{read, Reading};
use super::test_support::{audit, Case};
use super::{Account, Retirement, ACCOUNT_DIR};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const UUID: &str = "22222222-2222-4222-8222-222222222222";
const OTHER: &str = "33333333-3333-4333-8333-333333333333";

fn root() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("root");
    let resolved = crate::platform::canonical_path(dir.path()).expect("resolve");
    (dir, resolved)
}

fn hold(root: &Path, uuid: &str) {
    fs::create_dir_all(root.join(ACCOUNT_DIR).join(uuid)).expect("account directory");
}

fn bound(root: &Path, uuid: &str) -> BTreeMap<PathBuf, String> {
    BTreeMap::from([(root.to_path_buf(), uuid.to_string())])
}

/// Bound to this root: the account's home.
#[test]
fn a_root_the_index_binds_is_home() {
    let (_dir, this) = root();
    hold(&this, UUID);
    assert_eq!(read(&bound(&this, UUID), &this, UUID), Reading::Home);
}

/// Bound to no root: adopted, which proceeds and records.
#[test]
fn a_uuid_the_index_does_not_know_is_adopted() {
    let (_dir, this) = root();
    hold(&this, UUID);
    let (_other, elsewhere) = root();
    assert_eq!(read(&bound(&elsewhere, OTHER), &this, UUID), Reading::Adopted);
}

/// Bound to another root that still holds the uuid: a copy, naming the holder.
#[test]
fn a_root_whose_holder_still_holds_the_uuid_is_a_copy() {
    let (_held, holder) = root();
    hold(&holder, UUID);
    let (_dir, copy) = root();
    hold(&copy, UUID);
    assert_eq!(read(&bound(&holder, UUID), &copy, UUID), Reading::Copy { holder });
}

/// Bound only to a root that is gone: a move.
#[test]
fn a_bound_root_that_is_gone_is_a_move() {
    let (dir, old) = root();
    drop(dir);
    let (_moved, moved) = root();
    hold(&moved, UUID);
    assert_eq!(read(&bound(&old, UUID), &moved, UUID), Reading::Move { stale: vec![old] });
}

/// The bound root is still there but holds no account at all: a move, not a
/// copy, because nothing there holds the uuid.
#[test]
fn a_bound_root_without_budgets_is_a_move() {
    let (_old, old) = root();
    let (_moved, moved) = root();
    hold(&moved, UUID);
    assert_eq!(read(&bound(&old, UUID), &moved, UUID), Reading::Move { stale: vec![old] });
}

/// The bound root now holds a *different* uuid — a new project laid there
/// after the move: still a move for this uuid.
#[test]
fn a_bound_root_holding_another_uuid_is_a_move() {
    let (_old, old) = root();
    hold(&old, OTHER);
    let (_moved, moved) = root();
    hold(&moved, UUID);
    assert_eq!(read(&bound(&old, UUID), &moved, UUID), Reading::Move { stale: vec![old] });
}

/// `cp -r` of the account directory into a second root on the same machine.
fn copy_of(case: &Case) -> (tempfile::TempDir, PathBuf) {
    let (dir, copy) = root();
    let target = copy.join(ACCOUNT_DIR).join(case.account.uuid());
    fs::create_dir_all(&target).expect("copy's account directory");
    fs::copy(case.journal_path(), target.join("journal.jsonl")).expect("copy the journal");
    (dir, copy)
}

/// A copy resolves carrying its holder; every charge on it is refused before
/// anything reaches the holder's witness, and `show` reads `held_elsewhere`.
/// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10
#[test]
fn a_copy_is_refused_and_reported_without_touching_the_witness() {
    let case = Case::new();
    let witness = fs::read(case.witness_path()).expect("read the witness");
    let (_dir, copy) = copy_of(&case);
    let holder = crate::platform::canonical_path(case.root()).expect("resolve");

    let located = Account::locate(&copy).expect("locate").expect("the copy presents an account");
    assert_eq!(located.held_by(), Some(holder.as_path()));
    let refusal = Account::establish(&copy, &audit()).expect_err("a copy's charge is refused");
    assert!(
        refusal.message.contains(&copy.display().to_string())
            && refusal.message.contains(&holder.display().to_string())
            && refusal.message.contains("rhei budget forget"),
        "the refusal names both roots and forget: {}",
        refusal.message
    );
    assert!(located.open(true).is_err(), "nor is it opened for a charge");
    match located.inspect().expect("inspect the copy") {
        Inspection::Damaged(diagnosis) => {
            assert_eq!(diagnosis.damage, Damage::HeldElsewhere);
            assert_eq!(diagnosis.held_by.as_deref(), Some(holder.as_path()));
            assert_eq!(diagnosis.restore_command(), None, "the holder's witness is not restored");
            assert!(diagnosis.forget_command().is_some());
        }
        other => panic!("expected held_elsewhere; got {other:?}"),
    }
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
}

/// `forget` in a copy retires the claim under `budgets/retired/` and touches
/// neither the witness nor the index; the next charge mints a fresh account.
/// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10
#[test]
fn retiring_a_copy_keeps_its_claim_and_leaves_the_holder_alone() {
    let case = Case::new();
    let witness = fs::read(case.witness_path()).expect("read the witness");
    let (_dir, copy) = copy_of(&case);
    let located = Account::locate(&copy).expect("locate").expect("the copy presents an account");

    let Retirement::Retired(retired) = located.retire(&audit()).expect("retire the claim") else {
        panic!("a copy's claim is retired");
    };

    assert_eq!(retired.damage, Damage::HeldElsewhere);
    assert!(retired.kept_at.starts_with(copy.join(ACCOUNT_DIR).join("retired")));
    assert!(retired.kept_at.join("journal.jsonl").exists(), "every byte is kept");
    assert!(retired.kept_at.join("retirement.json").exists(), "with the audit receipt");
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
    assert!(Account::locate(&copy).expect("locate").is_none(), "the copy has no account now");
    let (fresh, _) = Account::establish(&copy, &audit()).expect("a fresh account");
    assert_ne!(fresh.uuid(), case.account.uuid());
    assert!(Account::locate(case.root()).expect("locate").expect("holder").held_by().is_none());
}
