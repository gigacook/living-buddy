//! A deliberately bounded RFC 5545 RRULE implementation shared by calendar
//! events and recurring tasks.
//!
//! Supported: FREQ=DAILY|WEEKLY|MONTHLY|YEARLY, INTERVAL, COUNT, UNTIL, BYDAY
//! (with ordinals for monthly/yearly), BYMONTHDAY (including negative days),
//! BYMONTH and WKST. Other parts (BYSETPOS, BYHOUR, BYWEEKNO, ...) are rejected
//! with [`RRuleError::Unsupported`] so callers can surface an honest warning
//! instead of silently producing wrong dates.
//!
//! Expansion happens in local wall-clock time and is converted to UTC per
//! occurrence, which keeps "every Monday at 09:00" at 09:00 across DST changes.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeekdayNum {
    pub ordinal: Option<i32>,
    pub weekday: Weekday,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Until {
    Date(NaiveDate),
    Local(NaiveDateTime),
    Utc(DateTime<Utc>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RRule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<Until>,
    pub by_day: Vec<WeekdayNum>,
    pub by_month_day: Vec<i32>,
    pub by_month: Vec<u32>,
    pub week_start: Weekday,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RRuleError {
    #[error("recurrence rule has no FREQ")]
    MissingFreq,
    #[error("recurrence part {0} is not supported yet")]
    Unsupported(String),
    #[error("recurrence part {0} has an invalid value")]
    Invalid(String),
}

/// Upper bound on generated periods, protecting against hostile or absurd rules.
const MAX_PERIODS: usize = 50_000;

fn parse_weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

pub fn weekday_code(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

/// Parses an iCalendar basic-format date or date-time used in UNTIL/EXDATE values.
pub fn parse_basic_datetime(v: &str) -> Option<Until> {
    let v = v.trim();
    if v.len() == 8 {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(Until::Date);
    }
    if let Some(stripped) = v.strip_suffix('Z') {
        return NaiveDateTime::parse_from_str(stripped, "%Y%m%dT%H%M%S").ok().map(|n| Until::Utc(Utc.from_utc_datetime(&n)));
    }
    NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok().map(Until::Local)
}

impl RRule {
    pub fn new(freq: Freq) -> Self {
        RRule {
            freq,
            interval: 1,
            count: None,
            until: None,
            by_day: vec![],
            by_month_day: vec![],
            by_month: vec![],
            week_start: Weekday::Mon,
        }
    }

    pub fn parse(input: &str) -> Result<Self, RRuleError> {
        let input = input.trim();
        let input = input.strip_prefix("RRULE:").unwrap_or(input);
        let mut freq = None;
        let mut rule = RRule::new(Freq::Daily);
        for part in input.split(';').filter(|p| !p.is_empty()) {
            let (key, value) = part.split_once('=').ok_or_else(|| RRuleError::Invalid(part.to_string()))?;
            let key = key.trim().to_ascii_uppercase();
            let value = value.trim();
            match key.as_str() {
                "FREQ" => {
                    freq = Some(match value.to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        "HOURLY" | "MINUTELY" | "SECONDLY" => return Err(RRuleError::Unsupported(format!("FREQ={value}"))),
                        _ => return Err(RRuleError::Invalid("FREQ".into())),
                    })
                }
                "INTERVAL" => {
                    rule.interval = value
                        .parse::<u32>()
                        .ok()
                        .filter(|n| (1..=1000).contains(n))
                        .ok_or_else(|| RRuleError::Invalid("INTERVAL".into()))?
                }
                "COUNT" => {
                    rule.count = Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|n| (1..=100_000).contains(n))
                            .ok_or_else(|| RRuleError::Invalid("COUNT".into()))?,
                    )
                }
                "UNTIL" => rule.until = Some(parse_basic_datetime(value).ok_or_else(|| RRuleError::Invalid("UNTIL".into()))?),
                "BYDAY" => {
                    for item in value.split(',') {
                        let item = item.trim().to_ascii_uppercase();
                        if item.len() < 2 {
                            return Err(RRuleError::Invalid("BYDAY".into()));
                        }
                        let (num, day) = item.split_at(item.len() - 2);
                        let weekday = parse_weekday(day).ok_or_else(|| RRuleError::Invalid("BYDAY".into()))?;
                        let ordinal = if num.is_empty() {
                            None
                        } else {
                            let n: i32 = num.trim_start_matches('+').parse().map_err(|_| RRuleError::Invalid("BYDAY".into()))?;
                            if n == 0 || n.abs() > 53 {
                                return Err(RRuleError::Invalid("BYDAY".into()));
                            }
                            Some(n)
                        };
                        rule.by_day.push(WeekdayNum { ordinal, weekday });
                    }
                }
                "BYMONTHDAY" => {
                    for item in value.split(',') {
                        let n: i32 = item
                            .trim()
                            .parse()
                            .ok()
                            .filter(|n: &i32| *n != 0 && n.abs() <= 31)
                            .ok_or_else(|| RRuleError::Invalid("BYMONTHDAY".into()))?;
                        rule.by_month_day.push(n);
                    }
                }
                "BYMONTH" => {
                    for item in value.split(',') {
                        let n: u32 = item
                            .trim()
                            .parse()
                            .ok()
                            .filter(|n| (1..=12).contains(n))
                            .ok_or_else(|| RRuleError::Invalid("BYMONTH".into()))?;
                        rule.by_month.push(n);
                    }
                }
                "WKST" => rule.week_start = parse_weekday(&value.to_ascii_uppercase()).ok_or_else(|| RRuleError::Invalid("WKST".into()))?,
                k if k.starts_with("X-") => {}
                other => return Err(RRuleError::Unsupported(other.to_string())),
            }
        }
        rule.freq = freq.ok_or(RRuleError::MissingFreq)?;
        if rule.count.is_some() && rule.until.is_some() {
            return Err(RRuleError::Invalid("COUNT and UNTIL together".into()));
        }
        if (rule.freq == Freq::Daily || rule.freq == Freq::Weekly) && rule.by_day.iter().any(|d| d.ordinal.is_some()) {
            return Err(RRuleError::Invalid("BYDAY ordinal with DAILY/WEEKLY".into()));
        }
        rule.by_month.sort_unstable();
        rule.by_month.dedup();
        Ok(rule)
    }

    /// Human-friendly summary used by the UI ("Every 2 weeks on Mon, Thu").
    pub fn describe(&self) -> String {
        let unit = match self.freq {
            Freq::Daily => "day",
            Freq::Weekly => "week",
            Freq::Monthly => "month",
            Freq::Yearly => "year",
        };
        let mut s = if self.interval == 1 { format!("Every {unit}") } else { format!("Every {} {unit}s", self.interval) };
        if !self.by_day.is_empty() {
            let days: Vec<String> = self
                .by_day
                .iter()
                .map(|d| {
                    let name = match d.weekday {
                        Weekday::Mon => "Mon",
                        Weekday::Tue => "Tue",
                        Weekday::Wed => "Wed",
                        Weekday::Thu => "Thu",
                        Weekday::Fri => "Fri",
                        Weekday::Sat => "Sat",
                        Weekday::Sun => "Sun",
                    };
                    match d.ordinal {
                        Some(-1) => format!("last {name}"),
                        Some(n) if n > 0 => format!("{} {name}", ordinal_word(n)),
                        Some(n) => format!("{n} {name}"),
                        None => name.to_string(),
                    }
                })
                .collect();
            s.push_str(" on ");
            s.push_str(&days.join(", "));
        }
        if !self.by_month_day.is_empty() {
            let days: Vec<String> =
                self.by_month_day.iter().map(|d| if *d == -1 { "last day".into() } else { format!("day {d}") }).collect();
            s.push_str(" on ");
            s.push_str(&days.join(", "));
        }
        if let Some(c) = self.count {
            s.push_str(&format!(", {c} times"));
        }
        s
    }

    /// Expands the rule from `start` (local wall time) and returns occurrences
    /// `<= window_end`, in order, at most `max` items. `to_utc` converts local
    /// times so a UTC `UNTIL` can be compared correctly.
    pub fn expand_local(
        &self,
        start: NaiveDateTime,
        window_end: NaiveDateTime,
        max: usize,
        to_utc: &dyn Fn(NaiveDateTime) -> DateTime<Utc>,
    ) -> Vec<NaiveDateTime> {
        let mut out = Vec::new();
        let mut produced: u32 = 0;
        let time = start.time();
        let within_until = |cand: NaiveDateTime| match &self.until {
            None => true,
            Some(Until::Date(d)) => cand.date() <= *d,
            Some(Until::Local(l)) => cand <= *l,
            Some(Until::Utc(u)) => to_utc(cand) <= *u,
        };

        // DTSTART is always the first instance.
        if start <= window_end && within_until(start) {
            out.push(start);
        }
        produced += 1;
        if self.count == Some(1) || out.len() >= max {
            return out;
        }

        for k in 0..MAX_PERIODS {
            let dates = self.period_dates(start.date(), k as i64);
            let Some(dates) = dates else { continue };
            for d in dates {
                let cand = d.and_time(time);
                if cand <= start {
                    continue;
                }
                if cand > window_end || !within_until(cand) {
                    return out;
                }
                produced += 1;
                out.push(cand);
                if out.len() >= max {
                    return out;
                }
                if let Some(c) = self.count {
                    if produced >= c {
                        return out;
                    }
                }
            }
            // Stop when the period itself is entirely past the window.
            if self.period_start(start.date(), k as i64).map(|p| p.and_time(time) > window_end) == Some(true) {
                return out;
            }
        }
        out
    }

    fn period_start(&self, start: NaiveDate, k: i64) -> Option<NaiveDate> {
        let step = k * self.interval as i64;
        match self.freq {
            Freq::Daily => start.checked_add_signed(Duration::days(step)),
            Freq::Weekly => week_start(start, self.week_start).checked_add_signed(Duration::weeks(step)),
            Freq::Monthly => {
                let (y, m) = add_months(start.year(), start.month(), step)?;
                NaiveDate::from_ymd_opt(y, m, 1)
            }
            Freq::Yearly => NaiveDate::from_ymd_opt(start.year().checked_add(step as i32)?, 1, 1),
        }
    }

    /// Candidate dates for period `k`, sorted. `None` means the period is out of range.
    fn period_dates(&self, start: NaiveDate, k: i64) -> Option<Vec<NaiveDate>> {
        let step = k * self.interval as i64;
        let mut dates: Vec<NaiveDate> = match self.freq {
            Freq::Daily => {
                let d = start.checked_add_signed(Duration::days(step))?;
                let ok_day = self.by_day.is_empty() || self.by_day.iter().any(|w| w.weekday == d.weekday());
                let ok_md = self.by_month_day.is_empty() || month_day_matches(d, &self.by_month_day);
                if ok_day && ok_md {
                    vec![d]
                } else {
                    vec![]
                }
            }
            Freq::Weekly => {
                let ws = week_start(start, self.week_start).checked_add_signed(Duration::weeks(step))?;
                let wanted: Vec<Weekday> =
                    if self.by_day.is_empty() { vec![start.weekday()] } else { self.by_day.iter().map(|w| w.weekday).collect() };
                (0..7).filter_map(|i| ws.checked_add_signed(Duration::days(i))).filter(|d| wanted.contains(&d.weekday())).collect()
            }
            Freq::Monthly => {
                let (y, m) = add_months(start.year(), start.month(), step)?;
                self.month_dates(y, m, start.day())
            }
            Freq::Yearly => {
                let y = start.year().checked_add(step as i32)?;
                if self.by_month.is_empty() && self.by_month_day.is_empty() && !self.by_day.is_empty() {
                    year_weekday_dates(y, &self.by_day)
                } else {
                    let months = if self.by_month.is_empty() { vec![start.month()] } else { self.by_month.clone() };
                    let mut all = Vec::new();
                    for m in months {
                        all.extend(self.month_dates(y, m, start.day()));
                    }
                    all
                }
            }
        };
        if !self.by_month.is_empty() {
            dates.retain(|d| self.by_month.contains(&d.month()));
        }
        dates.sort_unstable();
        dates.dedup();
        Some(dates)
    }

    fn month_dates(&self, y: i32, m: u32, start_day: u32) -> Vec<NaiveDate> {
        let len = days_in_month(y, m);
        if !self.by_month_day.is_empty() {
            let mut v: Vec<NaiveDate> = self
                .by_month_day
                .iter()
                .filter_map(|&d| {
                    let day = if d > 0 { d } else { len as i32 + d + 1 };
                    if day < 1 || day > len as i32 {
                        None
                    } else {
                        NaiveDate::from_ymd_opt(y, m, day as u32)
                    }
                })
                .collect();
            if !self.by_day.is_empty() {
                v.retain(|d| self.by_day.iter().any(|w| w.weekday == d.weekday()));
            }
            return v;
        }
        if !self.by_day.is_empty() {
            let mut v = Vec::new();
            for wd in &self.by_day {
                let all: Vec<NaiveDate> =
                    (1..=len).filter_map(|d| NaiveDate::from_ymd_opt(y, m, d)).filter(|d| d.weekday() == wd.weekday).collect();
                match wd.ordinal {
                    None => v.extend(all),
                    Some(n) if n > 0 => v.extend(all.get((n - 1) as usize).copied()),
                    Some(n) => {
                        let idx = all.len() as i32 + n;
                        if idx >= 0 {
                            v.extend(all.get(idx as usize).copied());
                        }
                    }
                }
            }
            return v;
        }
        NaiveDate::from_ymd_opt(y, m, start_day).into_iter().collect()
    }

    /// Adds one "interval" to a moment; used by after-completion repetition.
    pub fn advance_from(&self, from: NaiveDateTime) -> Option<NaiveDateTime> {
        let n = self.interval as i64;
        match self.freq {
            Freq::Daily => from.checked_add_signed(Duration::days(n)),
            Freq::Weekly => from.checked_add_signed(Duration::weeks(n)),
            Freq::Monthly => {
                let (y, m) = add_months(from.year(), from.month(), n)?;
                let d = from.day().min(days_in_month(y, m));
                Some(NaiveDate::from_ymd_opt(y, m, d)?.and_time(from.time()))
            }
            Freq::Yearly => {
                let y = from.year() + n as i32;
                let d = from.day().min(days_in_month(y, from.month()));
                Some(NaiveDate::from_ymd_opt(y, from.month(), d)?.and_time(from.time()))
            }
        }
    }

    /// First occurrence strictly after `after`, following the fixed schedule.
    pub fn next_after(
        &self,
        start: NaiveDateTime,
        after: NaiveDateTime,
        to_utc: &dyn Fn(NaiveDateTime) -> DateTime<Utc>,
    ) -> Option<NaiveDateTime> {
        // Expand in growing windows so long-running rules stay cheap.
        let mut horizon = after + Duration::days(400);
        for _ in 0..4 {
            let occ = self.expand_local(start, horizon, 20_000, to_utc);
            if let Some(n) = occ.into_iter().find(|o| *o > after) {
                return Some(n);
            }
            horizon += Duration::days(3650);
        }
        None
    }
}

fn ordinal_word(n: i32) -> String {
    match n {
        1 => "1st".into(),
        2 => "2nd".into(),
        3 => "3rd".into(),
        n => format!("{n}th"),
    }
}

impl fmt::Display for RRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let freq = match self.freq {
            Freq::Daily => "DAILY",
            Freq::Weekly => "WEEKLY",
            Freq::Monthly => "MONTHLY",
            Freq::Yearly => "YEARLY",
        };
        write!(f, "FREQ={freq}")?;
        if self.interval > 1 {
            write!(f, ";INTERVAL={}", self.interval)?;
        }
        if let Some(c) = self.count {
            write!(f, ";COUNT={c}")?;
        }
        match &self.until {
            Some(Until::Date(d)) => write!(f, ";UNTIL={}", d.format("%Y%m%d"))?,
            Some(Until::Local(l)) => write!(f, ";UNTIL={}", l.format("%Y%m%dT%H%M%S"))?,
            Some(Until::Utc(u)) => write!(f, ";UNTIL={}", u.format("%Y%m%dT%H%M%SZ"))?,
            None => {}
        }
        if !self.by_day.is_empty() {
            let parts: Vec<String> = self
                .by_day
                .iter()
                .map(|d| match d.ordinal {
                    Some(n) => format!("{n}{}", weekday_code(d.weekday)),
                    None => weekday_code(d.weekday).to_string(),
                })
                .collect();
            write!(f, ";BYDAY={}", parts.join(","))?;
        }
        if !self.by_month_day.is_empty() {
            let parts: Vec<String> = self.by_month_day.iter().map(|d| d.to_string()).collect();
            write!(f, ";BYMONTHDAY={}", parts.join(","))?;
        }
        if !self.by_month.is_empty() {
            let parts: Vec<String> = self.by_month.iter().map(|d| d.to_string()).collect();
            write!(f, ";BYMONTH={}", parts.join(","))?;
        }
        if self.week_start != Weekday::Mon {
            write!(f, ";WKST={}", weekday_code(self.week_start))?;
        }
        Ok(())
    }
}

fn month_day_matches(d: NaiveDate, by: &[i32]) -> bool {
    let len = days_in_month(d.year(), d.month()) as i32;
    by.iter().any(|&n| {
        let day = if n > 0 { n } else { len + n + 1 };
        day == d.day() as i32
    })
}

fn year_weekday_dates(y: i32, by_day: &[WeekdayNum]) -> Vec<NaiveDate> {
    let mut v = Vec::new();
    let Some(first) = NaiveDate::from_ymd_opt(y, 1, 1) else { return v };
    let Some(last) = NaiveDate::from_ymd_opt(y, 12, 31) else { return v };
    for wd in by_day {
        let all: Vec<NaiveDate> = first.iter_days().take_while(|d| *d <= last).filter(|d| d.weekday() == wd.weekday).collect();
        match wd.ordinal {
            None => v.extend(all),
            Some(n) if n > 0 => v.extend(all.get((n - 1) as usize).copied()),
            Some(n) => {
                let idx = all.len() as i32 + n;
                if idx >= 0 {
                    v.extend(all.get(idx as usize).copied());
                }
            }
        }
    }
    v
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    match (NaiveDate::from_ymd_opt(ny, nm, 1), NaiveDate::from_ymd_opt(y, m, 1)) {
        (Some(a), Some(b)) => (a - b).num_days() as u32,
        _ => 30,
    }
}

fn add_months(y: i32, m: u32, n: i64) -> Option<(i32, u32)> {
    let total = y as i64 * 12 + (m as i64 - 1) + n;
    let ny = total.div_euclid(12);
    let nm = total.rem_euclid(12) + 1;
    if !(-9999..=9999).contains(&ny) {
        return None;
    }
    Some((ny as i32, nm as u32))
}

fn week_start(d: NaiveDate, wkst: Weekday) -> NaiveDate {
    let offset = (7 + d.weekday().num_days_from_monday() as i64 - wkst.num_days_from_monday() as i64) % 7;
    d - Duration::days(offset)
}

/// Converts a local wall time in `tz` to UTC following RFC 5545: ambiguous
/// times use the first (earlier) occurrence; times inside a spring-forward gap
/// are interpreted with the offset in effect before the gap.
pub fn resolve_local(tz: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(a, b) => a.min(b).with_timezone(&Utc),
        LocalResult::None => {
            let before = local - Duration::hours(4);
            let offset = match tz.from_local_datetime(&before) {
                LocalResult::Single(dt) => dt.offset().fix(),
                LocalResult::Ambiguous(a, _) => a.offset().fix(),
                LocalResult::None => Utc.fix(),
            };
            let secs = offset.local_minus_utc() as i64;
            Utc.from_utc_datetime(&(local - Duration::seconds(secs)))
        }
    }
}

/// Parses an IANA zone name, falling back to UTC.
pub fn tz_or_utc(name: Option<&str>) -> Tz {
    name.and_then(|n| n.parse::<Tz>().ok()).unwrap_or(Tz::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn utc_conv(n: NaiveDateTime) -> DateTime<Utc> {
        Utc.from_utc_datetime(&n)
    }

    #[test]
    fn parses_and_serializes() {
        let r = RRule::parse("RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH;COUNT=6").unwrap();
        assert_eq!(r.freq, Freq::Weekly);
        assert_eq!(r.interval, 2);
        assert_eq!(r.to_string(), "FREQ=WEEKLY;INTERVAL=2;COUNT=6;BYDAY=MO,TH");
        assert_eq!(r.describe(), "Every 2 weeks on Mon, Thu, 6 times");
        let r = RRule::parse("FREQ=MONTHLY;BYDAY=-1FR").unwrap();
        assert_eq!(r.to_string(), "FREQ=MONTHLY;BYDAY=-1FR");
    }

    #[test]
    fn rejects_unsupported_and_invalid() {
        assert!(matches!(RRule::parse("FREQ=MONTHLY;BYSETPOS=-1;BYDAY=MO,TU"), Err(RRuleError::Unsupported(_))));
        assert!(matches!(RRule::parse("FREQ=HOURLY"), Err(RRuleError::Unsupported(_))));
        assert_eq!(RRule::parse("INTERVAL=2"), Err(RRuleError::MissingFreq));
        assert!(RRule::parse("FREQ=DAILY;COUNT=2;UNTIL=20260101").is_err());
        assert!(RRule::parse("FREQ=DAILY;INTERVAL=0").is_err());
        assert!(RRule::parse("FREQ=WEEKLY;BYDAY=2MO").is_err());
    }

    #[test]
    fn weekly_byday_with_count() {
        let r = RRule::parse("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=5").unwrap();
        // 2026-10-05 is a Monday.
        let occ = r.expand_local(dt("2026-10-05 09:00"), dt("2027-01-01 00:00"), 100, &utc_conv);
        let got: Vec<String> = occ.iter().map(|d| d.format("%m-%d").to_string()).collect();
        assert_eq!(got, vec!["10-05", "10-07", "10-12", "10-14", "10-19"]);
    }

    #[test]
    fn monthly_last_friday_and_negative_monthday() {
        let r = RRule::parse("FREQ=MONTHLY;BYDAY=-1FR;COUNT=3").unwrap();
        let occ = r.expand_local(dt("2026-01-30 18:00"), dt("2027-01-01 00:00"), 100, &utc_conv);
        let got: Vec<String> = occ.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect();
        assert_eq!(got, vec!["2026-01-30", "2026-02-27", "2026-03-27"]);

        let r = RRule::parse("FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=3").unwrap();
        let occ = r.expand_local(dt("2026-01-31 08:00"), dt("2027-01-01 00:00"), 100, &utc_conv);
        let got: Vec<String> = occ.iter().map(|d| d.format("%m-%d").to_string()).collect();
        assert_eq!(got, vec!["01-31", "02-28", "03-31"]);
    }

    #[test]
    fn monthly_on_31st_skips_short_months() {
        let r = RRule::parse("FREQ=MONTHLY;COUNT=4").unwrap();
        let occ = r.expand_local(dt("2026-01-31 08:00"), dt("2027-12-01 00:00"), 100, &utc_conv);
        let got: Vec<String> = occ.iter().map(|d| d.format("%m-%d").to_string()).collect();
        assert_eq!(got, vec!["01-31", "03-31", "05-31", "07-31"]);
    }

    #[test]
    fn yearly_and_until() {
        let r = RRule::parse("FREQ=YEARLY;UNTIL=20290101").unwrap();
        let occ = r.expand_local(dt("2026-02-14 00:00"), dt("2035-01-01 00:00"), 100, &utc_conv);
        assert_eq!(occ.len(), 3);
        let r = RRule::parse("FREQ=DAILY;UNTIL=20261003T090000Z").unwrap();
        let occ = r.expand_local(dt("2026-10-01 09:00"), dt("2030-01-01 00:00"), 100, &utc_conv);
        assert_eq!(occ.len(), 3);
    }

    #[test]
    fn dst_keeps_wall_clock_time() {
        let tz: Tz = "Europe/Stockholm".parse().unwrap();
        let r = RRule::parse("FREQ=WEEKLY;COUNT=3").unwrap();
        // Spans the 2026-10-25 fall-back transition.
        let occ = r.expand_local(dt("2026-10-18 09:00"), dt("2027-01-01 00:00"), 10, &|n| resolve_local(tz, n));
        let utc: Vec<String> = occ.iter().map(|n| resolve_local(tz, *n).format("%m-%d %H:%M").to_string()).collect();
        assert_eq!(utc, vec!["10-18 07:00", "10-25 08:00", "11-01 08:00"]);
    }

    #[test]
    fn gap_and_ambiguous_resolution() {
        let tz: Tz = "America/New_York".parse().unwrap();
        // 2026-03-08 02:30 does not exist in New York; RFC 5545 interprets it with the pre-gap offset (-05:00).
        let gap = resolve_local(tz, dt("2026-03-08 02:30"));
        assert_eq!(gap.format("%H:%M").to_string(), "07:30");
        // 2026-11-01 01:30 happens twice; the first (EDT, -04:00) wins.
        let amb = resolve_local(tz, dt("2026-11-01 01:30"));
        assert_eq!(amb.format("%H:%M").to_string(), "05:30");
    }

    #[test]
    fn next_after_and_advance() {
        let r = RRule::parse("FREQ=WEEKLY;BYDAY=SA").unwrap();
        let n = r.next_after(dt("2026-10-03 10:00"), dt("2026-10-03 12:00"), &utc_conv).unwrap();
        assert_eq!(n, dt("2026-10-10 10:00"));
        let r = RRule::parse("FREQ=MONTHLY").unwrap();
        assert_eq!(r.advance_from(dt("2026-01-31 10:00")).unwrap(), dt("2026-02-28 10:00"));
    }

    #[test]
    fn hostile_rules_are_bounded() {
        let r = RRule::parse("FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30").unwrap();
        let occ = r.expand_local(dt("2026-01-01 00:00"), dt("9000-01-01 00:00"), 100, &utc_conv);
        assert_eq!(occ.len(), 1); // only DTSTART; Feb 30 never exists
    }
}
