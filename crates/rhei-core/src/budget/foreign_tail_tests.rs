//! A witness whose tail was written from another root, before a copy could be
//! refused: told apart from a lost tail, and never offered the restore.
//! §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.2

use super::diagnosis::{foreign_tail, Damage, Inspection};
use super::test_support::{audit, Case};
use super::Retirement;
use std::fs;
use std::io::Write;
use std::path::Path;

/// One receipt line of the given kind, as much of it as the tail is read for.
fn line(kind: &str, source: Option<&Path>) -> String {
    let payload = match source {
        Some(source) => serde_json::json!({"source_path": source}),
        None => serde_json::json!({}),
    };
    format!("{}\n", serde_json::json!({"kind": kind, "payload": payload}))
}

/// Append a tail to the case's witness, the way a copy's charge did.
fn contaminate(case: &Case, tail: &str) {
    let mut witness =
        fs::OpenOptions::new().append(true).open(case.witness_path()).expect("open the witness");
    witness.write_all(tail.as_bytes()).expect("append the tail");
}

/// An `identity` receipt naming a source outside this root makes the tail
/// foreign, and names that root and the receipts past the journal.
#[test]
fn an_identity_receipt_from_another_root_makes_the_tail_foreign() {
    let root = Path::new("/w/original");
    let journal = line("initialize", None);
    let tail = format!(
        "{}{}",
        line("identity", Some(Path::new("/w/copy/plan.rhei.md"))),
        line("transition", None)
    );
    let witness = format!("{journal}{tail}");

    let foreign = foreign_tail(root, journal.as_bytes(), witness.as_bytes())
        .expect("the tail was written from /w/copy");

    assert_eq!(foreign.root, Path::new("/w/copy"));
    assert_eq!(foreign.receipts, 2);
}

/// A tail with no `identity` receipt, or one naming this root, is
/// indistinguishable from a lost one.
#[test]
fn a_tail_without_a_foreign_identity_is_not_foreign() {
    let root = Path::new("/w/original");
    let journal = line("initialize", None);
    let bare = format!("{journal}{}", line("transition", None));
    let own = format!("{journal}{}", line("identity", Some(&root.join("plan.rhei.md"))));

    assert_eq!(foreign_tail(root, journal.as_bytes(), bare.as_bytes()), None);
    assert_eq!(foreign_tail(root, journal.as_bytes(), own.as_bytes()), None);
}

/// The account reads `foreign_tail`, offers no restore, offers the set-aside,
/// and `forget` refuses it as a present journal.
#[test]
fn a_contaminated_witness_is_foreign_tail_and_never_offered_the_copy() {
    let case = Case::new();
    let elsewhere = tempfile::tempdir().expect("another root");
    contaminate(&case, &line("identity", Some(&elsewhere.path().join("plan.rhei.md"))));

    let Inspection::Damaged(diagnosis) = case.account.inspect().expect("inspect") else {
        panic!("a contaminated witness is damaged");
    };
    assert_eq!(diagnosis.damage, Damage::ForeignTail);
    assert_eq!(diagnosis.restore_command(), None, "the copy would import the tail");
    let set_aside = diagnosis.set_aside_command().expect("the set-aside is offered");
    assert!(
        !set_aside.contains(&crate::platform::copy_command(&diagnosis.witness, &diagnosis.journal))
    );
    let refusal = case.account.open(true).expect_err("a charge is refused");
    assert!(refusal.message.contains(&set_aside), "{}", refusal.message);
    assert!(
        !refusal
            .message
            .contains(&crate::platform::copy_command(&diagnosis.witness, &diagnosis.journal)),
        "{}",
        refusal.message
    );
    let outcome = case.account.retire(&audit()).expect("retire returns an outcome");
    let Retirement::RestoreInstead(refused) = outcome else {
        panic!("forget refuses a present journal; got {outcome:?}");
    };
    assert_eq!(refused.damage, Damage::ForeignTail);
    assert!(refused.foreign.is_some());
}

/// A tail with no identity receipt stays `journal_truncated`, with the restore.
#[test]
fn a_tail_without_an_identity_receipt_stays_truncated() {
    let case = Case::new();
    contaminate(&case, &line("transition", None));

    let Inspection::Damaged(diagnosis) = case.account.inspect().expect("inspect") else {
        panic!("a witness ahead of the journal is damaged");
    };
    assert_eq!(diagnosis.damage, Damage::JournalTruncated);
    assert!(diagnosis.restore_command().is_some());
}
