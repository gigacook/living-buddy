//! Focus / Pomodoro / countdown timer state machine.
//!
//! Timers are persisted as absolute deadlines (`ends_at`), never as "ticks
//! remaining", so reloading the page, sleeping the laptop or suspending the app
//! cannot make a timer drift. Every operation takes `now` explicitly.
//!
//! After a long absence the timer does not silently chain through many phases:
//! if a phase ended more than [`AUTO_ADVANCE_GRACE_SECS`] ago it waits in
//! `phase_complete` so the person can choose what to do next.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const AUTO_ADVANCE_GRACE_SECS: i64 = 90;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TimerKind {
    Focus,
    Pomodoro,
    Countdown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TimerStatus {
    Idle,
    Running,
    Paused,
    /// A phase ended; waiting for the person to start the next one.
    PhaseComplete,
    /// A focus or countdown timer reached zero.
    Finished,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroConfig {
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub cycles_before_long_break: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        PomodoroConfig {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            cycles_before_long_break: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
        }
    }
}

impl PomodoroConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        let ok = |m: u32| (1..=240).contains(&m);
        if !ok(self.focus_minutes) || !ok(self.short_break_minutes) || !ok(self.long_break_minutes) {
            return Err("Durations must be between 1 and 240 minutes.");
        }
        if !(1..=12).contains(&self.cycles_before_long_break) {
            return Err("Cycles before a long break must be between 1 and 12.");
        }
        Ok(())
    }

    fn phase_ms(&self, phase: Phase) -> i64 {
        let m = match phase {
            Phase::Focus => self.focus_minutes,
            Phase::ShortBreak => self.short_break_minutes,
            Phase::LongBreak => self.long_break_minutes,
        };
        m as i64 * 60_000
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TimerState {
    pub kind: TimerKind,
    pub status: TimerStatus,
    pub phase: Phase,
    /// Completed focus phases in the current Pomodoro set.
    pub completed_focus: u32,
    /// Total length of the current phase in milliseconds.
    #[ts(type = "number")]
    pub duration_ms: i64,
    /// Absolute deadline while running.
    pub ends_at: Option<DateTime<Utc>>,
    /// Remaining time while paused.
    #[ts(type = "number | null")]
    pub remaining_ms: Option<i64>,
    /// When the most recent phase ended (for "you were away" messaging and de-duplicated notifications).
    pub phase_ended_at: Option<DateTime<Utc>>,
    pub label: Option<String>,
    pub task_id: Option<String>,
    pub config: PomodoroConfig,
}

impl Default for TimerState {
    fn default() -> Self {
        TimerState {
            kind: TimerKind::Focus,
            status: TimerStatus::Idle,
            phase: Phase::Focus,
            completed_focus: 0,
            duration_ms: 25 * 60_000,
            ends_at: None,
            remaining_ms: None,
            phase_ended_at: None,
            label: None,
            task_id: None,
            config: PomodoroConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimerError {
    #[error("The timer is not running.")]
    NotRunning,
    #[error("The timer is not paused.")]
    NotPaused,
    #[error("{0}")]
    Invalid(&'static str),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(tag = "action", rename_all = "snake_case", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum TimerCommand {
    Start {
        kind: TimerKind,
        #[ts(type = "number | null")]
        minutes: Option<u32>,
        label: Option<String>,
        task_id: Option<String>,
        config: Option<PomodoroConfig>,
    },
    Pause,
    Resume,
    Reset,
    /// Ends the current phase now (e.g. skip a break) and moves to the next.
    Skip,
    /// Starts the next phase after `phase_complete`.
    Continue,
    /// Adds minutes to a running or paused timer.
    Extend { minutes: u32 },
}

impl TimerState {
    pub fn remaining_ms(&self, now: DateTime<Utc>) -> i64 {
        match self.status {
            TimerStatus::Running => self.ends_at.map(|e| (e - now).num_milliseconds().max(0)).unwrap_or(0),
            TimerStatus::Paused => self.remaining_ms.unwrap_or(0),
            TimerStatus::Idle => self.duration_ms,
            _ => 0,
        }
    }

    fn next_phase(&self) -> Phase {
        match self.phase {
            Phase::Focus => {
                let done = self.completed_focus + 1;
                if done % self.config.cycles_before_long_break.max(1) == 0 {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                }
            }
            _ => Phase::Focus,
        }
    }

    fn begin_phase(&mut self, phase: Phase, at: DateTime<Utc>) {
        self.phase = phase;
        self.duration_ms = self.config.phase_ms(phase);
        self.ends_at = Some(at + Duration::milliseconds(self.duration_ms));
        self.remaining_ms = None;
        self.status = TimerStatus::Running;
    }

    /// Ends the current phase at `ended_at`, deciding what follows.
    fn finish_phase(&mut self, ended_at: DateTime<Utc>, now: DateTime<Utc>) {
        self.phase_ended_at = Some(ended_at);
        self.ends_at = None;
        self.remaining_ms = None;
        if self.kind != TimerKind::Pomodoro {
            self.status = TimerStatus::Finished;
            return;
        }
        let next = self.next_phase();
        if self.phase == Phase::Focus {
            self.completed_focus += 1;
        }
        let auto = match next {
            Phase::Focus => self.config.auto_start_focus,
            _ => self.config.auto_start_breaks,
        };
        let recent = (now - ended_at).num_seconds() <= AUTO_ADVANCE_GRACE_SECS;
        if auto && recent {
            // Start from the original deadline so phases stay aligned.
            self.begin_phase(next, ended_at);
            if self.ends_at.map(|e| e <= now).unwrap_or(false) {
                self.begin_phase(next, now);
            }
        } else {
            self.phase = next;
            self.duration_ms = self.config.phase_ms(next);
            self.status = TimerStatus::PhaseComplete;
        }
    }

    /// Brings the state up to date with the clock. Returns true if a phase ended.
    pub fn reconcile(&mut self, now: DateTime<Utc>) -> bool {
        if self.status != TimerStatus::Running {
            return false;
        }
        match self.ends_at {
            Some(end) if end <= now => {
                self.finish_phase(end, now);
                true
            }
            _ => false,
        }
    }

    pub fn apply(&mut self, cmd: TimerCommand, now: DateTime<Utc>) -> Result<(), TimerError> {
        self.reconcile(now);
        match cmd {
            TimerCommand::Start { kind, minutes, label, task_id, config } => {
                let config = config.unwrap_or_else(|| self.config.clone());
                config.validate().map_err(TimerError::Invalid)?;
                let mut next = TimerState { kind, label, task_id, config, ..TimerState::default() };
                match kind {
                    TimerKind::Pomodoro => next.begin_phase(Phase::Focus, now),
                    TimerKind::Focus | TimerKind::Countdown => {
                        let m = minutes.unwrap_or(next.config.focus_minutes);
                        if !(1..=24 * 60).contains(&m) {
                            return Err(TimerError::Invalid("Choose between 1 minute and 24 hours."));
                        }
                        next.duration_ms = m as i64 * 60_000;
                        next.ends_at = Some(now + Duration::milliseconds(next.duration_ms));
                        next.status = TimerStatus::Running;
                    }
                }
                *self = next;
            }
            TimerCommand::Pause => {
                if self.status != TimerStatus::Running {
                    return Err(TimerError::NotRunning);
                }
                self.remaining_ms = Some(self.remaining_ms(now));
                self.ends_at = None;
                self.status = TimerStatus::Paused;
            }
            TimerCommand::Resume => {
                if self.status != TimerStatus::Paused {
                    return Err(TimerError::NotPaused);
                }
                let rem = self.remaining_ms.unwrap_or(0);
                self.ends_at = Some(now + Duration::milliseconds(rem));
                self.remaining_ms = None;
                self.status = TimerStatus::Running;
            }
            TimerCommand::Reset => {
                let keep = (self.kind, self.config.clone(), self.label.clone(), self.task_id.clone());
                *self = TimerState { kind: keep.0, config: keep.1, label: keep.2, task_id: keep.3, ..TimerState::default() };
                self.duration_ms = self.config.phase_ms(Phase::Focus);
            }
            TimerCommand::Skip => match self.status {
                TimerStatus::Running | TimerStatus::Paused => {
                    if self.kind == TimerKind::Pomodoro {
                        // Skipping never auto-chains: land in phase_complete or start the next phase now.
                        let next = self.next_phase();
                        if self.phase == Phase::Focus {
                            self.completed_focus += 1;
                        }
                        self.phase_ended_at = Some(now);
                        self.begin_phase(next, now);
                    } else {
                        self.finish_phase(now, now);
                    }
                }
                TimerStatus::PhaseComplete => {
                    // Skip the pending phase (e.g. skip the break) and start the one after it.
                    if self.phase != Phase::Focus {
                        self.begin_phase(Phase::Focus, now);
                    } else {
                        let next = self.next_phase();
                        self.completed_focus += 1;
                        self.begin_phase(next, now);
                    }
                }
                _ => return Err(TimerError::NotRunning),
            },
            TimerCommand::Continue => {
                if self.status != TimerStatus::PhaseComplete {
                    return Err(TimerError::Invalid("There is no phase waiting to start."));
                }
                let p = self.phase;
                self.begin_phase(p, now);
            }
            TimerCommand::Extend { minutes } => {
                if !(1..=240).contains(&minutes) {
                    return Err(TimerError::Invalid("Extend by 1 to 240 minutes."));
                }
                let add = minutes as i64 * 60_000;
                match self.status {
                    TimerStatus::Running => {
                        self.ends_at = self.ends_at.map(|e| e + Duration::milliseconds(add));
                        self.duration_ms += add;
                    }
                    TimerStatus::Paused => {
                        self.remaining_ms = self.remaining_ms.map(|r| r + add);
                        self.duration_ms += add;
                    }
                    TimerStatus::Finished => {
                        self.duration_ms = add;
                        self.ends_at = Some(now + Duration::milliseconds(add));
                        self.status = TimerStatus::Running;
                    }
                    _ => return Err(TimerError::NotRunning),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 3, 9, 0, 0).unwrap()
    }

    fn start(kind: TimerKind, minutes: Option<u32>) -> TimerState {
        let mut s = TimerState::default();
        s.apply(TimerCommand::Start { kind, minutes, label: None, task_id: None, config: None }, t0()).unwrap();
        s
    }

    #[test]
    fn focus_timer_survives_sleep() {
        let mut s = start(TimerKind::Focus, Some(10));
        assert_eq!(s.remaining_ms(t0() + Duration::minutes(4)), 6 * 60_000);
        // Laptop sleeps for an hour; on wake the deadline is still authoritative.
        let wake = t0() + Duration::hours(1);
        assert!(s.reconcile(wake));
        assert_eq!(s.status, TimerStatus::Finished);
        assert_eq!(s.phase_ended_at, Some(t0() + Duration::minutes(10)));
        // Reconciling again does not re-fire.
        assert!(!s.reconcile(wake));
    }

    #[test]
    fn pause_resume_preserves_remaining() {
        let mut s = start(TimerKind::Countdown, Some(5));
        s.apply(TimerCommand::Pause, t0() + Duration::minutes(2)).unwrap();
        assert_eq!(s.remaining_ms(t0() + Duration::hours(3)), 3 * 60_000);
        s.apply(TimerCommand::Resume, t0() + Duration::hours(3)).unwrap();
        assert_eq!(s.ends_at, Some(t0() + Duration::hours(3) + Duration::minutes(3)));
        assert_eq!(s.apply(TimerCommand::Resume, t0()), Err(TimerError::NotPaused));
    }

    #[test]
    fn pomodoro_auto_starts_break_when_present() {
        let mut s = start(TimerKind::Pomodoro, None);
        let end = t0() + Duration::minutes(25);
        assert!(s.reconcile(end + Duration::seconds(5)));
        assert_eq!(s.status, TimerStatus::Running);
        assert_eq!(s.phase, Phase::ShortBreak);
        assert_eq!(s.ends_at, Some(end + Duration::minutes(5)));
        assert_eq!(s.completed_focus, 1);
    }

    #[test]
    fn pomodoro_waits_after_long_absence() {
        let mut s = start(TimerKind::Pomodoro, None);
        assert!(s.reconcile(t0() + Duration::hours(2)));
        assert_eq!(s.status, TimerStatus::PhaseComplete);
        assert_eq!(s.phase, Phase::ShortBreak);
        // Skip the break and go straight back to focus.
        s.apply(TimerCommand::Skip, t0() + Duration::hours(2)).unwrap();
        assert_eq!(s.phase, Phase::Focus);
        assert_eq!(s.status, TimerStatus::Running);
    }

    #[test]
    fn long_break_after_configured_cycles() {
        let cfg = PomodoroConfig { cycles_before_long_break: 2, auto_start_breaks: false, ..PomodoroConfig::default() };
        let mut s = TimerState::default();
        s.apply(TimerCommand::Start { kind: TimerKind::Pomodoro, minutes: None, label: None, task_id: None, config: Some(cfg) }, t0()).unwrap();
        let mut now = t0();
        // focus 1 -> short break
        now += Duration::minutes(26);
        s.reconcile(now);
        assert_eq!(s.phase, Phase::ShortBreak);
        s.apply(TimerCommand::Skip, now).unwrap(); // skip break -> focus 2
        now += Duration::minutes(26);
        s.reconcile(now);
        assert_eq!((s.phase, s.status), (Phase::LongBreak, TimerStatus::PhaseComplete));
        s.apply(TimerCommand::Continue, now).unwrap();
        assert_eq!(s.ends_at, Some(now + Duration::minutes(15)));
    }

    #[test]
    fn reset_and_extend_and_validation() {
        let mut s = start(TimerKind::Focus, Some(10));
        s.apply(TimerCommand::Extend { minutes: 5 }, t0()).unwrap();
        assert_eq!(s.remaining_ms(t0()), 15 * 60_000);
        s.apply(TimerCommand::Reset, t0()).unwrap();
        assert_eq!(s.status, TimerStatus::Idle);
        let bad = PomodoroConfig { focus_minutes: 0, ..PomodoroConfig::default() };
        assert!(s.apply(TimerCommand::Start { kind: TimerKind::Pomodoro, minutes: None, label: None, task_id: None, config: Some(bad) }, t0()).is_err());
        assert!(s.apply(TimerCommand::Start { kind: TimerKind::Focus, minutes: Some(0), label: None, task_id: None, config: None }, t0()).is_err());
    }
}
