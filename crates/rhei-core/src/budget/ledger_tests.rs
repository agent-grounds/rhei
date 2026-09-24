//! The ledger-state matrix: absent, damaged, adopted, and the two kinds of
//! chain that verify against nothing.
//!
//! These are answered differently on purpose, and the difference is the whole
//! argument for keeping a witness at all: without one, a deleted journal and a
//! fresh project are the same thing on disk.
//! §FS-rhei-budgets.5.4 §AR-neural-admission.4

use super::test_support::*;
use super::*;

/// Absent is lawful and silent. No command, no prompt, and no capacity minted:
/// a new account starts at zero consumed under the window contract.
/// §FS-rhei-budgets.5.4
#[test]
fn an_absent_account_is_established_by_the_first_admission() {
    let case = Case::new();

    assert!(case.journal_path().is_file(), "the first admission wrote the journal");
    assert!(case.witness_path().is_file(), "and the witness beside it");
    let journal = case.account.open(false).expect("open");
    assert_eq!(journal.contract().expect("contract"), Contract::Window);
    assert_eq!(journal.snapshot().expect("snapshot").lifetime_invocations, Counter::default());
}

/// A witness that knows of receipts this root no longer has is **damaged**:
/// new work is refused, and the message names both paths and says the journal
/// is restored by copying the witness back. Establishing a fresh account over
/// it would be exactly the minting this design exists to prevent.
/// §FS-rhei-budgets.5.4
#[test]
fn a_witness_without_a_journal_refuses_and_names_both_paths() {
    let case = Case::new();
    std::fs::remove_file(case.journal_path()).expect("lose the journal");

    let refused = case.account.open(false).expect_err("a lost journal is not a fresh project");

    assert_eq!(refused.reason_code, "untrustworthy_ledger");
    let journal = case.journal_path().display().to_string();
    let witness = case.witness_path().display().to_string();
    // Both spellings quoted, not just the one the product printed: when these
    // two disagree the difference is the whole finding. §REQ-cross-platform.5
    assert!(refused.message.contains(&journal), "names the journal {journal}: {}", refused.message);
    assert!(refused.message.contains(&witness), "names the witness {witness}: {}", refused.message);
    assert!(refused.message.contains("copying the witness back"), "{}", refused.message);
}

/// Deleting the whole account directory is the same answer, because the
/// witness index remembers this canonical root. Otherwise `rm -rf` on one
/// directory would be the way to buy capacity. §FS-rhei-budgets.5.3
#[test]
fn deleting_the_account_directory_does_not_recreate_capacity() {
    let case = Case::new();
    std::fs::remove_dir_all(case.root().join(ACCOUNT_DIR)).expect("lose the directory");

    let located = Account::locate(case.root()).expect("locate").expect("the root is remembered");

    assert_eq!(located.uuid(), case.account.uuid());
    assert!(Account::establish(case.root(), &audit()).is_err(), "and it refuses rather than mints");
}

/// A journal that verifies on its own terms but that this machine has never
/// witnessed is **adopted**: a project cloned from git, or the same project on
/// a new machine. The witness is written from the journal's own bytes and the
/// run proceeds, which mints nothing because the consumed counts travelled with
/// the journal. §FS-rhei-budgets.5.4
#[test]
fn a_valid_journal_without_a_local_witness_is_adopted_with_its_counts() {
    let case = Case::new();
    let ticket = case.ticket();
    {
        let mut journal = case.open();
        let arms = arm("attempt-1");
        let group = journal
            .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
            .expect("reserve");
        journal.record_start(&group.reservation_ids[0], true, &audit()).expect("start");
    }
    // The clone: the journal travelled, the witness did not.
    std::fs::remove_dir_all(case.witness_path().parent().expect("witness dir"))
        .expect("lose the witness");

    let journal = case.account.open(false).expect("a fresh clone is not refused");

    assert!(journal.adopted(), "the open says it adopted rather than verified");
    assert_eq!(
        journal.snapshot().expect("snapshot").lifetime_invocations.consumed,
        1,
        "the consumed count travelled with the journal"
    );
    assert!(case.witness_path().is_file(), "and the witness now exists for next time");
}

/// A witness that disagrees with a journal that is neither a prefix nor a copy
/// of it refuses. The witness catches an accidentally lost tail, not a
/// hand-edited ledger, and that is the whole of what it claims — but a ledger
/// that has been edited is exactly where the two disagree.
/// §FS-rhei-budgets.5.3
#[test]
fn a_witness_that_disagrees_with_the_journal_refuses() {
    let case = Case::new();
    let mut lines: Vec<String> = std::fs::read_to_string(case.journal_path())
        .expect("read journal")
        .lines()
        .map(str::to_owned)
        .collect();
    lines[0] = lines[0].replace("\"actor\":\"tester\"", "\"actor\":\"someone-else\"");
    std::fs::write(case.journal_path(), format!("{}\n", lines.join("\n"))).expect("edit");

    let refused = case.account.open(false).expect_err("an edited ledger is refused");

    assert_eq!(refused.reason_code, "untrustworthy_ledger");
}

/// A chain whose last line was cut short refuses on its own terms, before the
/// witness is consulted at all: a partial write can never yield capacity.
/// §AR-neural-admission.4
#[test]
fn a_truncated_chain_refuses() {
    let case = Case::new();
    let raw = std::fs::read_to_string(case.journal_path()).expect("read journal");
    std::fs::write(case.journal_path(), &raw[..raw.len() - 12]).expect("truncate");

    let refused = case.account.open(false).expect_err("a truncated chain is refused");

    assert_eq!(refused.reason_code, "untrustworthy_ledger");
}

/// And a chain whose hashes do not link refuses, which is what makes the
/// sequence a chain rather than a list. §FS-rhei-budgets.5.2
#[test]
fn a_chain_whose_hashes_do_not_link_refuses() {
    let case = Case::new();
    let ticket = case.ticket();
    {
        let mut journal = case.open();
        let arms = arm("attempt-1");
        journal
            .reserve(&request(&case, &ticket, &arms, false), bounds(80, 200), &audit())
            .expect("reserve");
    }
    let raw = std::fs::read_to_string(case.journal_path()).expect("read journal");
    let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
    lines.remove(1);
    std::fs::write(case.journal_path(), format!("{}\n", lines.join("\n"))).expect("unlink");

    let refused = case.account.open(false).expect_err("a broken link is refused");

    assert_eq!(refused.reason_code, "untrustworthy_ledger");
}

/// The guard of §FS-rhei-budgets.5.3 compares two resolved paths, so both have
/// to be resolved the same way: a witness *inside* the account is refused, and
/// one beside it is not. Resolving one of the two with a raw `canonicalize` and
/// the other plainly makes this comparison stop matching on Windows, which
/// silently admits the self-witnessing ledger the guard exists to refuse — so
/// the refusal is pinned rather than reasoned about.
/// §FS-rhei-budgets.5.3 §REQ-cross-platform.5
#[test]
fn a_witness_inside_the_account_is_refused_and_one_beside_it_is_not() {
    let case = Case::new();
    let uuid = case.account.uuid();

    let inside = case.root().join(ACCOUNT_DIR).join("witness");
    // `err()` rather than `expect_err`: a held lock is not `Debug`.
    let refused = super::authority::Authority::lock_at(case.root(), &inside, uuid)
        .err()
        .expect("a chain verifying against itself verifies nothing");

    assert!(
        refused.message.contains("cannot live inside the account it witnesses"),
        "{}",
        refused.message
    );

    let beside = case.root().join("witness");
    if let Err(error) = super::authority::Authority::lock_at(case.root(), &beside, uuid) {
        panic!("a witness beside the account is one an operator may keep: {}", error.message);
    }
}
