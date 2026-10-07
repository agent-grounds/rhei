//! Private OS-local conversion for absolute Codex refusals. §FS-rhei-run.3.3

use chrono::{DateTime, NaiveDateTime, Utc};

#[cfg(unix)]
pub(super) struct LocalZone(tz::TimeZone);

/// Retain one successfully loaded zone for both civil minutes. Discovery
/// failure is distinct from intentionally configured UTC. §FS-rhei-run.3.3
#[cfg(unix)]
pub(super) fn load() -> Option<LocalZone> {
    select_unix_zone(
        std::env::var("TZ").ok().as_deref(),
        std::path::Path::new("/etc/localtime"),
        &|| {
            // Includes Linux name/config fallbacks and macOS CoreFoundation lookup.
            iana_time_zone::get_timezone().ok()
        },
    )
}

/// Injectable source paths/discovery use the production fallible TZif/POSIX
/// loaders. Valid copied data needs no IANA name. §FS-rhei-run.3.3
#[cfg(unix)]
pub(super) fn select_unix_zone(
    explicit: Option<&str>,
    localtime: &std::path::Path,
    native_name: &impl Fn() -> Option<String>,
) -> Option<LocalZone> {
    let explicit_zone = explicit.and_then(|source| match source {
        "" => Some(tz::TimeZone::utc()),
        "localtime" => load_tzif(localtime),
        _ => tz::TimeZone::from_posix_tz(source).ok(),
    });
    explicit_zone
        .or_else(|| load_tzif(localtime))
        .or_else(|| native_name().and_then(|source| tz::TimeZone::from_posix_tz(&source).ok()))
        .map(LocalZone)
}

#[cfg(unix)]
fn load_tzif(path: &std::path::Path) -> Option<tz::TimeZone> {
    tz::TimeZone::from_tz_data(&std::fs::read(path).ok()?).ok()
}

#[cfg(unix)]
impl LocalZone {
    /// tz-rs rejects skipped and multiply resolved times, including the first
    /// missing minute and the first unique minute after overlap. §FS-rhei-run.3.3
    pub(super) fn resolve(&self, minute: &NaiveDateTime) -> Option<DateTime<Utc>> {
        use chrono::{Datelike, Timelike};
        let found = tz::DateTime::find(
            minute.year(),
            minute.month() as u8,
            minute.day() as u8,
            minute.hour() as u8,
            minute.minute() as u8,
            minute.second() as u8,
            minute.nanosecond(),
            self.0.as_ref(),
        )
        .ok()?
        .unique()?;
        DateTime::from_timestamp(found.unix_time(), found.nanoseconds())
    }
}

#[cfg(windows)]
pub(super) struct LocalZone;

/// Windows keeps Chrono's native, fallible per-year query backend; it never
/// uses tz-rs's non-Unix UTC fallback. §FS-rhei-run.3.3
#[cfg(windows)]
pub(super) fn load() -> Option<LocalZone> {
    Some(LocalZone)
}

#[cfg(windows)]
impl LocalZone {
    pub(super) fn resolve(&self, minute: &NaiveDateTime) -> Option<DateTime<Utc>> {
        use chrono::TimeZone;
        resolve_native_query(minute, &|minute| chrono::Local.offset_from_local_datetime(minute))
    }
}

/// Preserve native-query failure and ambiguity rather than choosing an offset.
/// The injected query is the same boundary Windows production uses. §FS-rhei-run.3.3
#[cfg(any(windows, test))]
pub(super) fn resolve_native_query(
    minute: &NaiveDateTime,
    query: &impl Fn(&NaiveDateTime) -> chrono::LocalResult<chrono::FixedOffset>,
) -> Option<DateTime<Utc>> {
    use chrono::TimeZone;
    query(minute)
        .single()?
        .from_local_datetime(minute)
        .single()
        .map(|value| value.with_timezone(&Utc))
}
