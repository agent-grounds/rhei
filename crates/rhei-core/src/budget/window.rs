//! The day key, the one clock seam, and the renewal instant a halt quotes.
//!
//! Every caller in this module reads the clock through [`now`]; a second direct
//! read anywhere else is the bug this centralization exists to prevent.
//! §FS-rhei-budgets.3.3 §AR-neural-admission.5

use super::types::BudgetError;
use super::Result;
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime};

/// The environment variable that supplies the instant in place of the system
/// clock. It exists because the window is otherwise not testable on a portable
/// fixture without waiting for midnight, and it has exactly the authority the
/// system clock has and no more: the highest-day-key guard applies to it
/// unchanged. §FS-rhei-budgets.3.3.1
pub const CLOCK_ENV: &str = "RHEI_BUDGET_NOW";

/// The instant admission reckons by.
///
/// A `RHEI_BUDGET_NOW` that does not parse is an error naming the variable,
/// never a silent fallback to the system clock: falling back would make a typo
/// in a fixture look like a passing test. §FS-rhei-budgets.3.3.1
pub fn now() -> Result<OffsetDateTime> {
    let Some(raw) = std::env::var_os(CLOCK_ENV) else {
        return Ok(OffsetDateTime::now_utc());
    };
    let text = raw.to_str().ok_or_else(|| invalid_clock("it is not valid UTF-8"))?;
    OffsetDateTime::parse(text.trim(), &Rfc3339)
        .map(|at| at.to_offset(time::UtcOffset::UTC))
        .map_err(|err| invalid_clock(&format!("{text:?} is not an RFC 3339 instant: {err}")))
}

fn invalid_clock(why: &str) -> BudgetError {
    BudgetError::bounds(format!("{CLOCK_ENV} is set but {why}"))
}

/// The UTC calendar day an instant falls in, `YYYY-MM-DD`.
///
/// Not local time: daylight saving makes a local day 23 or 25 hours long and
/// the answer would differ per machine. §FS-rhei-budgets.3.3 §REQ-cross-platform.2
pub fn day_key(at: OffsetDateTime) -> String {
    let date = at.to_offset(time::UtcOffset::UTC).date();
    format!("{:04}-{:02}-{:02}", date.year(), u8::from(date.month()), date.day())
}

/// The instant a day key's window renews: `00:00:00Z` of the following day.
///
/// A calendar day rather than a rolling 24 hours, because a rolling window
/// would have to retain every timestamp forever and could not answer this
/// question at all. §FS-rhei-budgets.3.3
pub fn renewal_instant(day: &str) -> Result<String> {
    let next = parse_day(day)?
        .next_day()
        .ok_or_else(|| BudgetError::corrupt("day key has no following day"))?;
    Ok(format!("{:04}-{:02}-{:02}T00:00:00Z", next.year(), u8::from(next.month()), next.day()))
}

/// The day capacity is drawn against: the clock's day, never earlier than the
/// highest day the chain has ever recorded.
///
/// A clock moved backwards therefore keeps drawing on the recorded day and
/// mints nothing. A clock moved forwards does open a new day; that is the
/// honest residual, and an operator who can set the machine's clock can already
/// set the machine's settings key. §FS-rhei-budgets.3.3
pub fn effective_day(clock_day: &str, highest_recorded: Option<&str>) -> String {
    match highest_recorded {
        Some(recorded) if recorded > clock_day => recorded.to_string(),
        _ => clock_day.to_string(),
    }
}

/// An RFC 3339 UTC instant, to the second, for a receipt's `written_at`.
pub fn instant(at: OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// Parse `YYYY-MM-DD` without a format description, so the crate needs no
/// formatting feature for the one shape it ever reads.
fn parse_day(day: &str) -> Result<Date> {
    let malformed = || BudgetError::corrupt(format!("'{day}' is not a YYYY-MM-DD day key"));
    let parts: Vec<&str> = day.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err(malformed());
    }
    let year: i32 = parts[0].parse().map_err(|_| malformed())?;
    let month: u8 = parts[1].parse().map_err(|_| malformed())?;
    let day_of_month: u8 = parts[2].parse().map_err(|_| malformed())?;
    let month = Month::try_from(month).map_err(|_| malformed())?;
    Date::from_calendar_date(year, month, day_of_month).map_err(|_| malformed())
}
