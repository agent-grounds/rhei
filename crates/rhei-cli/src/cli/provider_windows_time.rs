//! Windows native-rule arithmetic for absolute Codex dates. §FS-rhei-run.3.3

use chrono::{
    DateTime, Datelike, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, Utc, Weekday,
};

#[cfg(windows)]
#[path = "provider_windows_native.rs"]
pub(crate) mod native;

/// The civil fields returned in a Windows transition SYSTEMTIME. A zero month
/// disables the transition; a zero year denotes an nth-weekday rule.
/// §FS-rhei-run.3.3
#[derive(Clone, Copy, Default)]
pub(crate) struct TransitionDate {
    pub year: u16,
    pub month: u16,
    pub weekday: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millis: u16,
}

/// Biases are minutes west of UTC. Standard/daylight adjustments apply only
/// when both transition dates are supplied by the native query.
/// §FS-rhei-run.3.3
#[derive(Clone, Copy, Default)]
pub(crate) struct YearRules {
    pub bias: i32,
    pub standard_bias: i32,
    pub daylight_bias: i32,
    pub standard: TransitionDate,
    pub daylight: TransitionDate,
}

/// Inject native rule data, rather than predefined uniqueness results. Both
/// production and portable full-sentence regressions use this conversion.
/// Query by the civil year, including after a following-minute rollover.
/// §FS-rhei-run.3.3
pub(crate) fn resolve_query(
    minute: &NaiveDateTime,
    query: &impl Fn(i32) -> Option<YearRules>,
) -> Option<DateTime<Utc>> {
    super::resolve_native_query(minute, &|minute| {
        query(minute.year()).and_then(|rules| rules.offsets(minute)).unwrap_or(LocalResult::None)
    })
}

impl YearRules {
    /// Validate each possible UTC candidate against the actual offset active
    /// at that instant. A transition takes effect at equality, so gaps and
    /// overlaps have half-open civil intervals for either offset direction.
    /// §FS-rhei-run.3.3
    fn offsets(self, minute: &NaiveDateTime) -> Option<LocalResult<FixedOffset>> {
        let offset = |bias: i32| bias.checked_mul(60).and_then(FixedOffset::west_opt);
        if self.standard.month == 0 && self.daylight.month == 0 {
            return Some(LocalResult::Single(offset(self.bias)?));
        }
        // A partial pair is invalid native data, never a fixed-offset fallback. §FS-rhei-run.3.3
        if self.standard.month == 0 || self.daylight.month == 0 {
            return None;
        }
        let standard = offset(self.bias.checked_add(self.standard_bias)?)?;
        let daylight = offset(self.bias.checked_add(self.daylight_bias)?)?;
        let to_daylight = self.daylight.at_year(minute.year())?.checked_sub_offset(standard)?;
        let to_standard = self.standard.at_year(minute.year())?.checked_sub_offset(daylight)?;
        if to_daylight == to_standard {
            return None;
        }
        let active = |utc: NaiveDateTime| {
            let in_interval = if to_daylight < to_standard {
                utc >= to_daylight && utc < to_standard
            } else {
                utc < to_standard || utc >= to_daylight
            };
            if in_interval {
                daylight
            } else {
                standard
            }
        };
        let mut found = None;
        for candidate in [standard, daylight] {
            let utc = minute.checked_sub_offset(candidate)?;
            if active(utc) == candidate {
                if let Some(first) = found {
                    if first != candidate {
                        return Some(LocalResult::Ambiguous(first, candidate));
                    }
                } else {
                    found = Some(candidate);
                }
            }
        }
        Some(found.map_or(LocalResult::None, LocalResult::Single))
    }
}

impl TransitionDate {
    /// Decode native recurring and absolute dates without rounding a transition
    /// or assuming that its daylight offset increases the wall clock.
    /// §FS-rhei-run.3.3
    fn at_year(self, year: i32) -> Option<NaiveDateTime> {
        let date = if self.year != 0 {
            NaiveDate::from_ymd_opt(self.year.into(), self.month.into(), self.day.into())?
        } else {
            let weekday = match self.weekday {
                0 => Weekday::Sun,
                1 => Weekday::Mon,
                2 => Weekday::Tue,
                3 => Weekday::Wed,
                4 => Weekday::Thu,
                5 => Weekday::Fri,
                6 => Weekday::Sat,
                _ => return None,
            };
            let nth = match self.day {
                1..=5 => self.day as u8,
                _ => return None,
            };
            NaiveDate::from_weekday_of_month_opt(year, self.month.into(), weekday, nth).or_else(
                || {
                    (nth == 5)
                        .then(|| {
                            NaiveDate::from_weekday_of_month_opt(
                                year,
                                self.month.into(),
                                weekday,
                                4,
                            )
                        })
                        .flatten()
                },
            )?
        };
        date.and_hms_milli_opt(
            self.hour.into(),
            self.minute.into(),
            self.second.into(),
            self.millis.into(),
        )
    }
}
