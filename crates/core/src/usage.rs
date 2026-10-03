//! Manual Claude usage-limit tracker.
//!
//! There is no official consumer quota API; values are typed in by the person
//! from what Claude's own settings page shows. Everything here is an estimate
//! and is labelled as such. This tracker is separate from any AI metering the
//! application itself does.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const FIVE_HOUR_CHECKPOINTS: &[u8] = &[25, 50, 75, 100];
pub const SEVEN_DAY_CHECKPOINTS: &[u8] = &[50, 100];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WindowState {
    /// No value entered yet.
    Unknown,
    /// Value entered and the window has not reset yet.
    Current,
    /// The reset time has passed; the stored value is out of date.
    ResetPassed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindowSummary {
    pub percent: Option<f32>,
    pub resets_at: Option<DateTime<Utc>>,
    pub state: WindowState,
    /// Checkpoints at or below the entered percentage.
    pub reached: Vec<u8>,
    /// The next checkpoint above the entered percentage.
    pub next_checkpoint: Option<u8>,
}

pub fn summarize(percent: Option<f32>, resets_at: Option<DateTime<Utc>>, checkpoints: &[u8], now: DateTime<Utc>) -> WindowSummary {
    let state = match (percent, resets_at) {
        (None, _) => WindowState::Unknown,
        (Some(_), Some(r)) if r <= now => WindowState::ResetPassed,
        _ => WindowState::Current,
    };
    let p = if state == WindowState::Current { percent.unwrap_or(0.0) } else { 0.0 };
    WindowSummary {
        percent,
        resets_at,
        state,
        reached: checkpoints.iter().copied().filter(|c| p >= *c as f32).collect(),
        next_checkpoint: checkpoints.iter().copied().find(|c| p < *c as f32),
    }
}

pub fn validate_percent(p: Option<f32>) -> Result<(), &'static str> {
    match p {
        Some(v) if !(0.0..=100.0).contains(&v) || v.is_nan() => Err("Enter a percentage between 0 and 100."),
        _ => Ok(()),
    }
}

/// Threshold crossings between an old and a new value, used for optional reminders.
pub fn crossed(old: Option<f32>, new: Option<f32>, thresholds: &[u8]) -> Vec<u8> {
    let (o, n) = (old.unwrap_or(0.0), new.unwrap_or(0.0));
    thresholds.iter().copied().filter(|t| o < *t as f32 && n >= *t as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn summaries() {
        let now = Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap();
        let s = summarize(Some(60.0), Some(now + Duration::hours(2)), FIVE_HOUR_CHECKPOINTS, now);
        assert_eq!(s.state, WindowState::Current);
        assert_eq!(s.reached, vec![25, 50]);
        assert_eq!(s.next_checkpoint, Some(75));
        let stale = summarize(Some(90.0), Some(now - Duration::minutes(1)), FIVE_HOUR_CHECKPOINTS, now);
        assert_eq!(stale.state, WindowState::ResetPassed);
        assert!(stale.reached.is_empty());
        assert_eq!(summarize(None, None, SEVEN_DAY_CHECKPOINTS, now).state, WindowState::Unknown);
    }

    #[test]
    fn crossings_and_validation() {
        assert_eq!(crossed(Some(40.0), Some(80.0), FIVE_HOUR_CHECKPOINTS), vec![50, 75]);
        assert!(crossed(Some(80.0), Some(70.0), FIVE_HOUR_CHECKPOINTS).is_empty());
        assert!(validate_percent(Some(101.0)).is_err());
        assert!(validate_percent(Some(f32::NAN)).is_err());
        assert!(validate_percent(None).is_ok());
    }
}
