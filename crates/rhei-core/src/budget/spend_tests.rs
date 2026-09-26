//! The one dimension whose unit is not a count.
//!
//! Three things are load-bearing here and none of them is true of the two
//! counts. A spend settle **revises** — it is the only write in this ledger
//! allowed to lower a number — and the case that matters most is that it
//! lowers *only* that number: the invariant of §REQ-bounded-neural-work.4 is
//! scoped to the counts in writing, and scoped is not suspended. An amount
//! nobody could price or measure is charged the worst case rather than
//! nothing, because a dimension that read zero for every transport it cannot
//! price would be silently absent for exactly the transports no other bound
//! watches. And a build that meets a receipt kind it does not know **reads**
//! the account rather than refusing it, which is what a downgrade across this
//! version was promised.
//! §FS-rhei-budgets.6.2 §FS-rhei-budgets.3.4 §FS-rhei-budgets.5.2

use super::test_support::*;
use super::*;

const RESERVE: u64 = built_in::SPEND_RESERVE;
const MEASURED: u64 = 3 * crate::money::MICRO;

/// One admitted, started arm, with the reservation id the settle names.
fn started(case: &Case, attempt: &str) -> String {
    let mut journal = case.open();
    let arms = arm(attempt);
    let group = journal
        .reserve(&request(case, &case.ticket(), &arms, false), bounds(80, 200), &audit())
        .expect("reserve");
    let id = group.reservation_ids[0].clone();
    journal.record_start(&id, true, &audit()).expect("start");
    id
}

fn spend_of(case: &Case) -> Counter {
    case.account.open(false).expect("open").snapshot().expect("snapshot").spend
}

fn marks_of(case: &Case) -> SpendMarks {
    case.account.open(false).expect("open").snapshot().expect("snapshot").spend_marks
}

/// The label and the settings key are what every surface names this dimension
/// by, and they are not interchangeable: one is prose and one is a file.
/// §FS-rhei-budgets.2.1 §FS-rhei-budgets.8
#[test]
fn the_spend_dimension_names_itself_the_same_way_on_every_surface() {
    assert_eq!(Dimension::Spend.label(), "project spend");
    assert_eq!(Dimension::Spend.settings_key(), "spend_per_day");
    assert!(Dimension::Spend.is_money());
    assert!(!Dimension::Invocations.is_money());
}

/// The counter is reused unchanged over micro-units, and it may not
/// manufacture capacity by wrapping. An amount that overflows is an error,
/// never a small number. §FS-rhei-budgets.1
#[test]
fn an_amount_that_overflows_is_an_error_rather_than_a_wrap() {
    let counter = Counter { consumed: u64::MAX, reserved: 1 };

    let overflowed = counter.exposure().expect_err("u64::MAX + 1 has no answer");

    assert_eq!(overflowed.reason_code, "untrustworthy_ledger");
    assert_eq!(Counter { consumed: 3, reserved: 2 }.remaining(4).expect("remaining"), 0);
}

/// A fractional bound is the *point* of this key, which is what distinguishes
/// it from the three counts. §FS-rhei-budgets.2.1
#[test]
fn a_money_bound_resolves_and_writes_itself_as_the_number_its_key_takes() {
    let built = Bound::resolve_money("spend_per_day", built_in::SPEND_PER_DAY, None, None);
    assert_eq!(built.report_line(), "spend_per_day: 400.00 (built_in)");

    let clamped = Bound::resolve_money(
        "spend_per_day",
        built_in::SPEND_PER_DAY,
        Some(25 * crate::money::MICRO),
        Some((100 * crate::money::MICRO, BoundSource::Project)),
    );
    assert_eq!(clamped.effective, 25 * crate::money::MICRO);
    assert_eq!(
        clamped.report_line(),
        "spend_per_day: 25.00 (requested 100.00 by the project, limited by machine settings)"
    );
    // The same bound as a halt writes it, which is the other of the two
    // renderings and the reason the unit is carried at all.
    assert_eq!(
        clamped.amount_phrase(Some("USD")),
        "$25.00 (requested $100.00 by the project, limited by machine settings)"
    );
    assert_eq!(
        Bound::resolve_money("spend_per_day", 25 * crate::money::MICRO, None, None)
            .amount_phrase(Some("CHF")),
        "25.00 CHF (built_in)"
    );
    // A count bound is untouched by any of it.
    assert_eq!(
        Bound::resolve("transition_limit", built_in::TRANSITION_LIMIT, None, None).report_line(),
        "transition_limit: 80 (built_in)"
    );
}

/// The six rows, in the order §FS-rhei-budgets.8 fixes, with the seventh
/// present only where some part of the day was estimated. A count halt is
/// unchanged, which is the whole point of checking spend last.
/// §FS-rhei-budgets.8
#[test]
fn a_spend_halt_says_six_things_and_a_seventh_only_where_it_has_to() {
    let bound = Bound::resolve_money(
        "spend_per_day",
        built_in::SPEND_PER_DAY,
        Some(25 * crate::money::MICRO),
        None,
    );
    let spent = Exhaustion {
        dimension: Dimension::Spend,
        subject: "panta".into(),
        counter: Counter { consumed: 6 * crate::money::MICRO, reserved: RESERVE },
        bound: 25 * crate::money::MICRO,
        contract: Contract::Window,
        day: "2026-09-26".into(),
        currency: Some("USD".into()),
    };

    let measured = halt_text(
        &spent,
        &bound,
        &Remedy::Renews("2026-09-27T00:00:00Z".into()),
        &SpendMarks::default(),
    );

    assert_eq!(
        measured,
        concat!(
            "error: project 'panta' has spent today's measured budget\n",
            "       dimension:   project spend\n",
            "       bound:       $25.00 (machine)\n",
            "       consumed:    $6.00  outstanding: $20.00  remaining: $0.00\n",
            "       mode:        window (2026-09-26Z)\n",
            "       renews at:   2026-09-27T00:00:00Z",
        )
    );

    let estimated = halt_text(
        &spent,
        &bound,
        &Remedy::Renews("2026-09-27T00:00:00Z".into()),
        &SpendMarks { unpriced: 2, unmeasurable: 0, unsettled: 1 },
    );

    assert!(
        estimated.contains(
            "\n       estimated:   2 unpriced, 1 unsettled (charged at $20.00 each)\
             \n       mode:"
        ),
        "the optional row sits between `consumed:` and `mode:`, and a mark with \
         no members is left out rather than printed as a zero; got:\n{estimated}"
    );
}

/// A settle revises the reserve to what the request cost, in either
/// direction. The day carries the measured amount; the reserve bounded only
/// the exposure while the request was in flight. §FS-rhei-budgets.6.2
#[test]
fn a_spend_settle_revises_the_reserve_down_and_up() {
    let case = Case::new();
    let down = started(&case, "attempt:down");
    assert_eq!(spend_of(&case), Counter { consumed: 0, reserved: RESERVE });

    case.open()
        .settle_spend(&down, MEASURED, "USD", SpendBasis::Measured, &audit())
        .expect("settle down");

    assert_eq!(spend_of(&case), Counter { consumed: MEASURED, reserved: 0 });

    let up = started(&case, "attempt:up");
    let over = 40 * crate::money::MICRO;
    case.open().settle_spend(&up, over, "USD", SpendBasis::Measured, &audit()).expect("settle up");

    assert_eq!(
        spend_of(&case),
        Counter { consumed: MEASURED + over, reserved: 0 },
        "an actual above the reserve is the residual §REQ-bounded-neural-work.6 \
         already states, not an error"
    );
    assert_eq!(marks_of(&case), SpendMarks::default(), "both were measured");
}

/// The invariant the amended §REQ-bounded-neural-work.4 is only as strong as.
///
/// A settle is the first write allowed to lower a number, and this is the case
/// that says it lowers exactly one: the two counts after it are what two
/// admitted starts owe, not one less.
/// §REQ-bounded-neural-work.4 §FS-rhei-budgets.6.2
#[test]
fn a_spend_settle_changes_neither_count() {
    let case = Case::new();
    let first = started(&case, "attempt:one");
    started(&case, "attempt:two");
    let before = case.account.open(false).expect("open").snapshot().expect("snapshot");

    case.open()
        .settle_spend(&first, MEASURED, "USD", SpendBasis::Measured, &audit())
        .expect("settle");

    let after = case.account.open(false).expect("open").snapshot().expect("snapshot");
    assert_eq!(after.invocations, before.invocations, "no invocation unit was released");
    assert_eq!(after.lifetime_invocations, before.lifetime_invocations);
    assert_eq!(after.travel, before.travel, "and no travel unit");
}

/// One settle per reservation. Replaying the same bytes is idempotent;
/// different bytes for the same reservation, or a reservation the chain does
/// not hold, is corruption. §FS-rhei-budgets.5.2
#[test]
fn a_second_settle_for_one_reservation_is_corruption_and_so_is_an_absent_one() {
    let case = Case::new();
    let id = started(&case, "attempt:once");
    case.open().settle_spend(&id, MEASURED, "USD", SpendBasis::Measured, &audit()).expect("settle");

    let again = case
        .open()
        .settle_spend(&id, 7 * crate::money::MICRO, "USD", SpendBasis::Measured, &audit())
        .expect_err("a second settle with different content is corruption");
    assert_eq!(again.reason_code, "untrustworthy_ledger");

    let absent = case
        .open()
        .settle_spend(
            "reservation:11111111-1111-4111-8111-111111111111",
            MEASURED,
            "USD",
            SpendBasis::Measured,
            &audit(),
        )
        .expect_err("a settle naming no reservation is corruption");
    assert_eq!(absent.reason_code, "untrustworthy_ledger");
    assert_eq!(spend_of(&case).consumed, MEASURED, "and neither one moved the day");
}

/// Never nothing. An invocation whose record could not be priced, and one
/// that produced no record at all, are both charged the worst case and are
/// reported apart — the two send a reader somewhere different.
/// §FS-rhei-budgets.6.2
#[test]
fn an_unpriced_or_unmeasurable_invocation_is_charged_the_reserve() {
    let case = Case::new();
    let unpriced = started(&case, "attempt:unpriced");
    let unmeasurable = started(&case, "attempt:unmeasurable");

    let mut journal = case.open();
    journal.settle_spend(&unpriced, RESERVE, "USD", SpendBasis::Unpriced, &audit()).expect("a");
    journal
        .settle_spend(&unmeasurable, RESERVE, "USD", SpendBasis::Unmeasurable, &audit())
        .expect("b");
    drop(journal);

    assert_eq!(spend_of(&case), Counter { consumed: 2 * RESERVE, reserved: 0 });
    assert_eq!(marks_of(&case), SpendMarks { unpriced: 1, unmeasurable: 1, unsettled: 0 });
}

/// `unsettled` is derived rather than stored, and it needs the same "is the
/// owning run gone" proof the release step performs: a started reserve whose
/// run is still live is in flight, not unsettled. §FS-rhei-budgets.6.2
#[test]
fn a_started_reserve_whose_run_is_gone_derives_the_unsettled_mark() {
    let case = Case::new();
    started(&case, "attempt:dropped");
    let snapshot = case.account.open(false).expect("open").snapshot().expect("snapshot");

    assert_eq!(
        snapshot.marks(&|_| false),
        SpendMarks::default(),
        "a run that is still live is holding a reserve, not losing one"
    );
    assert_eq!(snapshot.marks(&|_| true), SpendMarks { unsettled: 1, ..SpendMarks::default() });
    assert_eq!(
        snapshot.spend,
        Counter { consumed: 0, reserved: RESERVE },
        "and either way the amount stays outstanding, so a wrong day reads high"
    );
}

/// The spend window is the UTC day, always, and it is a key rather than a
/// refill: yesterday's amounts stay stamped with yesterday.
/// §FS-rhei-budgets.3.4
#[test]
fn the_day_is_the_spend_window_whichever_contract_the_account_holds() {
    let case = Case::at("2026-09-26T12:00:00Z");
    let id = started(&case, "attempt:yesterday");
    case.open().settle_spend(&id, MEASURED, "USD", SpendBasis::Measured, &audit()).expect("settle");
    case.open().adjust(500, &audit()).expect("move to a lifetime allowance");

    case.set_clock("2026-09-27T09:00:00Z");
    let tomorrow = case.account.open(false).expect("open").snapshot().expect("snapshot");

    assert_eq!(tomorrow.spend, Counter::default(), "the new day has its own key");
    assert!(tomorrow.contract.is_lifetime(), "and the contract did not change that");
}

/// The third check, and the order it is in.
///
/// A spawn standing at the invocation bound and the spend bound at once is
/// refused on the count, with the text that already ships, because that check
/// comes first. §FS-rhei-budgets.6.1
#[test]
fn spend_is_checked_after_travel_and_after_invocations() {
    let case = Case::new();
    let arms = arm("attempt:both");
    let ticket = case.ticket();
    let request = request(&case, &ticket, &arms, true);
    let both = EffectiveBounds { transition_limit: 0, invocations_per_day: 0, spend_per_day: 0 };

    let travel = case.open().preview(&request, both).expect_err("refused");
    assert_eq!(travel.reason_code, "travel_exhausted");

    let counts = EffectiveBounds { transition_limit: 80, invocations_per_day: 0, spend_per_day: 0 };
    let invocations = case.open().preview(&request, counts).expect_err("refused");
    assert_eq!(invocations.reason_code, "invocation_exhausted");

    let spend =
        EffectiveBounds { transition_limit: 80, invocations_per_day: 200, spend_per_day: 0 };
    let refused = case.open().preview(&request, spend).expect_err("refused");
    assert_eq!(refused.reason_code, "spend_exhausted");
    let exhaustion = refused.exhaustion.expect("a spent bound");
    assert_eq!(exhaustion.dimension, Dimension::Spend);
    assert_eq!(
        exhaustion.counter.reserved, RESERVE,
        "the refused request's own worst case is in the number a reader adds up"
    );
}

/// One currency per account, fixed by the first receipt carrying an amount,
/// and a second refused before any agent starts. Nothing is converted.
/// §FS-rhei-budgets.5.5
#[test]
fn a_second_currency_in_one_account_is_refused_before_anything_is_appended() {
    let case = Case::new();
    started(&case, "attempt:usd");
    let arms = arm("attempt:chf");
    let ticket = case.ticket();
    let mut request = request(&case, &ticket, &arms, false);
    request.spend_currency = "CHF";

    let mut journal = case.open();
    let before = journal.receipts().len();
    let refused = journal.reserve(&request, bounds(80, 200), &audit()).expect_err("refused");

    assert_eq!(refused.reason_code, "currency_conflict");
    assert!(
        refused.message.contains("USD") && refused.message.contains("CHF"),
        "the refusal names both, so a reader knows which is the account's; got: {}",
        refused.message
    );
    assert_eq!(journal.receipts().len(), before, "and it appended nothing");
}

/// A build that meets a kind it does not know **reads** the account and
/// refuses to append to it, which is what the closed-vocabulary rule promises
/// a downgrade across this version. Failing the open would leave an operator
/// unable even to see what stopped them. §FS-rhei-budgets.5.2
#[test]
fn an_unknown_receipt_kind_is_retained_and_makes_the_account_read_only() {
    let case = Case::new();
    started(&case, "attempt:before");
    let path = case.journal_path();
    let bytes = std::fs::read(&path).expect("read the journal");
    let last: serde_json::Value = serde_json::from_slice(
        bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()).last().expect("a receipt"),
    )
    .expect("parse");
    let mut newer = last.clone();
    newer["sequence"] = (last["sequence"].as_u64().expect("sequence") + 1).into();
    newer["receipt_id"] = format!("receipt:{}", uuid::Uuid::new_v4()).into();
    newer["previous_hash"] = serde_json::Value::Null;
    newer["kind"] = "conjecture".into();
    newer["payload"] = serde_json::json!({"from": "a later build"});
    // The chain is what it is: the hash of the line this one follows.
    let lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()).collect();
    newer["previous_hash"] = journal_digest(lines.last().expect("a line")).into();
    let mut appended = bytes.clone();
    appended.extend_from_slice(&serde_json::to_vec(&newer).expect("serialize"));
    appended.push(b'\n');
    std::fs::write(&path, &appended).expect("write the journal");
    std::fs::write(case.witness_path(), &appended).expect("write the witness");

    let journal = case.account.open(true).expect("an account newer than this build still opens");

    assert!(!journal.writable(), "and it may not be appended to");
    let snapshot = journal.snapshot().expect("it still reports");
    assert_eq!(snapshot.spend.reserved, RESERVE, "from the receipts it does understand");
    let refused = case
        .account
        .open(true)
        .expect("open")
        .release_travel("reservation:whatever", &audit())
        .expect_err("a read-only account appends nothing");
    assert!(
        refused.message.contains("conjecture"),
        "and it says which kind it did not understand; got: {}",
        refused.message
    );
}

/// The digest the chain is built on, spelled the way the journal spells it, so
/// this suite forges a *valid* newer chain rather than a damaged one.
fn journal_digest(line: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(line))
}
