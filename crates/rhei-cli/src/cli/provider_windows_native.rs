//! Fallible Windows local selection and year-rule queries. §FS-rhei-run.3.3

use super::{resolve_query, TransitionDate, YearRules};
use chrono::{DateTime, NaiveDateTime, Utc};
use std::{cell::RefCell, collections::BTreeMap};
use windows_sys::Win32::{
    Foundation::SYSTEMTIME,
    System::Time::{
        GetDynamicTimeZoneInformation, GetTimeZoneInformationForYear,
        DYNAMIC_TIME_ZONE_INFORMATION, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    },
};

/// Retain the selected OS zone and each queried local year's rule data for
/// both minutes; errors never become UTC or a guessed named zone.
/// §FS-rhei-run.3.3
pub(crate) struct LocalZone {
    selected: DYNAMIC_TIME_ZONE_INFORMATION,
    years: RefCell<BTreeMap<i32, Option<YearRules>>>,
}

pub(crate) fn load() -> Option<LocalZone> {
    let mut selected = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // SAFETY: the API writes a valid, fully sized output structure. §FS-rhei-run.3.3
    if unsafe { GetDynamicTimeZoneInformation(&mut selected) } == TIME_ZONE_ID_INVALID {
        return None;
    }
    Some(LocalZone::from_selected(selected))
}

impl LocalZone {
    pub(crate) fn from_selected(selected: DYNAMIC_TIME_ZONE_INFORMATION) -> Self {
        Self { selected, years: RefCell::new(BTreeMap::new()) }
    }

    pub(crate) fn resolve(&self, minute: &NaiveDateTime) -> Option<DateTime<Utc>> {
        resolve_query(minute, &|year| {
            *self.years.borrow_mut().entry(year).or_insert_with(|| self.query(year))
        })
    }

    fn query(&self, year: i32) -> Option<YearRules> {
        let year = u16::try_from(year).ok()?;
        let mut rules = TIME_ZONE_INFORMATION::default();
        // SAFETY: both pointers refer to live, correctly sized structures.
        // Keep the native failure distinct from valid zero-offset rules. §FS-rhei-run.3.3
        if unsafe { GetTimeZoneInformationForYear(year, &self.selected, &mut rules) } == 0 {
            return None;
        }
        Some(YearRules {
            bias: rules.Bias,
            standard_bias: rules.StandardBias,
            daylight_bias: rules.DaylightBias,
            standard: transition(rules.StandardDate),
            daylight: transition(rules.DaylightDate),
        })
    }
}

fn transition(time: SYSTEMTIME) -> TransitionDate {
    TransitionDate {
        year: time.wYear,
        month: time.wMonth,
        weekday: time.wDayOfWeek,
        day: time.wDay,
        hour: time.wHour,
        minute: time.wMinute,
        second: time.wSecond,
        millis: time.wMilliseconds,
    }
}
