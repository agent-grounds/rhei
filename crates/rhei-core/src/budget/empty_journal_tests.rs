//! A journal that is there but holds no receipt, with no witness history for
//! its uuid: the lawful **absent** state, which the first charge establishes by
//! taking that file as the account's own (`agent-grounds/rhei#540`).
//!
//! Before the fix the first receipt opened the journal as a new file, so every
//! charge on this state failed with `File exists`, and an inspection read it as
//! a verified account with no initialization.
//! §FS-rhei-budgets.5.4 §AR-neural-admission.4

use super::authority::write_whole;
use super::diagnosis::Inspection;
use super::failed_append_tests::{cut_short, LINE};
use super::test_support::{audit, Case};
use super::Account;
use std::fs::{self, File};

/// An established account whose journal was then emptied from outside rhei,
/// and whose witness is gone: the state the ticket reports.
fn emptied() -> Case {
    let case = Case::new();
    File::create(case.journal_path()).expect("empty the journal");
    fs::remove_file(case.witness_path()).expect("delete the witness");
    case
}

/// The first charge takes the empty journal as the account's own, under the
/// uuid its directory already names, and the witness holds the same receipt.
/// §FS-rhei-budgets.5.4
#[test]
fn the_first_charge_takes_an_empty_journal_as_its_own() {
    let case = emptied();

    let (account, journal) = Account::establish(case.root(), &audit())
        .expect("the first charge establishes the absent account");

    assert_eq!(account.uuid(), case.account.uuid(), "the uuid its directory names");
    assert_eq!(journal.receipts().len(), 1, "the initialize receipt and nothing else");
    drop(journal);
    let ledger = fs::read(case.journal_path()).expect("read the journal");
    assert!(!ledger.is_empty(), "the receipt reached the journal it took over");
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), ledger);
}

/// The first receipt's journal write is cut short. The journal it took over is
/// cut back to zero bytes and left in place, not removed, and the witness is
/// absent again: both files are as they were before the edge.
/// §AR-neural-admission.4
#[test]
fn a_failed_first_append_leaves_the_empty_journal_empty_and_in_place() {
    let case = emptied();
    let mut journal = case.account.open(true).expect("open the absent account");
    assert!(journal.receipts().is_empty(), "the open replayed no receipt");

    let error = journal.write_receipt(LINE, cut_short).expect_err("the journal's write fails");

    assert!(
        error.message.contains("no space left"),
        "the write's own error, so the line reached the journal: {}",
        error.message
    );
    let left = fs::metadata(case.journal_path()).expect("the journal is still there");
    assert_eq!(left.len(), 0, "the journal is cut back to zero bytes");
    assert!(!case.witness_path().exists(), "the witness is absent again");
    assert!(journal.authority.bytes().is_empty(), "the in-memory copy follows the file");
}

/// Bytes that reached the journal after the open replayed it empty are not the
/// account's, so the first receipt is refused rather than written behind them,
/// and the witness is cut back. §AR-neural-admission.4
///
/// This passes before the fix, because then every first receipt into an
/// existing journal is refused. It guards the fix: taking an empty journal
/// over must not become appending to whatever is there.
#[test]
fn a_first_receipt_is_not_written_behind_bytes_the_open_did_not_replay() {
    let case = emptied();
    let mut journal = case.account.open(true).expect("open the absent account");
    fs::write(case.journal_path(), b"not replayed\n").expect("write behind the open");

    journal.write_receipt(LINE, write_whole).expect_err("the first receipt is refused");

    assert_eq!(fs::read(case.journal_path()).expect("read the journal"), b"not replayed\n");
    assert!(!case.witness_path().exists(), "the witness is absent again");
}

/// An inspection reads both absent spellings as absent, as the charge does:
/// an empty journal, and the account directory holding only its lock file.
/// It writes nothing. §FS-rhei-budgets.5.4 §FS-rhei-budgets.10
#[test]
fn an_empty_journal_and_a_bare_lock_inspect_as_absent() {
    let case = emptied();

    let inspected = case.account.inspect().expect("inspect the empty journal");
    assert!(matches!(inspected, Inspection::Absent), "an empty journal is absent: {inspected:?}");
    drop(inspected);
    assert_eq!(fs::metadata(case.journal_path()).expect("the journal").len(), 0);
    assert!(!case.witness_path().exists(), "the inspection wrote no witness");

    fs::remove_file(case.journal_path()).expect("delete the journal, keeping its lock");
    let inspected = case.account.inspect().expect("inspect the bare lock");
    assert!(matches!(inspected, Inspection::Absent), "a bare lock is absent: {inspected:?}");
}
