//! A project laid down at a path another project used before it.
//!
//! The end-to-end proof is `tests/e2e/budget_path_reuse_tests.rs`; what is
//! pinned here is the part of it that is arithmetic rather than wording — that
//! the three damaged sub-cases are told apart at all, and that a retirement
//! gives up an identity without giving up a receipt or a sound balance.
//! §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4

use super::diagnosis::{Damage, Inspection};
use super::test_support::{audit, Case};
use super::{Account, Retirement};
use std::fs;

/// What the shared witness home currently holds under `retired/`.
fn retired_names(case: &Case) -> Vec<String> {
    let retired =
        case.witness_path().parent().expect("the witness has a parent").with_file_name("retired");
    let Ok(entries) = fs::read_dir(retired) else { return Vec::new() };
    entries
        .map(|entry| entry.expect("retired entry").file_name().to_string_lossy().into())
        .collect()
}

fn damage(case: &Case) -> Damage {
    match case.account.inspect().expect("inspect the account") {
        Inspection::Damaged(diagnosis) => diagnosis.damage,
        other => panic!("expected a damaged account; got {other:?}"),
    }
}

/// The seam the whole ticket turns on: a journal that is **not there** is not
/// the same accident as one whose tail was lost, because only the first is
/// indistinguishable from a path a different project held before this one.
/// §FS-rhei-budgets.5.4
#[test]
fn an_absent_journal_is_classified_apart_from_a_truncated_one() {
    let case = Case::new();
    assert!(
        matches!(case.account.inspect().expect("inspect"), Inspection::Verified(_)),
        "the established account verifies before anything is done to it"
    );

    fs::write(case.journal_path(), "").expect("truncate the journal");
    assert_eq!(damage(&case), Damage::JournalTruncated);

    fs::remove_file(case.journal_path()).expect("remove the journal");
    assert_eq!(
        damage(&case),
        Damage::JournalAbsent,
        "no file at all is its own sub-case, because the history may not be this project's"
    );
}

/// A journal whose chain does not verify keeps the sharpest reading, and must
/// not be softened into the absent case by the classification above.
/// §FS-rhei-budgets.5.4
#[test]
fn a_broken_chain_is_classified_as_broken_rather_than_missing() {
    let case = Case::new();
    let text = fs::read_to_string(case.journal_path()).expect("read the journal");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let last = lines.pop().expect("the journal holds a receipt");
    let mut receipt: serde_json::Value = serde_json::from_str(&last).expect("receipt is JSON");
    receipt["previous_hash"] = serde_json::json!("0".repeat(64));
    lines.push(receipt.to_string());
    fs::write(case.journal_path(), format!("{}\n", lines.join("\n"))).expect("write it back");

    assert_eq!(damage(&case), Damage::ChainBroken);
}

/// The guarantee of §FS-rhei-budgets.5.3, at the layer that owns it. A command
/// could forget to ask; the account cannot.
#[test]
fn retiring_a_sound_account_is_refused_and_writes_nothing() {
    let case = Case::new();
    let witness = fs::read(case.witness_path()).expect("read the witness");

    let outcome = case.account.retire(&audit()).expect("retire returns an outcome");

    assert!(
        matches!(outcome, Retirement::Sound),
        "an account whose journal verifies is refused; got {outcome:?}"
    );
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
    // By uuid rather than by the presence of `retired/`: one witness home
    // serves the whole process, so a sibling case's retirement has already
    // made that directory.
    assert!(
        !retired_names(&case).iter().any(|name| name.starts_with(case.account.uuid())),
        "a refused retirement retired nothing; got {:?}",
        retired_names(&case)
    );
    assert_eq!(
        Account::locate(case.root()).expect("locate").map(|a| a.uuid().to_string()),
        Some(case.account.uuid().to_string()),
        "the path still resolves to the account it always did"
    );
}

/// The other refusal, at the same layer and for the same reason: a journal that
/// is **there** is this project's, so the remedy is the restore and a retirement
/// would discard a real account while leaving the journal unverifiable.
/// §FS-rhei-budgets.5.4 §FS-rhei-budgets.10
#[test]
fn retiring_an_account_whose_journal_is_present_is_refused_and_writes_nothing() {
    let case = Case::new();
    fs::write(case.journal_path(), "").expect("truncate the journal");
    let witness = fs::read(case.witness_path()).expect("read the witness");

    let outcome = case.account.retire(&audit()).expect("retire returns an outcome");

    let Retirement::RestoreInstead(diagnosis) = outcome else {
        panic!("a journal that is present is refused; got {outcome:?}")
    };
    assert_eq!(diagnosis.damage, Damage::JournalTruncated);
    // The platform's own copy, not `cp`: this sub-case offers the bare copy, and
    // matching a POSIX word here was asserting Unix. §FS-rhei-budgets.10
    assert_eq!(
        diagnosis.restore_command(),
        crate::platform::copy_command(&diagnosis.witness, &diagnosis.journal),
        "the refusal carries the remedy the sub-case has"
    );
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
    assert!(
        !retired_names(&case).iter().any(|name| name.starts_with(case.account.uuid())),
        "a refused retirement retired nothing; got {:?}",
        retired_names(&case)
    );
    assert_eq!(
        Account::locate(case.root()).expect("locate").map(|a| a.uuid().to_string()),
        Some(case.account.uuid().to_string()),
        "the path still resolves to the account it always did"
    );
}

/// Retirement keeps every receipt, records who gave the root up, and retracts
/// the identity so the next admission mints a fresh one.
/// §FS-rhei-budgets.5.3 §FS-rhei-budgets.10
#[test]
fn retiring_a_damaged_account_keeps_the_history_and_drops_the_identity() {
    let case = Case::new();
    let uuid = case.account.uuid().to_string();
    let witness = fs::read(case.witness_path()).expect("read the witness");
    fs::remove_dir_all(case.account.directory()).expect("clear the account directory");

    let outcome = case.account.retire(&audit()).expect("retire the damaged root");

    let Retirement::Retired(retired) = outcome else { panic!("expected a retirement") };
    assert_eq!(retired.damage, Damage::JournalAbsent);
    assert_eq!(
        fs::read(retired.kept_at.join("history.jsonl")).expect("the kept history is readable"),
        witness,
        "the receipts are kept byte for byte: a retirement is not a deletion"
    );
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(retired.kept_at.join("retirement.json")).expect("receipt"),
    )
    .expect("the receipt is JSON");
    assert_eq!(receipt["actor"], "tester");
    assert_eq!(receipt["reason"], "unit test");
    assert_eq!(receipt["uuid"], uuid);
    assert!(receipt["argv"].is_array(), "the audit shape `adjust` carries; got {receipt:#}");
    assert!(!case.witness_path().exists(), "nothing live claims the retired uuid");

    assert!(
        Account::locate(case.root()).expect("locate").is_none(),
        "the retracted index leaves the path resolving to no account"
    );
    let (fresh, _) = Account::establish(case.root(), &audit()).expect("establish a new account");
    assert_ne!(fresh.uuid(), uuid, "the next admission mints an identity rather than reviving one");
}

/// A root no witness claims has nothing to give up, and says so rather than
/// reporting a retirement that did not happen. §FS-rhei-budgets.10
#[test]
fn retiring_a_root_with_no_witness_retires_nothing() {
    let case = Case::new();
    fs::remove_dir_all(case.account.directory()).expect("clear the account directory");
    fs::remove_file(case.witness_path()).expect("clear the witness");

    let outcome = case.account.retire(&audit()).expect("retire returns an outcome");

    assert!(matches!(outcome, Retirement::NothingToRetire), "got {outcome:?}");
}
