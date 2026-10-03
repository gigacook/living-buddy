//! Alarm-clock style reminders: a time of day on chosen weekdays, or once on a date.
//!
//! The next fire time is computed from the last time the alarm fired, so an
//! alarm missed while the app was closed is reported once ("missed at 07:00")
//! instead of being silently dropped or repeated.

use crate::recurrence::resolve_local;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;

#[derive(Clone, Debug)]
pub struct AlarmRule {
    pub time: NaiveTime,
    /// Bit 0 = Monday ... bit 6 = Sunday. Zero means every day.
    pub weekdays: u8,
    /// When set, the alarm fires once on this date.
    pub date: Option<NaiveDate>,
    pub tz: Tz,
    pub enabled: bool,
    pub snoozed_until: Option<DateTime<Utc>>,
}

/// First fire time strictly after `after` (typically the last fire or creation time).
pub fn next_fire(rule: &AlarmRule, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if !rule.enabled {
        return None;
    }
    if let Some(s) = rule.snoozed_until {
        if s > after {
            return Some(s);
        }
    }
    if let Some(date) = rule.date {
        let at = resolve_local(rule.tz, date.and_time(rule.time));
        return (at > after).then_some(at);
    }
    let local_after = after.with_timezone(&rule.tz).date_naive();
    for i in 0..8 {
        let d = local_after + Duration::days(i);
        let bit = 1u8 << d.weekday().num_days_from_monday();
        if rule.weekdays != 0 && rule.weekdays & bit == 0 {
            continue;
        }
        let at = resolve_local(rule.tz, d.and_time(rule.time));
        if at > after {
            return Some(at);
        }
    }
    None
}

pub fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn rule(weekdays: u8) -> AlarmRule {
        AlarmRule {
            time: parse_hhmm("07:30").unwrap(),
            weekdays,
            date: None,
            tz: "Europe/Stockholm".parse().unwrap(),
            enabled: true,
            snoozed_until: None,
        }
    }

    #[test]
    fn weekday_alarm() {
        // Saturday 2026-10-03 10:00 UTC; weekdays only -> Monday 07:30 local (05:30 UTC, CEST).
        let after = Utc.with_ymd_and_hms(2026, 10, 3, 10, 0, 0).unwrap();
        let n = next_fire(&rule(0b0011111), after).unwrap();
        assert_eq!(n, Utc.with_ymd_and_hms(2026, 10, 5, 5, 30, 0).unwrap());
        // Every day -> Sunday.
        let n = next_fire(&rule(0), after).unwrap();
        assert_eq!(n, Utc.with_ymd_and_hms(2026, 10, 4, 5, 30, 0).unwrap());
    }

    #[test]
    fn missed_alarm_is_reported_once() {
        let created = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let first = next_fire(&rule(0), created).unwrap();
        assert_eq!(first, Utc.with_ymd_and_hms(2026, 10, 1, 5, 30, 0).unwrap());
        // After it fires (even late), the next one is the following day.
        let fired_late = Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap();
        assert_eq!(next_fire(&rule(0), fired_late).unwrap(), Utc.with_ymd_and_hms(2026, 10, 4, 5, 30, 0).unwrap());
    }

    #[test]
    fn one_shot_snooze_disabled() {
        let mut r = rule(0);
        r.date = NaiveDate::from_ymd_opt(2026, 12, 24);
        let after = Utc.with_ymd_and_hms(2026, 10, 3, 0, 0, 0).unwrap();
        assert_eq!(next_fire(&r, after).unwrap(), Utc.with_ymd_and_hms(2026, 12, 24, 6, 30, 0).unwrap());
        assert!(next_fire(&r, Utc.with_ymd_and_hms(2026, 12, 25, 0, 0, 0).unwrap()).is_none());
        r.snoozed_until = Some(after + Duration::minutes(10));
        assert_eq!(next_fire(&r, after), r.snoozed_until);
        r.enabled = false;
        assert!(next_fire(&r, after).is_none());
    }
}
