//! The day key, the guard that keeps a clock from reaching backwards, and the
//! renewal instant a halt quotes.
//! §FS-rhei-budgets.3.3 §FS-rhei-budgets.3.3.1

use super::test_support::*;
use super::*;

/// The day is the UTC calendar day, whatever offset the instant carries. An
/// instant late on the 24th in Tokyo is the 24th here only if it is still the
/// 24th in UTC. §FS-rhei-budgets.3.3
#[test]
fn the_day_key_is_the_utc_calendar_day_of_the_instant() {
    let case = Case::at("2026-09-25T08:30:00+09:00");

    // 08:30 on the 25th in Tokyo is 23:30 on the 24th in UTC.
    assert_eq!(day_key(now().expect("clock")), "2026-09-24");
    drop(case);
}

/// The variable supplies the instant in place of the system clock, and gets
/// exactly that authority: it says what day it is, not what the balance is.
/// §FS-rhei-budgets.3.3.1
#[test]
fn the_clock_variable_supplies_the_instant_admission_reckons_by() {
    let case = Case::at("2026-09-24T12:00:00Z");
    assert_eq!(day_key(now().expect("clock")), "2026-09-24");

    case.set_clock("2027-01-01T00:00:01Z");

    assert_eq!(day_key(now().expect("clock")), "2027-01-01");
}

/// A value that does not parse is an error naming the variable. Falling back
/// to the system clock would make a typo in a fixture look like a passing test,
/// which is the one failure a test seam must not have.
/// §FS-rhei-budgets.3.3.1
#[test]
fn an_unparseable_clock_is_an_error_naming_the_variable_not_a_fallback() {
    let case = Case::new();
    case.set_clock("yesterday afternoon");

    let refused = now().expect_err("an unparseable instant is refused");

    assert!(refused.message.contains(CLOCK_ENV), "{}", refused.message);
    assert!(refused.message.contains("RFC 3339"), "{}", refused.message);
}

/// The window renews at midnight UTC of the following day, which is what a
/// window-limited halt prints instead of a settings key.
/// §FS-rhei-budgets.3.3 §FS-rhei-budgets.8
#[test]
fn a_day_renews_at_midnight_utc_of_the_following_day() {
    assert_eq!(renewal_instant("2026-09-24").expect("renewal"), "2026-09-25T00:00:00Z");
    assert_eq!(
        renewal_instant("2026-12-31").expect("renewal"),
        "2027-01-01T00:00:00Z",
        "a year boundary is an ordinary day boundary"
    );
    assert_eq!(
        renewal_instant("2028-02-28").expect("renewal"),
        "2028-02-29T00:00:00Z",
        "and a leap day is an ordinary day"
    );
}

/// The day capacity is drawn against is never earlier than the highest day the
/// chain records, so reaching for yesterday mints nothing. Reaching forward
/// does open a new day; that is the stated residual.
/// §FS-rhei-budgets.3.3
#[test]
fn the_effective_day_is_never_earlier_than_the_highest_one_recorded() {
    use super::window::effective_day;

    assert_eq!(effective_day("2026-09-24", None), "2026-09-24");
    assert_eq!(
        effective_day("2026-09-20", Some("2026-09-24")),
        "2026-09-24",
        "a clock moved backwards keeps drawing on the day already spent"
    );
    assert_eq!(
        effective_day("2026-09-25", Some("2026-09-24")),
        "2026-09-25",
        "a clock moved forwards opens the next day, which it could anyway"
    );
}

/// The whole of the above, on a real account: spend a day, reach backwards, and
/// find the day still spent. §FS-rhei-budgets.3.3
#[test]
fn an_account_drawn_on_an_earlier_clock_keeps_the_day_it_already_spent() {
    let case = Case::at("2026-09-25T01:00:00Z");
    let ticket = case.ticket();
    {
        let mut journal = case.open();
        let arms = arm("attempt-1");
        journal
            .reserve(&request(&case, &ticket, &arms, false), bounds(80, 1), &audit())
            .expect("the day's one unit");
    }

    case.set_clock("2026-09-24T12:00:00Z");
    let mut journal = case.account.open(true).expect("reopen on an earlier clock");

    assert_eq!(journal.day(), "2026-09-25", "an earlier day is not a new day");
    let arms = arm("attempt-2");
    let refused = journal
        .reserve(&request(&case, &ticket, &arms, false), bounds(80, 1), &audit())
        .expect_err("the recorded day is still spent");
    assert_eq!(refused.reason_code, "invocation_exhausted");
}
