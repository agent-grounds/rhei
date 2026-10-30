//! A witness that ends mid-receipt, already on disk.
//!
//! Today rhei reads it as a journal whose tail was lost and offers copying the
//! witness over the journal. That copy writes the fragment into the journal,
//! the account is refused again as `chain_broken`, and the same copy is offered,
//! now changing nothing (`agent-grounds/rhei#536`, the reproducer's parts B and
//! C). It is `witness_torn`, counted by its whole receipts, and answered by the
//! trim — keep every byte, cut the witness back to its last whole receipt —
//! never by the copy ([§FS-rhei-budgets.5.4.3](../../docs/functional-spec/rhei-budgets.spec.md)).
//! Each case ends as the reproducer's control did: the same edge goes through.

// §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10

use std::fs;

use super::budget_torn_witness_support::*;
use super::*;

/// The hand-made tear: refused naming the trim and never the copy, reported as
/// `witness_torn` with its whole receipts, and usable again once the offered
/// trim has run, every byte of the torn witness kept beside it.
// §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10
#[test]
fn a_torn_witness_is_named_and_trimmed_never_copied() {
    let world = world("budget-torn-hand");
    let whole = world.tear();
    let torn = read(&world.witness());

    let refused = world.edge("work", "review");

    assert!(!refused.status.success(), "a torn witness is refused:\n{}", said(&refused));
    let report = world.damaged();
    assert_eq!(report["damage"], "witness_torn", "the sub-case is named:\n{report:#}");
    assert_eq!(report["receipts"], receipts(&whole), "a fragment is not a receipt:\n{report:#}");
    assert!(report.get("restore").is_none(), "the copy is never offered:\n{report:#}");
    assert!(report.get("forget").is_none(), "the journal is this project's:\n{report:#}");
    let trim =
        report["trim"].as_str().unwrap_or_else(|| panic!("the trim is offered:\n{report:#}"));
    let refusal = said(&refused);
    assert!(refusal.contains(trim), "the refusal offers the trim {trim}; got:\n{refusal}");
    assert!(refusal.contains("200 bytes"), "the refusal says what follows; got:\n{refusal}");
    assert!(
        !refusal.contains("copying the witness back over it"),
        "the copy would carry the fragment into the journal; got:\n{refusal}"
    );

    world.shell(trim);

    let kept = world.kept();
    assert_eq!(kept.len(), 1, "the trim kept one copy beside the witness: {kept:?}");
    assert!(read(&kept[0]) == torn, "the kept copy holds every byte of the torn witness");
    assert!(read(&world.witness()) == whole, "the witness is cut back to its last whole receipt");
    assert_success(&world.edge("work", "review"));
}

/// The old advice already followed: journal and witness end in the same
/// fragment. `witness_torn` with the trim; then `chain_broken` with a copy that
/// now works; then the edge goes through.
// §FS-rhei-budgets.5.4.3
#[test]
fn the_old_advice_followed_is_trimmed_then_restored() {
    let world = world("budget-torn-advice");
    world.tear();
    fs::copy(world.witness(), world.journal()).expect("follow the old advice");
    assert!(!world.edge("work", "review").status.success(), "the copy repaired nothing");

    let report = world.damaged();
    assert_eq!(report["damage"], "witness_torn", "not a copy that changes nothing:\n{report:#}");
    assert!(report.get("restore").is_none(), "the copy is never offered:\n{report:#}");
    world.shell(report["trim"].as_str().expect("the trim is offered"));

    let trimmed = world.damaged();
    assert_eq!(trimmed["damage"], "chain_broken", "the journal still ends in it:\n{trimmed:#}");
    world.shell(trimmed["restore"].as_str().expect("the copy is safe now"));

    assert_success(&world.edge("work", "review"));
}

/// `budget forget` on a torn witness whose journal is wholly absent retires the
/// root as it does for `journal_absent`, keeping every byte, fragment included;
/// the report offers both the trim and `forget`.
// §FS-rhei-budgets.5.4.3 §FS-rhei-budgets.10
#[test]
fn forget_retires_a_torn_witness_whose_journal_is_absent() {
    let world = world("budget-torn-forget-absent");
    let uuid = world.uuid();
    world.tear();
    let torn = read(&world.witness());
    fs::remove_dir_all(world.project.join(".agent-grounds/rhei/budgets"))
        .expect("lose the account directory");

    let report = world.damaged();
    assert_eq!(report["damage"], "witness_torn", "the sub-case is named:\n{report:#}");
    assert!(report.get("restore").is_none(), "the copy is never offered:\n{report:#}");
    assert!(report["trim"].is_string(), "the trim is offered:\n{report:#}");
    assert!(report["forget"].is_string(), "and forget beside it:\n{report:#}");

    assert_success(&world.forget());

    let retired: Vec<_> = fs::read_dir(world.authority().join("retired"))
        .expect("the witness is kept under retired/")
        .map(|entry| entry.expect("retired entry").path())
        .collect();
    assert!(
        retired.len() == 1
            && retired[0].file_name().unwrap().to_string_lossy().starts_with(&format!("{uuid}-")),
        "the witness is kept as <uuid>-<stamp>: {retired:?}"
    );
    assert!(read(&retired[0].join("history.jsonl")) == torn, "every byte, fragment included");
    assert_success(&world.edge("work", "review"));
}

/// `budget forget` on a torn witness whose journal is present refuses, writes
/// nothing, and names the trim rather than the copy.
// §FS-rhei-budgets.10
#[test]
fn forget_refuses_a_torn_witness_whose_journal_is_present_naming_the_trim() {
    let world = world("budget-torn-forget-present");
    world.tear();
    let witness = read(&world.witness());
    let journal = read(&world.journal());
    let report = world.damaged();
    assert_eq!(report["damage"], "witness_torn", "the sub-case is named:\n{report:#}");
    let trim =
        report["trim"].as_str().unwrap_or_else(|| panic!("the trim is offered:\n{report:#}"));

    let refused = world.forget();

    assert!(!refused.status.success(), "forget refuses a present journal:\n{}", said(&refused));
    let refusal = said(&refused);
    assert!(refusal.contains(trim), "the refusal names the trim {trim}; got:\n{refusal}");
    assert!(!refusal.contains("restore this project's journal"), "{refusal}");
    assert!(read(&world.witness()) == witness, "the witness is untouched");
    assert!(read(&world.journal()) == journal, "the journal is untouched");
    assert!(!world.authority().join("retired").exists(), "nothing was retired");
}
