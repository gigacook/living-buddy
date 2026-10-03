//! Cooperative reminders ("nudges") between group members.
//!
//! Nudges are opt-in for the recipient, respect quiet hours by deferring
//! delivery, and are rate limited so they cannot become spam or pressure.

use chrono::{DateTime, Duration, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;

pub const PER_TASK_COOLDOWN_HOURS: i64 = 6;
pub const PER_PAIR_DAILY_LIMIT: usize = 5;

#[derive(Clone, Debug)]
pub struct RecipientPrefs {
    pub accepts_nudges: bool,
    pub quiet_start: Option<NaiveTime>,
    pub quiet_end: Option<NaiveTime>,
    pub tz: Tz,
}

#[derive(Clone, Debug)]
pub struct SentNudge {
    pub from: String,
    pub to: String,
    pub task_id: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NudgeDenied {
    #[error("They have not turned on reminders from others.")]
    NotAccepted,
    #[error("You can't send a reminder to yourself here — try an alarm instead.")]
    SelfNudge,
    #[error("A reminder for this task was sent recently. Let's give it some time.")]
    TaskCooldown,
    #[error("That's plenty of reminders for today.")]
    DailyLimit,
}

/// Returns when the nudge should be delivered (now, or the end of quiet hours).
pub fn plan_nudge(
    history: &[SentNudge],
    from: &str,
    to: &str,
    task_id: Option<&str>,
    prefs: &RecipientPrefs,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, NudgeDenied> {
    if from == to {
        return Err(NudgeDenied::SelfNudge);
    }
    if !prefs.accepts_nudges {
        return Err(NudgeDenied::NotAccepted);
    }
    let day_ago = now - Duration::hours(24);
    let pair: Vec<&SentNudge> = history.iter().filter(|n| n.from == from && n.to == to && n.at > day_ago).collect();
    if let Some(t) = task_id {
        let cooldown = now - Duration::hours(PER_TASK_COOLDOWN_HOURS);
        if pair.iter().any(|n| n.task_id.as_deref() == Some(t) && n.at > cooldown) {
            return Err(NudgeDenied::TaskCooldown);
        }
    }
    if pair.len() >= PER_PAIR_DAILY_LIMIT {
        return Err(NudgeDenied::DailyLimit);
    }
    Ok(quiet_hours_release(prefs, now))
}

/// If `now` falls within quiet hours, returns the end of quiet hours; otherwise `now`.
pub fn quiet_hours_release(prefs: &RecipientPrefs, now: DateTime<Utc>) -> DateTime<Utc> {
    let (Some(start), Some(end)) = (prefs.quiet_start, prefs.quiet_end) else { return now };
    if start == end {
        return now;
    }
    let local = now.with_timezone(&prefs.tz);
    let t = local.time().with_nanosecond(0).unwrap_or(local.time());
    let inside = if start < end { t >= start && t < end } else { t >= start || t < end };
    if !inside {
        return now;
    }
    let mut date = local.date_naive();
    if start > end && t >= start {
        date += Duration::days(1);
    }
    crate::recurrence::resolve_local(prefs.tz, date.and_time(end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn prefs() -> RecipientPrefs {
        RecipientPrefs {
            accepts_nudges: true,
            quiet_start: NaiveTime::from_hms_opt(22, 0, 0),
            quiet_end: NaiveTime::from_hms_opt(7, 0, 0),
            tz: Tz::UTC,
        }
    }

    #[test]
    fn opt_in_required_and_no_self() {
        let now = Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap();
        let mut p = prefs();
        p.accepts_nudges = false;
        assert_eq!(plan_nudge(&[], "a", "b", None, &p, now), Err(NudgeDenied::NotAccepted));
        assert_eq!(plan_nudge(&[], "a", "a", None, &prefs(), now), Err(NudgeDenied::SelfNudge));
        assert_eq!(plan_nudge(&[], "a", "b", None, &prefs(), now), Ok(now));
    }

    #[test]
    fn cooldown_and_daily_limit() {
        let now = Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap();
        let h = vec![SentNudge { from: "a".into(), to: "b".into(), task_id: Some("t".into()), at: now - Duration::hours(1) }];
        assert_eq!(plan_nudge(&h, "a", "b", Some("t"), &prefs(), now), Err(NudgeDenied::TaskCooldown));
        assert!(plan_nudge(&h, "a", "b", Some("other"), &prefs(), now).is_ok());
        let many: Vec<SentNudge> = (0..5)
            .map(|i| SentNudge { from: "a".into(), to: "b".into(), task_id: Some(format!("t{i}")), at: now - Duration::hours(i + 1) })
            .collect();
        assert_eq!(plan_nudge(&many, "a", "b", None, &prefs(), now), Err(NudgeDenied::DailyLimit));
    }

    #[test]
    fn quiet_hours_defer() {
        let late = Utc.with_ymd_and_hms(2026, 10, 3, 23, 0, 0).unwrap();
        assert_eq!(quiet_hours_release(&prefs(), late), Utc.with_ymd_and_hms(2026, 10, 4, 7, 0, 0).unwrap());
        let early = Utc.with_ymd_and_hms(2026, 10, 4, 5, 0, 0).unwrap();
        assert_eq!(quiet_hours_release(&prefs(), early), Utc.with_ymd_and_hms(2026, 10, 4, 7, 0, 0).unwrap());
        let day = Utc.with_ymd_and_hms(2026, 10, 4, 13, 0, 0).unwrap();
        assert_eq!(quiet_hours_release(&prefs(), day), day);
    }
}
