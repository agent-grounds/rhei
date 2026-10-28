//! The names an `unpriced` count carries: how receipts with a pricing context
//! group, in what order, in which words, and over which day.
//! §FS-rhei-budgets.8.1 §FS-rhei-budgets.5.2 §FS-rhei-budgets.10

use super::test_support::*;
use super::*;

const RESERVE: u64 = built_in::SPEND_RESERVE;

fn context(
    source: &str,
    id: &str,
    path: Option<&str>,
    pair: Option<(&str, &str)>,
) -> PricingContext {
    PricingContext {
        price_book_source: Some(source.into()),
        price_book_id: Some(id.into()),
        price_book_path: path.map(str::to_owned),
        provider: pair.map(|(provider, _)| provider.into()),
        model: pair.map(|(_, model)| model.into()),
        missing_entry: pair.map(|_| true),
    }
}

/// Admit, start and settle one arm with `basis` and `context`.
fn settle(case: &Case, attempt: &str, basis: SpendBasis, context: Option<&PricingContext>) {
    let mut journal = case.open();
    let arms = arm(attempt);
    let group = journal
        .reserve(&request(case, &case.ticket(), &arms, false), bounds(80, 200), &audit())
        .expect("reserve");
    let id = group.reservation_ids[0].clone();
    journal.record_start(&id, true, &audit()).expect("start");
    journal.settle_spend_in_context(&id, RESERVE, "USD", basis, context, &audit()).expect("settle");
}

fn marks(case: &Case) -> SpendMarks {
    case.account.open(false).expect("open").snapshot().expect("snapshot").marks(&|_| false)
}

/// Receipts of two books and an old receipt group by book, unknown first,
/// pairs ascending within a group; the other marks are counted but never
/// named, and the JSON entries sum to `unpriced`. §FS-rhei-budgets.8.1
#[test]
fn unpriced_receipts_group_by_book_in_order_and_other_marks_stay_unnamed() {
    let case = Case::new();
    let file = |pair| context("file", "site-1", Some("/work/prices.json"), Some(pair));
    let builtin = context("builtin", "builtin-2026-05-20", None, Some(("openai", "gpt-9")));
    settle(&case, "attempt:sonnet", SpendBasis::Unpriced, Some(&file(("anthropic", "sonnet"))));
    settle(&case, "attempt:opus-1", SpendBasis::Unpriced, Some(&file(("anthropic", "opus"))));
    settle(&case, "attempt:opus-2", SpendBasis::Unpriced, Some(&file(("anthropic", "opus"))));
    settle(&case, "attempt:gpt", SpendBasis::Unpriced, Some(&builtin));
    settle(&case, "attempt:old", SpendBasis::Unpriced, None);
    // A context on any other basis is not written, so it names nothing.
    settle(&case, "attempt:none", SpendBasis::Unmeasurable, Some(&builtin));

    let marks = marks(&case);

    assert_eq!((marks.unpriced, marks.unmeasurable), (5, 1));
    assert_eq!(
        marks.continuations(),
        [
            "1 with no pricing context (settled before rhei recorded one)",
            "openai/gpt-9 (1) has no entry in price book builtin-2026-05-20 (built-in)",
            "anthropic/opus (2), anthropic/sonnet (1) have no entry in price book site-1 \
             (/work/prices.json)",
        ]
    );
    let details = details_json(&marks.unpriced_details);
    let counts: u64 =
        details.as_array().unwrap().iter().map(|d| d["count"].as_u64().unwrap()).sum();
    assert_eq!(counts, 5, "the counts sum to `unpriced`: {details}");
    assert!(details[0]["price_book_source"].is_null(), "null sorts first: {details}");
    assert!(details[0].as_object().unwrap().iter().all(|(k, v)| k == "count" || v.is_null()));
    assert_eq!(
        marks.one_line(RESERVE, Some("USD")).unwrap(),
        "5 unpriced, 1 unmeasurable (charged at $20.00 each): 1 with no pricing context \
         (settled before rhei recorded one); openai/gpt-9 (1) has no entry in price book \
         builtin-2026-05-20 (built-in); anthropic/opus (2), anthropic/sonnet (1) have no entry \
         in price book site-1 (/work/prices.json)"
    );
}

/// Each clause of §FS-rhei-budgets.8.1's table, and an unknown model listed
/// first in its group.
#[test]
fn each_pricing_context_reads_in_its_own_words() {
    let lines = |context: PricingContext| {
        let details = [UnpricedDetail { context, count: 3 }];
        unpriced::group_lines(&details)
    };
    let mut present = context("profiles", "profiles-sha256-ab", None, Some(("a", "m")));
    present.missing_entry = Some(false);
    assert_eq!(
        lines(present),
        ["a/m (3): price book profiles-sha256-ab (composed from profile prices) has an entry, \
          but no measured usage could be priced"]
    );
    let unchecked = context("builtin", "b", None, None);
    assert_eq!(
        lines(unchecked),
        ["an unknown model (3): could not be checked against price book b (built-in)"]
    );
    let none =
        PricingContext { price_book_source: Some("none".into()), ..PricingContext::default() };
    assert_eq!(lines(none), ["an unknown model (3): no price book was selected"]);

    let book = |pair| context("file", "s", Some("/p.json"), pair);
    let mut unknown = book(None);
    unknown.missing_entry = Some(true);
    let details = [
        UnpricedDetail { context: book(Some(("z", "m"))), count: 1 },
        UnpricedDetail { context: unknown, count: 2 },
    ];
    assert_eq!(
        unpriced::group_lines(&details),
        ["an unknown model (2), z/m (1) have no entry in price book s (/p.json)"]
    );
}

/// A receipt's context survives the journal unchanged, and a context of the
/// wrong shape reads as the unknown one rather than a guess.
/// §FS-rhei-budgets.5.2
#[test]
fn a_pricing_context_round_trips_and_a_malformed_one_is_unknown() {
    let written = context("file", "s", Some("/p.json"), Some(("a", "m")));
    assert_eq!(PricingContext::from_json(Some(&written.to_json())), written);
    assert_eq!(
        PricingContext::from_json(Some(&serde_json::json!("file"))),
        PricingContext::default()
    );
    let wrong = serde_json::json!({"provider": 7, "missing_entry": "yes"});
    assert_eq!(PricingContext::from_json(Some(&wrong)), PricingContext::default());
}

/// Only the window day's receipts are named: yesterday's unpriced settles
/// neither count nor name today. §FS-rhei-budgets.8.1 §FS-rhei-budgets.3.4
#[test]
fn yesterdays_unpriced_receipts_are_not_named_today() {
    let case = Case::at("2026-09-24T12:00:00Z");
    let yesterday = context("file", "s", Some("/p.json"), Some(("a", "old")));
    settle(&case, "attempt:yesterday", SpendBasis::Unpriced, Some(&yesterday));
    case.set_clock("2026-09-25T09:00:00Z");
    let today = context("file", "s", Some("/p.json"), Some(("a", "new")));
    settle(&case, "attempt:today", SpendBasis::Unpriced, Some(&today));

    let marks = marks(&case);

    assert_eq!(marks.unpriced, 1);
    assert_eq!(marks.continuations(), ["a/new (1) has no entry in price book s (/p.json)"]);
}

/// The halt aligns each group under its value column, between `estimated:`
/// and `mode:`, and adds no second remedy. §FS-rhei-budgets.8.1
#[test]
fn the_halt_aligns_the_groups_under_its_value_column() {
    let spent = Exhaustion {
        dimension: Dimension::Spend,
        subject: "panta".into(),
        contract: Contract::Window,
        bound: 40 * crate::money::MICRO,
        counter: Counter { consumed: 40 * crate::money::MICRO, reserved: 0 },
        day: "2026-09-26".into(),
        currency: Some("USD".into()),
    };
    let bound = Bound::resolve_money("spend_per_day", 40 * crate::money::MICRO, None, None);
    let details =
        vec![UnpricedDetail { context: context("builtin", "b", None, Some(("a", "m"))), count: 2 }];
    let marks = SpendMarks { unpriced: 2, unpriced_details: details, ..SpendMarks::default() };

    let halt = halt_text(&spent, &bound, &Remedy::Renews("2026-09-27T00:00:00Z".into()), &marks);

    assert!(
        halt.contains(
            "\n       estimated:   2 unpriced (charged at $20.00 each)\n                    \
             a/m (2) has no entry in price book b (built-in)\n       mode:"
        ),
        "got:\n{halt}"
    );
    assert_eq!(halt.matches("renews at:").count(), 1);
}
