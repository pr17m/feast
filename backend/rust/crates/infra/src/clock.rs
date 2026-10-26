//! Clocks. "Local" time is a city's IANA zone (passed per call), because slots,
//! "today" and opening hours are local concepts. An empty or unknown zone
//! falls back to the deployment's default (the country's main zone).

use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, SecondsFormat, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use feast_application::ports::Clock;

#[derive(Debug, Clone)]
pub struct SystemClock {
    tz: Tz,
}

impl SystemClock {
    pub fn new(tz: Tz) -> Self {
        Self { tz }
    }

    /// Parse an IANA zone name like `Asia/Kolkata`.
    pub fn from_name(name: &str) -> Result<Self, String> {
        name.parse::<Tz>().map(Self::new).map_err(|e| format!("bad timezone '{name}': {e}"))
    }
}

impl Clock for SystemClock {
    fn now_iso(&self) -> String {
        iso(Utc::now())
    }
    fn now_ms(&self) -> i64 {
        Utc::now().timestamp_millis()
    }
    fn local_date(&self, tz: &str, days: i64) -> String {
        local_date(zone(tz, self.tz), Utc::now(), days)
    }
    fn local_minutes(&self, tz: &str) -> i64 {
        local_minutes(zone(tz, self.tz), Utc::now())
    }
    fn local_to_ms(&self, tz: &str, date: &str, hhmm: &str) -> Option<i64> {
        local_to_ms(zone(tz, self.tz), date, hhmm)
    }
    fn local_date_of(&self, tz: &str, iso: &str) -> Option<String> {
        local_date_of(zone(tz, self.tz), iso)
    }
}

/// A controllable clock for tests: starts at a fixed instant and only moves when told.
#[derive(Debug)]
pub struct FixedClock {
    tz: Tz,
    ms: AtomicI64,
}

impl FixedClock {
    pub fn new(tz: Tz, start: DateTime<Utc>) -> Self {
        Self { tz, ms: AtomicI64::new(start.timestamp_millis()) }
    }
    pub fn advance(&self, by: Duration) {
        self.ms.fetch_add(by.num_milliseconds(), Ordering::SeqCst);
    }
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.ms.load(Ordering::SeqCst)).single().unwrap_or_else(Utc::now)
    }
}

impl Clock for FixedClock {
    fn now_iso(&self) -> String {
        iso(self.now())
    }
    fn now_ms(&self) -> i64 {
        self.ms.load(Ordering::SeqCst)
    }
    fn local_date(&self, tz: &str, days: i64) -> String {
        local_date(zone(tz, self.tz), self.now(), days)
    }
    fn local_minutes(&self, tz: &str) -> i64 {
        local_minutes(zone(tz, self.tz), self.now())
    }
    fn local_to_ms(&self, tz: &str, date: &str, hhmm: &str) -> Option<i64> {
        local_to_ms(zone(tz, self.tz), date, hhmm)
    }
    fn local_date_of(&self, tz: &str, iso: &str) -> Option<String> {
        local_date_of(zone(tz, self.tz), iso)
    }
}

fn zone(name: &str, default: Tz) -> Tz {
    if name.is_empty() {
        default
    } else {
        name.parse().unwrap_or(default)
    }
}

pub fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn local_date(tz: Tz, now: DateTime<Utc>, days: i64) -> String {
    (now.with_timezone(&tz).date_naive() + Duration::days(days)).format("%Y-%m-%d").to_string()
}

fn local_minutes(tz: Tz, now: DateTime<Utc>) -> i64 {
    let t = now.with_timezone(&tz);
    i64::from(t.hour()) * 60 + i64::from(t.minute())
}

fn local_to_ms(tz: Tz, date: &str, hhmm: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let t = NaiveTime::parse_from_str(hhmm, "%H:%M").ok()?;
    tz.from_local_datetime(&d.and_time(t)).earliest().map(|x| x.timestamp_millis())
}

fn local_date_of(tz: Tz, iso: &str) -> Option<String> {
    let t = DateTime::parse_from_rfc3339(iso).ok()?;
    Some(t.with_timezone(&tz).date_naive().format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_conversions_use_the_zone() {
        let start = Utc.with_ymd_and_hms(2026, 3, 1, 20, 0, 0).unwrap(); // 01:30 IST next day
        let c = FixedClock::new(chrono_tz::Asia::Kolkata, start);
        assert_eq!(c.local_date("", 0), "2026-03-02");
        assert_eq!(c.local_minutes(""), 90);
        assert_eq!(c.local_to_ms("Asia/Kolkata", "2026-03-02", "01:30"), Some(start.timestamp_millis()));
        assert_eq!(c.local_date_of("", "2026-03-01T20:00:00.000Z").as_deref(), Some("2026-03-02"));
        // Per-city zones: 20:00 UTC is 07:00 next day in Sydney (AEDT).
        assert_eq!(c.local_date("Australia/Sydney", 0), "2026-03-02");
        assert_eq!(c.local_minutes("Australia/Sydney"), 7 * 60);
        c.advance(Duration::minutes(5));
        assert_eq!(c.local_minutes(""), 95);
        assert!(c.now_iso().ends_with('Z'));
    }
}
