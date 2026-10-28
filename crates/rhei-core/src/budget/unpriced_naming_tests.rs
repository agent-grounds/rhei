//! An `unpriced` count names what it holds, and a receipt that never recorded
//! what it was says so rather than guessing.
//!
//! A `spend` receipt written before receipts carried a `pricing_context` is
//! still an unpriced receipt: it keeps its count and its charge, and every
//! surface reads it as the one explicit unknown group, never as a pair
//! reconstructed from today's targets or books.
//! §FS-rhei-budgets.8.1 §FS-rhei-budgets.5.2

use super::test_support::*;
use super::*;

const RESERVE: u64 = built_in::SPEND_RESERVE;

/// A receipt settled `unpriced` with no pricing context — the shape every
/// receipt this build writes today, and every receipt an older build wrote.
/// It is counted, charged the reserve, and named as the unknown group beneath
/// the `estimated:` line, aligned under that line's value column.
/// §FS-rhei-budgets.8.1 §FS-rhei-budgets.10
#[test]
#[ignore = "red until #515: an unpriced receipt without context reads as the unknown group; implement removes this"]
fn an_unpriced_receipt_without_a_pricing_context_reads_as_the_unknown_group() {
    let case = Case::new();
    let mut journal = case.open();
    let group = journal
        .reserve(
            &request(&case, &case.ticket(), &arm("attempt:old"), false),
            bounds(80, 200),
            &audit(),
        )
        .expect("reserve");
    let id = group.reservation_ids[0].clone();
    journal.record_start(&id, true, &audit()).expect("start");
    journal.settle_spend(&id, RESERVE, "USD", SpendBasis::Unpriced, &audit()).expect("settle");
    drop(journal);

    let journal = case.account.open(false).expect("open");
    let lines = journal.lines(None, 200, 80, built_in::SPEND_PER_DAY, &|_| false).expect("lines");
    let spend = lines.iter().find(|line| line.dimension == "project spend").expect("spend line");
    let marks = spend.marks_line().expect("an unpriced day has an estimated line");

    assert_eq!(
        marks,
        "  estimated:   1 unpriced (charged at $20.00 each)\n               \
         1 with no pricing context (settled before rhei recorded one)",
        "the count is kept and the receipt is named as the unknown group, \
         never as a pair rhei did not record"
    );
}
