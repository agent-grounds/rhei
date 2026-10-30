//! A witness that ends mid-receipt: told apart from every sub-case that offers
//! the copy, counted by its whole receipts, and answered by the trim, never the
//! copy, whatever the journal holds.
//!
//! The rows are the proposal's table on agent-grounds/rhei#536. The trim itself
//! is stood in for by cutting the witness back to its last newline here, because
//! what these pin is the classification before and after it; its spelling is the
//! platform's. §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10

use super::diagnosis::{Diagnosis, Inspection};
use super::test_support::{audit, Case};
use super::{Account, Retirement, ACCOUNT_DIR};
use std::fs;
use std::io::Write;

/// The first 200 bytes of the witness's last receipt, with no newline: the
/// hand-made tear of the issue. §FS-rhei-budgets.5.4.3
fn fragment(case: &Case) -> Vec<u8> {
    let witness = fs::read(case.witness_path()).expect("read the witness");
    let last = witness[..witness.len() - 1]
        .rsplit(|byte| *byte == b'\n')
        .next()
        .expect("the witness holds a receipt");
    last[..200.min(last.len() - 1)].to_vec()
}

/// Append bytes to the witness with no newline after them.
fn tear(case: &Case, bytes: &[u8]) {
    let mut witness =
        fs::OpenOptions::new().append(true).open(case.witness_path()).expect("open the witness");
    witness.write_all(bytes).expect("append the fragment");
}

/// The case's account with its ticket bound, so it holds two whole receipts.
fn bound() -> Case {
    let case = Case::new();
    drop(case.open());
    case
}

fn whole_receipts(case: &Case) -> usize {
    fs::read(case.witness_path()).expect("read the witness").iter().filter(|b| **b == b'\n').count()
}

/// What the trim leaves: the witness cut back to the end of its last whole
/// receipt. §FS-rhei-budgets.5.4.3
fn trim_by_hand(case: &Case) {
    let witness = fs::read(case.witness_path()).expect("read the witness");
    let whole = witness.iter().rposition(|byte| *byte == b'\n').map_or(0, |at| at + 1);
    fs::OpenOptions::new()
        .write(true)
        .open(case.witness_path())
        .and_then(|file| file.set_len(whole as u64))
        .expect("cut the witness back");
}

fn diagnosis(case: &Case) -> Box<Diagnosis> {
    match case.account.inspect().expect("inspect the account") {
        Inspection::Damaged(diagnosis) => diagnosis,
        other => panic!("expected a damaged account; got {other:?}"),
    }
}

/// The copy of this case's witness over its journal, as the refusal would
/// spell it. §FS-rhei-budgets.5.4
fn copy_line(diagnosis: &Diagnosis) -> String {
    crate::platform::copy_command(&diagnosis.witness, &diagnosis.journal)
}

/// What a cut-short append leaves — the journal is the witness less the
/// fragment — is `witness_torn`, counted by its whole receipts, offered neither
/// the copy nor `forget`, and verifies once trimmed. §FS-rhei-budgets.5.4.3
#[test]
fn a_cut_short_append_is_witness_torn_and_verifies_once_trimmed() {
    let case = bound();
    let receipts = whole_receipts(&case);
    tear(&case, &fragment(&case));

    let torn = diagnosis(&case);
    assert_eq!(torn.damage.as_str(), "witness_torn");
    assert_eq!(torn.history.receipts, receipts, "a fragment is not a receipt");
    assert_eq!(torn.restore_command(), None, "the copy would carry the fragment into the journal");
    assert_eq!(torn.forget_command(), None, "the journal is present and this project's own");
    let refusal = case.account.open(true).expect_err("a charge is refused");
    assert!(!refusal.message.contains(&copy_line(&torn)), "{}", refusal.message);
    assert!(!refusal.message.contains("copying the witness back"), "{}", refusal.message);

    trim_by_hand(&case);
    assert!(
        matches!(case.account.inspect().expect("inspect"), Inspection::Verified(_)),
        "trimmed to its last whole receipt, the witness is the journal again"
    );
}

/// This project's whole receipts past the journal, then a fragment: the trim
/// keeps them, and the account is then `journal_truncated` with the copy, which
/// is safe once the fragment is gone. §FS-rhei-budgets.5.4.3
#[test]
fn whole_receipts_then_a_fragment_are_witness_torn_and_then_truncated() {
    let case = Case::new();
    let journal = fs::read(case.journal_path()).expect("read the journal");
    drop(case.open());
    fs::write(case.journal_path(), &journal).expect("lose the journal's tail");
    tear(&case, &fragment(&case));

    let torn = diagnosis(&case);
    assert_eq!(torn.damage.as_str(), "witness_torn");
    assert_eq!(torn.restore_command(), None);

    trim_by_hand(&case);
    let trimmed = diagnosis(&case);
    assert_eq!(trimmed.damage.as_str(), "journal_truncated", "the trim kept the whole receipts");
    assert!(trimmed.restore_command().is_some());
}

/// A journal that diverges, under a fragment: `witness_torn` first, and
/// `chain_broken` with the copy once trimmed. §FS-rhei-budgets.5.4.3
#[test]
fn a_diverging_journal_under_a_fragment_is_witness_torn_and_then_broken() {
    let case = Case::new();
    let text = fs::read_to_string(case.journal_path()).expect("read the journal");
    let mut receipt: serde_json::Value =
        serde_json::from_str(text.trim_end()).expect("one receipt");
    receipt["previous_hash"] = serde_json::json!("0".repeat(64));
    fs::write(case.journal_path(), format!("{receipt}\n")).expect("break the chain");
    tear(&case, &fragment(&case));

    assert_eq!(diagnosis(&case).damage.as_str(), "witness_torn");

    trim_by_hand(&case);
    let trimmed = diagnosis(&case);
    assert_eq!(trimmed.damage.as_str(), "chain_broken");
    assert!(trimmed.restore_command().is_some());
}

/// No journal, and a witness that ends in a fragment: `witness_torn` through
/// both the journal-file seam and the missing-directory seam, offered `forget`
/// beside the trim and never the copy, and `journal_absent` once trimmed.
/// §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10
#[test]
fn no_journal_and_a_fragment_are_witness_torn_with_forget_beside_the_trim() {
    let case = bound();
    tear(&case, &fragment(&case));
    fs::remove_file(case.journal_path()).expect("lose the journal");

    let torn = diagnosis(&case);
    assert_eq!(torn.damage.as_str(), "witness_torn", "the journal file is gone");
    assert_eq!(torn.restore_command(), None);
    assert!(torn.forget_command().is_some(), "retiring keeps every byte, fragment included");

    fs::remove_dir_all(case.root().join(ACCOUNT_DIR)).expect("lose the account directory");
    let torn = diagnosis(&case);
    assert_eq!(torn.damage.as_str(), "witness_torn", "the account directory is gone");
    assert_eq!(torn.restore_command(), None);
    assert!(torn.forget_command().is_some());

    trim_by_hand(&case);
    assert_eq!(diagnosis(&case).damage.as_str(), "journal_absent");
}

/// The old advice already followed: journal and witness end in the same
/// fragment. `witness_torn`, then `chain_broken` with a copy that works.
/// §FS-rhei-budgets.5.4.3
#[test]
fn the_old_advice_followed_is_witness_torn_and_then_restored_by_the_copy() {
    let case = bound();
    tear(&case, &fragment(&case));
    fs::copy(case.witness_path(), case.journal_path()).expect("follow the old advice");

    let torn = diagnosis(&case);
    assert_eq!(torn.damage.as_str(), "witness_torn");
    assert_eq!(torn.restore_command(), None, "the copy is the step that changed nothing");

    trim_by_hand(&case);
    let trimmed = diagnosis(&case);
    assert_eq!(trimmed.damage.as_str(), "chain_broken");
    assert!(trimmed.restore_command().is_some());
    fs::copy(case.witness_path(), case.journal_path()).expect("run the restore");
    assert!(matches!(case.account.inspect().expect("inspect"), Inspection::Verified(_)));
}

/// A complete JSON object with no newline is still an append that did not
/// finish. §FS-rhei-budgets.5.4.3
#[test]
fn a_fragment_that_parses_as_json_is_still_witness_torn() {
    let case = bound();
    tear(&case, br#"{"kind":"transition","payload":{}}"#);

    let torn = diagnosis(&case);
    assert_eq!(torn.damage.as_str(), "witness_torn");
    assert_eq!(torn.restore_command(), None);
}

/// Another root's receipts past the journal, then a fragment: still
/// `foreign_tail`, and the fragment is not counted among the foreign receipts.
/// §FS-rhei-budgets.5.4.2
#[test]
fn a_foreign_tail_that_ends_in_a_fragment_stays_foreign_without_counting_it() {
    let case = Case::new();
    let elsewhere = tempfile::tempdir().expect("another root");
    let identity = serde_json::json!({
        "kind": "identity",
        "payload": {"source_path": elsewhere.path().join("plan.rhei.md")},
    });
    tear(&case, format!("{identity}\n{{\"kind\":\"transi").as_bytes());

    let foreign = diagnosis(&case);
    assert_eq!(foreign.damage.as_str(), "foreign_tail", "its set-aside removes the fragment too");
    assert_eq!(foreign.foreign.as_ref().expect("the other root is named").receipts, 1);
}

/// A copy presenting a torn account is `held_elsewhere`, settled before any
/// journal is opened. Passes before the fix; it guards the precedence.
/// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.5.4.3
#[test]
fn a_copy_of_a_torn_account_stays_held_elsewhere() {
    let case = bound();
    tear(&case, &fragment(&case));
    let copy = tempfile::tempdir().expect("the copy's root");
    let target = copy.path().join(ACCOUNT_DIR).join(case.account.uuid());
    fs::create_dir_all(&target).expect("the copy's account directory");
    fs::copy(case.journal_path(), target.join("journal.jsonl")).expect("copy the journal");

    let located = Account::locate(copy.path()).expect("locate").expect("the copy presents it");
    match located.inspect().expect("inspect the copy") {
        Inspection::Damaged(diagnosis) => assert_eq!(diagnosis.damage.as_str(), "held_elsewhere"),
        other => panic!("expected held_elsewhere; got {other:?}"),
    }
}

/// `forget` with the journal present refuses, writes nothing, and answers
/// `witness_torn` rather than offering the copy. §FS-rhei-budgets.10
#[test]
fn forget_refuses_a_torn_witness_whose_journal_is_present() {
    let case = bound();
    tear(&case, &fragment(&case));
    let witness = fs::read(case.witness_path()).expect("read the witness");
    let journal = fs::read(case.journal_path()).expect("read the journal");

    let outcome = case.account.retire(&audit()).expect("retire returns an outcome");

    let Retirement::RestoreInstead(refused) = outcome else {
        panic!("forget refuses a present journal; got {outcome:?}");
    };
    assert_eq!(refused.damage.as_str(), "witness_torn");
    assert_eq!(refused.restore_command(), None);
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
    assert_eq!(fs::read(case.journal_path()).expect("read the journal"), journal);
}

/// `forget` with the journal wholly absent retires the root as it does for
/// `journal_absent`, keeping every byte of the witness, fragment included.
/// §FS-rhei-budgets.10
#[test]
fn forget_retires_a_torn_witness_whose_journal_is_absent() {
    let case = bound();
    let receipts = whole_receipts(&case);
    tear(&case, &fragment(&case));
    let witness = fs::read(case.witness_path()).expect("read the witness");
    fs::remove_file(case.journal_path()).expect("lose the journal");

    let outcome = case.account.retire(&audit()).expect("retire returns an outcome");

    let Retirement::Retired(retired) = outcome else {
        panic!("forget retires a root with no journal; got {outcome:?}");
    };
    assert_eq!(retired.damage.as_str(), "witness_torn");
    assert_eq!(retired.history.receipts, receipts, "a fragment is not a receipt");
    let kept = fs::read(retired.kept_at.join("history.jsonl")).expect("read the kept witness");
    assert_eq!(kept, witness, "the witness moved whole, fragment included");
}
