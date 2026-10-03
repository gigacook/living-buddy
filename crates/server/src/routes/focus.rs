//! Focus timer, Pomodoro, event countdowns, alarms and the manual Claude usage card.

use crate::db::{new_id, parse_ts, parse_ts_opt, ts};
use crate::error::{AppError, AppResult};
use crate::routes::groups::ensure_member;
use crate::security::Actor;
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Tz;
use serde_json::{json, Value};
use tendly_core::alarm::{next_fire, parse_hhmm, AlarmRule};
use tendly_core::api::{Alarm, AlarmInput, Countdown, CountdownInput, SnoozeInput, TimerCommandInput, TimerView, UsageInput, UsageView};
use tendly_core::model::Category;
use tendly_core::recurrence::resolve_local;
use tendly_core::timer::TimerState;
use tendly_core::usage::{summarize, validate_percent, FIVE_HOUR_CHECKPOINTS, SEVEN_DAY_CHECKPOINTS};

async fn load_timer(state: &AppState, member: &str) -> AppResult<(TimerState, u32)> {
    let row: Option<(String, i64)> = sqlx::query_as("SELECT state, version FROM timers WHERE member_id = ?").bind(member).fetch_optional(&state.db).await?;
    Ok(match row {
        Some((s, v)) => (serde_json::from_str(&s).unwrap_or_default(), v as u32),
        None => (TimerState::default(), 0),
    })
}

async fn save_timer(state: &AppState, member: &str, s: &TimerState, expected: u32) -> AppResult<u32> {
    let now = ts(state.now());
    let json = serde_json::to_string(s)?;
    if expected == 0 {
        let res = sqlx::query("INSERT INTO timers (member_id, state, version, updated_at) VALUES (?,?,1,?) ON CONFLICT(member_id) DO NOTHING")
            .bind(member)
            .bind(&json)
            .bind(&now)
            .execute(&state.db)
            .await?;
        if res.rows_affected() == 1 {
            return Ok(1);
        }
    }
    let res = sqlx::query("UPDATE timers SET state = ?, version = version + 1, updated_at = ? WHERE member_id = ? AND version = ?")
        .bind(&json)
        .bind(&now)
        .bind(member)
        .bind(expected as i64)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::Conflict { message: "The timer changed on another device.".into(), current: None });
    }
    Ok(expected + 1)
}

/// Returns the timer brought up to date with the server clock. Because the
/// state stores absolute deadlines, this is correct after reloads and sleep.
pub async fn get_timer(State(state): State<AppState>, actor: Actor) -> AppResult<Json<TimerView>> {
    let (mut s, v) = load_timer(&state, &actor.id).await?;
    let now = state.now();
    let ended = s.reconcile(now);
    let v = if ended { save_timer(&state, &actor.id, &s, v).await? } else { v };
    Ok(Json(TimerView { remaining_ms: s.remaining_ms(now), state: s, server_now: now, version: v, phase_just_ended: ended }))
}

pub async fn timer_command(State(state): State<AppState>, actor: Actor, Json(input): Json<TimerCommandInput>) -> AppResult<Json<TimerView>> {
    let (mut s, v) = load_timer(&state, &actor.id).await?;
    if let Some(expected) = input.expected_version {
        if expected != v {
            return Err(AppError::Conflict { message: "The timer changed on another device.".into(), current: None });
        }
    }
    let now = state.now();
    if let tendly_core::timer::TimerCommand::Start { task_id: Some(t), .. } = &input.command {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE id = ? AND deleted_at IS NULL").bind(t).fetch_one(&state.db).await?;
        if exists == 0 {
            return Err(AppError::field("taskId", "That task no longer exists."));
        }
    }
    let ended = s.reconcile(now);
    let mut cmd = input.command;
    if let tendly_core::timer::TimerCommand::Start { label, .. } = &mut cmd {
        *label = validate::opt_text(label.as_deref(), 80, false);
    }
    s.apply(cmd, now).map_err(|e| AppError::bad(e.to_string()))?;
    let v = save_timer(&state, &actor.id, &s, v).await?;
    Ok(Json(TimerView { remaining_ms: s.remaining_ms(now), state: s, server_now: now, version: v, phase_just_ended: ended }))
}

// ---------------------------------------------------------------- countdowns

fn target(date: &str, time: Option<&str>, tz: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| AppError::field("date", "Use a date like 2026-12-24."))?;
    let t = time.and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok()).unwrap_or(NaiveTime::MIN);
    Ok(resolve_local(tz.parse::<Tz>().unwrap_or(Tz::UTC), d.and_time(t)))
}

pub async fn countdowns(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Vec<Countdown>>> {
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, Option<String>, Option<String>, String)>(
        "SELECT id, title, date, time, timezone, target_at, category, group_id, member_id FROM countdowns WHERE member_id = ? OR group_id IN (SELECT group_id FROM group_members WHERE member_id = ?) ORDER BY target_at",
    )
    .bind(&actor.id)
    .bind(&actor.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, title, date, time, timezone, target_at, category, group_id, member_id)| Countdown {
                id,
                title,
                date,
                time,
                timezone,
                target_at: parse_ts(&target_at),
                category: category.as_deref().and_then(Category::parse),
                group_id,
                member_id,
            })
            .collect(),
    ))
}

pub async fn create_countdown(State(state): State<AppState>, actor: Actor, Json(i): Json<CountdownInput>) -> AppResult<Json<Value>> {
    let title = validate::title("title", &i.title, 100)?;
    let date = validate::date("date", Some(&i.date))?.ok_or_else(|| AppError::field("date", "Pick a date."))?;
    let time = validate::time("time", i.time.as_deref())?;
    let tz = validate::timezone(i.timezone.as_deref(), &state.config.default_timezone)?;
    if let Some(g) = &i.group_id {
        ensure_member(&state, g, &actor).await?;
    }
    let at = target(&date, time.as_deref(), &tz)?;
    let id = new_id();
    sqlx::query("INSERT INTO countdowns (id, member_id, title, date, time, timezone, target_at, category, group_id, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(&actor.id)
        .bind(&title)
        .bind(&date)
        .bind(&time)
        .bind(&tz)
        .bind(ts(at))
        .bind(i.category.map(|c| c.as_str()))
        .bind(&i.group_id)
        .bind(ts(state.now()))
        .execute(&state.db)
        .await?;
    Ok(Json(json!({"id": id})))
}

pub async fn delete_countdown(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let n = sqlx::query("DELETE FROM countdowns WHERE id = ? AND (member_id = ? OR group_id IN (SELECT group_id FROM group_members WHERE member_id = ?))")
        .bind(&id)
        .bind(&actor.id)
        .bind(&actor.id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("countdown"));
    }
    Ok(Json(json!({"deleted": true})))
}

// ---------------------------------------------------------------- alarms

#[derive(sqlx::FromRow)]
struct AlarmRow {
    id: String,
    member_id: String,
    label: String,
    time: String,
    weekdays: i64,
    date: Option<String>,
    timezone: String,
    enabled: i64,
    sound: i64,
    task_id: Option<String>,
    last_fired_at: Option<String>,
    snoozed_until: Option<String>,
    created_at: String,
}

const ALARM_SELECT: &str = "SELECT id, member_id, label, time, weekdays, date, timezone, enabled, sound, task_id, last_fired_at, snoozed_until, created_at FROM alarms";

fn to_alarm(r: AlarmRow) -> Alarm {
    let rule = AlarmRule {
        time: parse_hhmm(&r.time).unwrap_or(NaiveTime::MIN),
        weekdays: r.weekdays as u8,
        date: r.date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
        tz: r.timezone.parse().unwrap_or(Tz::UTC),
        enabled: r.enabled != 0,
        snoozed_until: parse_ts_opt(r.snoozed_until.clone()),
    };
    let after = parse_ts_opt(r.last_fired_at.clone()).unwrap_or_else(|| parse_ts(&r.created_at));
    Alarm {
        next_fire_at: next_fire(&rule, after),
        id: r.id,
        member_id: r.member_id,
        label: r.label,
        time: r.time,
        weekdays: r.weekdays as u8,
        date: r.date,
        timezone: r.timezone,
        enabled: r.enabled != 0,
        sound: r.sound != 0,
        task_id: r.task_id,
        last_fired_at: parse_ts_opt(r.last_fired_at),
        snoozed_until: parse_ts_opt(r.snoozed_until),
    }
}

async fn load_alarm(state: &AppState, actor: &Actor, id: &str) -> AppResult<Alarm> {
    sqlx::query_as::<_, AlarmRow>(&format!("{ALARM_SELECT} WHERE id = ? AND member_id = ?"))
        .bind(id)
        .bind(&actor.id)
        .fetch_optional(&state.db)
        .await?
        .map(to_alarm)
        .ok_or(AppError::NotFound("alarm"))
}

pub async fn alarms(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Vec<Alarm>>> {
    let rows = sqlx::query_as::<_, AlarmRow>(&format!("{ALARM_SELECT} WHERE member_id = ? ORDER BY time")).bind(&actor.id).fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(to_alarm).collect()))
}

fn check_alarm_input(state: &AppState, i: &AlarmInput) -> AppResult<(String, String, Option<String>, String)> {
    let label = validate::title("label", &i.label, 80)?;
    let time = validate::time("time", Some(&i.time))?.ok_or_else(|| AppError::field("time", "Pick a time."))?;
    let date = validate::date("date", i.date.as_deref())?;
    let tz = validate::timezone(i.timezone.as_deref(), &state.config.default_timezone)?;
    if i.weekdays.unwrap_or(0) > 0b111_1111 {
        return Err(AppError::field("weekdays", "Invalid weekdays."));
    }
    Ok((label, time, date, tz))
}

pub async fn create_alarm(State(state): State<AppState>, actor: Actor, Json(i): Json<AlarmInput>) -> AppResult<Json<Alarm>> {
    let (label, time, date, tz) = check_alarm_input(&state, &i)?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alarms WHERE member_id = ?").bind(&actor.id).fetch_one(&state.db).await?;
    if count >= 50 {
        return Err(AppError::bad("That's a lot of alarms. Remove a few first."));
    }
    let id = new_id();
    let now = ts(state.now());
    sqlx::query("INSERT INTO alarms (id, member_id, label, time, weekdays, date, timezone, enabled, sound, task_id, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(&actor.id)
        .bind(&label)
        .bind(&time)
        .bind(i.weekdays.unwrap_or(0) as i64)
        .bind(&date)
        .bind(&tz)
        .bind(i.enabled.unwrap_or(true) as i64)
        .bind(i.sound.unwrap_or(true) as i64)
        .bind(&i.task_id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    Ok(Json(load_alarm(&state, &actor, &id).await?))
}

pub async fn update_alarm(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(i): Json<AlarmInput>) -> AppResult<Json<Alarm>> {
    load_alarm(&state, &actor, &id).await?;
    let (label, time, date, tz) = check_alarm_input(&state, &i)?;
    let now = ts(state.now());
    // Editing resets the reference point so an edited alarm doesn't fire "missed" immediately.
    sqlx::query("UPDATE alarms SET label=?, time=?, weekdays=?, date=?, timezone=?, enabled=?, sound=?, task_id=?, snoozed_until=NULL, last_fired_at=?, updated_at=? WHERE id=? AND member_id=?")
        .bind(&label)
        .bind(&time)
        .bind(i.weekdays.unwrap_or(0) as i64)
        .bind(&date)
        .bind(&tz)
        .bind(i.enabled.unwrap_or(true) as i64)
        .bind(i.sound.unwrap_or(true) as i64)
        .bind(&i.task_id)
        .bind(&now)
        .bind(&now)
        .bind(&id)
        .bind(&actor.id)
        .execute(&state.db)
        .await?;
    Ok(Json(load_alarm(&state, &actor, &id).await?))
}

pub async fn delete_alarm(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM alarms WHERE id = ? AND member_id = ?").bind(&id).bind(&actor.id).execute(&state.db).await?;
    Ok(Json(json!({"deleted": true})))
}

/// Records that the client showed the alarm. Idempotent per fire time.
pub async fn alarm_fired(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Alarm>> {
    let a = load_alarm(&state, &actor, &id).await?;
    let now = state.now();
    if a.next_fire_at.map(|n| n <= now).unwrap_or(false) {
        sqlx::query("UPDATE alarms SET last_fired_at = ?, snoozed_until = NULL WHERE id = ?").bind(ts(now)).bind(&id).execute(&state.db).await?;
    }
    Ok(Json(load_alarm(&state, &actor, &id).await?))
}

pub async fn alarm_snooze(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(i): Json<SnoozeInput>) -> AppResult<Json<Alarm>> {
    load_alarm(&state, &actor, &id).await?;
    if !(1..=120).contains(&i.minutes) {
        return Err(AppError::field("minutes", "Snooze for 1 to 120 minutes."));
    }
    let now = state.now();
    sqlx::query("UPDATE alarms SET snoozed_until = ?, last_fired_at = ? WHERE id = ?")
        .bind(ts(now + chrono::Duration::minutes(i.minutes as i64)))
        .bind(ts(now))
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(load_alarm(&state, &actor, &id).await?))
}

// ---------------------------------------------------------------- Claude usage (manual)

pub async fn usage(State(state): State<AppState>, actor: Actor) -> AppResult<Json<UsageView>> {
    let row: Option<(Option<f64>, Option<String>, Option<f64>, Option<String>, Option<String>, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT five_hour_percent, five_hour_resets_at, seven_day_percent, seven_day_resets_at, notes, reminder_thresholds, remind_on_reset, updated_at FROM usage_tracker WHERE member_id = ?",
    )
    .bind(&actor.id)
    .fetch_optional(&state.db)
    .await?;
    let now = state.now();
    let (fp, fr, sp, sr, notes, thr, ror, upd) = row.unwrap_or((None, None, None, None, None, "[]".into(), 0, None));
    Ok(Json(UsageView {
        five_hour: summarize(fp.map(|v| v as f32), parse_ts_opt(fr), FIVE_HOUR_CHECKPOINTS, now),
        seven_day: summarize(sp.map(|v| v as f32), parse_ts_opt(sr), SEVEN_DAY_CHECKPOINTS, now),
        notes,
        source: "manual".into(),
        updated_at: parse_ts_opt(upd),
        reminder_thresholds: serde_json::from_str(&thr).unwrap_or_default(),
        remind_on_reset: ror != 0,
    }))
}

pub async fn put_usage(State(state): State<AppState>, actor: Actor, Json(i): Json<UsageInput>) -> AppResult<Json<UsageView>> {
    validate_percent(i.five_hour_percent).map_err(|m| AppError::field("fiveHourPercent", m))?;
    validate_percent(i.seven_day_percent).map_err(|m| AppError::field("sevenDayPercent", m))?;
    let mut thr: Vec<u8> = i.reminder_thresholds.iter().copied().filter(|t| (1..=100).contains(t)).collect();
    thr.sort_unstable();
    thr.dedup();
    let old: Option<(Option<f64>, Option<f64>)> = sqlx::query_as("SELECT five_hour_percent, seven_day_percent FROM usage_tracker WHERE member_id = ?")
        .bind(&actor.id)
        .fetch_optional(&state.db)
        .await?;
    let now = state.now();
    sqlx::query(
        "INSERT INTO usage_tracker (member_id, five_hour_percent, five_hour_resets_at, seven_day_percent, seven_day_resets_at, notes, reminder_thresholds, remind_on_reset, updated_at) VALUES (?,?,?,?,?,?,?,?,?)
         ON CONFLICT(member_id) DO UPDATE SET five_hour_percent=excluded.five_hour_percent, five_hour_resets_at=excluded.five_hour_resets_at, seven_day_percent=excluded.seven_day_percent, seven_day_resets_at=excluded.seven_day_resets_at, notes=excluded.notes, reminder_thresholds=excluded.reminder_thresholds, remind_on_reset=excluded.remind_on_reset, updated_at=excluded.updated_at",
    )
    .bind(&actor.id)
    .bind(i.five_hour_percent.map(|v| v as f64))
    .bind(i.five_hour_resets_at.map(ts))
    .bind(i.seven_day_percent.map(|v| v as f64))
    .bind(i.seven_day_resets_at.map(ts))
    .bind(validate::opt_text(i.notes.as_deref(), 2000, true))
    .bind(serde_json::to_string(&thr)?)
    .bind(i.remind_on_reset as i64)
    .bind(ts(now))
    .execute(&state.db)
    .await?;
    // Optional reminders when an entered value crosses a chosen threshold.
    let (old5, old7) = old.unwrap_or((None, None));
    let mut crossed = tendly_core::usage::crossed(old5.map(|v| v as f32), i.five_hour_percent, &thr);
    crossed.extend(tendly_core::usage::crossed(old7.map(|v| v as f32), i.seven_day_percent, &thr));
    if !crossed.is_empty() {
        let mut conn = state.db.acquire().await?;
        let max = crossed.iter().max().copied().unwrap_or(0);
        crate::routes::tasks::notify(&mut conn, &state, &actor.id, "usage", &format!("Claude usage estimate passed {max}% (manual entry)"), None, None).await?;
    }
    usage(State(state), actor).await
}
