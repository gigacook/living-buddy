//! Templates, activity feed, notifications and nudges.

use crate::activity::{record, ActivityRow, NewActivity, SELECT as ACTIVITY_SELECT};
use crate::db::{new_id, parse_ts, parse_ts_opt, ts};
use crate::error::{AppError, AppResult};
use crate::routes::groups::ensure_member;
use crate::routes::tasks::create_task;
use crate::security::Actor;
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tendly_core::api::{Activity, Notification, NudgeInput, RoutineTemplate, Task, TaskInput, TemplatePatch, UseTemplateInput};
use tendly_core::model::{Category, RepeatMode};
use tendly_core::recurrence::RRule;

#[derive(sqlx::FromRow)]
struct TemplateRow {
    key: String,
    title: String,
    category: String,
    recurrence: String,
    repeat_mode: String,
    duration_minutes: i64,
    checklist: String,
    tip: String,
}

impl From<TemplateRow> for RoutineTemplate {
    fn from(r: TemplateRow) -> Self {
        let mode = RepeatMode::parse(&r.repeat_mode).unwrap_or(RepeatMode::Fixed);
        let mut label = RRule::parse(&r.recurrence).map(|x| x.describe()).unwrap_or_default();
        if mode == RepeatMode::AfterCompletion {
            label.push_str(" after done");
        }
        RoutineTemplate {
            key: r.key,
            title: r.title,
            category: Category::parse(&r.category).unwrap_or(Category::Home),
            recurrence: r.recurrence,
            recurrence_label: label,
            repeat_mode: mode,
            duration_minutes: r.duration_minutes as u32,
            checklist: serde_json::from_str(&r.checklist).unwrap_or_default(),
            tip: r.tip,
        }
    }
}

const TEMPLATE_SELECT: &str =
    "SELECT key, title, category, recurrence, repeat_mode, duration_minutes, checklist, tip FROM routine_templates";

async fn load_template(state: &AppState, key: &str) -> AppResult<RoutineTemplate> {
    sqlx::query_as::<_, TemplateRow>(&format!("{TEMPLATE_SELECT} WHERE key = ?"))
        .bind(key)
        .fetch_optional(&state.db)
        .await?
        .map(Into::into)
        .ok_or(AppError::NotFound("template"))
}

pub async fn templates(State(state): State<AppState>) -> AppResult<Json<Vec<RoutineTemplate>>> {
    let rows = sqlx::query_as::<_, TemplateRow>(&format!("{TEMPLATE_SELECT} ORDER BY rowid")).fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

pub async fn update_template(
    State(state): State<AppState>,
    actor: Actor,
    Path(key): Path<String>,
    Json(p): Json<TemplatePatch>,
) -> AppResult<Json<RoutineTemplate>> {
    let mut t = load_template(&state, &key).await?;
    if let Some(v) = &p.title {
        t.title = validate::title("title", v, 80)?;
    }
    if let Some(v) = &p.recurrence {
        t.recurrence = validate::recurrence(Some(v))?.ok_or_else(|| AppError::field("recurrence", "Templates need a repeat rule."))?;
    }
    if let Some(v) = p.repeat_mode {
        t.repeat_mode = v;
    }
    if let Some(v) = p.duration_minutes {
        if v == 0 || v > 600 {
            return Err(AppError::field("durationMinutes", "Use 1 to 600 minutes."));
        }
        t.duration_minutes = v;
    }
    if let Some(v) = &p.checklist {
        t.checklist = v.iter().map(|s| tendly_core::model::clean_text(s, 120, false)).filter(|s| !s.is_empty()).take(30).collect();
    }
    if let Some(v) = &p.tip {
        t.tip = tendly_core::model::clean_text(v, 300, false);
    }
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE routine_templates SET title=?, recurrence=?, repeat_mode=?, duration_minutes=?, checklist=?, tip=?, updated_at=? WHERE key=?")
        .bind(&t.title)
        .bind(&t.recurrence)
        .bind(t.repeat_mode.as_str())
        .bind(t.duration_minutes as i64)
        .bind(serde_json::to_string(&t.checklist)?)
        .bind(&t.tip)
        .bind(&now)
        .bind(&key)
        .execute(&mut *tx)
        .await?;
    record(
        &mut tx,
        &now,
        NewActivity {
            actor: Some(&actor),
            source: "app",
            entity_type: "template",
            entity_id: &key,
            group_id: None,
            op: "update",
            summary: format!("{} edited the “{}” template", actor.name, t.title),
            revision: 0,
            before: None,
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load_template(&state, &key).await?))
}

pub async fn use_template(
    State(state): State<AppState>,
    actor: Actor,
    Path(key): Path<String>,
    Json(input): Json<UseTemplateInput>,
) -> AppResult<Json<Task>> {
    let t = load_template(&state, &key).await?;
    let task = create_task(
        &state,
        &actor,
        TaskInput {
            title: input.title.clone().unwrap_or(t.title.clone()),
            group_id: input.group_id.clone(),
            notes: Some(t.tip.clone()),
            category: Some(t.category),
            duration_minutes: Some(t.duration_minutes),
            assignee_id: input.assignee_id.clone(),
            due_date: input.due_date.clone(),
            timezone: input.timezone.clone(),
            recurrence: Some(input.recurrence.clone().unwrap_or(t.recurrence.clone())),
            repeat_mode: Some(t.repeat_mode),
            rotation: input.rotation.clone(),
            subtasks: Some(t.checklist.clone()),
            template_key: Some(key.clone()),
            ..TaskInput::default()
        },
        "template",
    )
    .await?;
    Ok(Json(task))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ActivityQuery {
    group_id: Option<String>,
    entity_type: Option<String>,
    entity_id: Option<String>,
    before: Option<String>,
    limit: Option<u32>,
}

/// Activity visible to the actor: their groups, plus their own personal items.
pub async fn activity(State(state): State<AppState>, actor: Actor, Query(q): Query<ActivityQuery>) -> AppResult<Json<Vec<Activity>>> {
    let mut sql = format!("{ACTIVITY_SELECT} WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();
    if let Some(g) = &q.group_id {
        ensure_member(&state, g, &actor).await?;
        sql.push_str(" AND group_id = ?");
        binds.push(g.clone());
    } else {
        sql.push_str(" AND ((group_id IS NOT NULL AND group_id IN (SELECT group_id FROM group_members WHERE member_id = ?)) OR (group_id IS NULL AND actor_id = ?) OR (group_id IS NULL AND entity_type IN ('calendar_source','calendar_event','share')))");
        binds.push(actor.id.clone());
        binds.push(actor.id.clone());
    }
    if let Some(t) = &q.entity_type {
        sql.push_str(" AND entity_type = ?");
        binds.push(t.clone());
    }
    if let Some(e) = &q.entity_id {
        sql.push_str(" AND entity_id = ?");
        binds.push(e.clone());
    }
    if let Some(b) = &q.before {
        sql.push_str(" AND at < ?");
        binds.push(b.clone());
    }
    sql.push_str(&format!(" ORDER BY at DESC LIMIT {}", q.limit.unwrap_or(50).clamp(1, 200)));
    let mut query = sqlx::query_as::<_, ActivityRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    Ok(Json(query.fetch_all(&state.db).await?.into_iter().map(Into::into).collect()))
}

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: String,
    kind: String,
    title: String,
    body: Option<String>,
    task_id: Option<String>,
    from_member_id: Option<String>,
    created_at: String,
    deliver_after: String,
    read_at: Option<String>,
}

pub async fn notifications(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Vec<Notification>>> {
    let rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT id, kind, title, body, task_id, from_member_id, created_at, deliver_after, read_at FROM notifications WHERE member_id = ? AND deliver_after <= ? ORDER BY created_at DESC LIMIT 100",
    )
    .bind(&actor.id)
    .bind(ts(state.now()))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| Notification {
                id: r.id,
                kind: r.kind,
                title: r.title,
                body: r.body,
                task_id: r.task_id,
                from_member_id: r.from_member_id,
                created_at: parse_ts(&r.created_at),
                deliver_after: parse_ts(&r.deliver_after),
                read_at: parse_ts_opt(r.read_at),
            })
            .collect(),
    ))
}

pub async fn mark_read(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE notifications SET read_at = ? WHERE id = ? AND member_id = ?")
        .bind(ts(state.now()))
        .bind(&id)
        .bind(&actor.id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({"ok": true})))
}

pub async fn mark_all_read(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE notifications SET read_at = ? WHERE member_id = ? AND read_at IS NULL AND deliver_after <= ?")
        .bind(ts(state.now()))
        .bind(&actor.id)
        .bind(ts(state.now()))
        .execute(&state.db)
        .await?;
    Ok(Json(json!({"ok": true})))
}

/// A friendly, opt-in, rate-limited reminder to another group member.
pub async fn nudge(State(state): State<AppState>, actor: Actor, Json(input): Json<NudgeInput>) -> AppResult<Json<Value>> {
    let to = crate::routes::members::load(&state, &input.to_member_id).await?;
    let mut tx = state.db.begin().await?;
    // Nudges only between people who share a group.
    let shared: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members a JOIN group_members b ON a.group_id = b.group_id WHERE a.member_id = ? AND b.member_id = ?",
    )
    .bind(&actor.id)
    .bind(&to.id)
    .fetch_one(&mut *tx)
    .await?;
    if shared == 0 {
        return Err(AppError::Forbidden("You can only send reminders to people in your groups."));
    }
    let now = state.now();
    let history: Vec<tendly_core::notify::SentNudge> = sqlx::query_as::<_, (String, String, Option<String>, String)>(
        "SELECT from_member_id, to_member_id, task_id, at FROM nudges WHERE from_member_id = ? AND to_member_id = ? AND at > ?",
    )
    .bind(&actor.id)
    .bind(&to.id)
    .bind(ts(now - chrono::Duration::hours(24)))
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(from, to, task_id, at)| tendly_core::notify::SentNudge { from, to, task_id, at: parse_ts(&at) })
    .collect();
    let prefs = tendly_core::notify::RecipientPrefs {
        accepts_nudges: to.prefs.accepts_nudges,
        quiet_start: to.prefs.quiet_start.as_deref().and_then(tendly_core::alarm::parse_hhmm),
        quiet_end: to.prefs.quiet_end.as_deref().and_then(tendly_core::alarm::parse_hhmm),
        tz: to.prefs.timezone.parse().unwrap_or(chrono_tz::Tz::UTC),
    };
    let deliver = tendly_core::notify::plan_nudge(&history, &actor.id, &to.id, input.task_id.as_deref(), &prefs, now)
        .map_err(|e| AppError::bad(e.to_string()))?;
    let task_title: Option<String> = match &input.task_id {
        Some(t) => {
            sqlx::query_scalar("SELECT title FROM tasks WHERE id = ? AND deleted_at IS NULL").bind(t).fetch_optional(&mut *tx).await?
        }
        None => None,
    };
    let msg = validate::opt_text(input.message.as_deref(), 140, false);
    let title = match (&task_title, &msg) {
        (Some(t), Some(m)) => format!("{}: “{t}” — {m}", actor.name),
        (Some(t), None) => format!("{} gently reminds you about “{t}”", actor.name),
        (None, Some(m)) => format!("{}: {m}", actor.name),
        (None, None) => format!("{} says hi 👋", actor.name),
    };
    sqlx::query("INSERT INTO nudges (id, from_member_id, to_member_id, task_id, at) VALUES (?,?,?,?,?)")
        .bind(new_id())
        .bind(&actor.id)
        .bind(&to.id)
        .bind(&input.task_id)
        .bind(ts(now))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO notifications (id, member_id, kind, title, task_id, from_member_id, created_at, deliver_after) VALUES (?,?,?,?,?,?,?,?)")
        .bind(new_id())
        .bind(&to.id)
        .bind("nudge")
        .bind(&title)
        .bind(&input.task_id)
        .bind(&actor.id)
        .bind(ts(now))
        .bind(ts(deliver))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"deliverAfter": deliver, "deferred": deliver > now})))
}
