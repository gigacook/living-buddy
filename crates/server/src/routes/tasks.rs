//! Tasks, recurring routines and project board items.

use crate::activity::{record, ActivityRow, NewActivity, SELECT as ACTIVITY_SELECT};
use crate::db::{new_id, parse_ts, parse_ts_opt, ts};
use crate::error::{AppError, AppResult};
use crate::routes::groups::{ensure_member, is_member};
use crate::security::Actor;
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::SqliteConnection;
use tendly_core::api::{
    CompleteTaskResult, MoveTaskInput, Subtask, Task, TaskCompletion, TaskHistory, TaskInput, TaskPatch,
};
use tendly_core::model::{Category, GroupMode, Priority, RepeatMode, DONE_COLUMN};
use tendly_core::recurrence::{resolve_local, RRule};
use tendly_core::rotation::next_assignee;

#[derive(sqlx::FromRow, Clone)]
pub struct TaskRow {
    pub id: String,
    pub group_id: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub category: String,
    pub priority: String,
    pub duration_minutes: Option<i64>,
    pub owner_id: String,
    pub assignee_id: Option<String>,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub start_date: Option<String>,
    pub start_time: Option<String>,
    pub deadline: Option<String>,
    pub timezone: String,
    pub recurrence: Option<String>,
    pub repeat_mode: String,
    pub series_start: Option<String>,
    pub rotation: String,
    pub column_key: Option<String>,
    pub milestone_id: Option<String>,
    pub tags: String,
    pub subtasks: String,
    pub position: f64,
    pub completed_at: Option<String>,
    pub completed_by: Option<String>,
    pub completion_count: i64,
    pub template_key: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub const SELECT: &str = "SELECT id, group_id, title, notes, category, priority, duration_minutes, owner_id, assignee_id, due_date, due_time, start_date, start_time, deadline, timezone, recurrence, repeat_mode, series_start, rotation, column_key, milestone_id, tags, subtasks, position, completed_at, completed_by, completion_count, template_key, version, created_at, updated_at FROM tasks";

impl From<TaskRow> for Task {
    fn from(r: TaskRow) -> Self {
        let recurrence_label = r.recurrence.as_deref().and_then(|s| RRule::parse(s).ok()).map(|rr| {
            let mut d = rr.describe();
            if r.repeat_mode == "after_completion" {
                d.push_str(" after done");
            }
            d
        });
        Task {
            id: r.id,
            group_id: r.group_id,
            title: r.title,
            notes: r.notes,
            category: Category::parse(&r.category).unwrap_or(Category::Personal),
            priority: Priority::parse(&r.priority).unwrap_or(Priority::Normal),
            duration_minutes: r.duration_minutes.map(|d| d as u32),
            owner_id: r.owner_id,
            assignee_id: r.assignee_id,
            due_date: r.due_date,
            due_time: r.due_time,
            start_date: r.start_date,
            start_time: r.start_time,
            deadline: r.deadline,
            timezone: r.timezone,
            recurrence: r.recurrence,
            recurrence_label,
            repeat_mode: RepeatMode::parse(&r.repeat_mode).unwrap_or(RepeatMode::Fixed),
            rotation: serde_json::from_str(&r.rotation).unwrap_or_default(),
            column_key: r.column_key,
            milestone_id: r.milestone_id,
            tags: serde_json::from_str(&r.tags).unwrap_or_default(),
            subtasks: serde_json::from_str(&r.subtasks).unwrap_or_default(),
            position: r.position,
            completed_at: parse_ts_opt(r.completed_at),
            completed_by: r.completed_by,
            completion_count: r.completion_count as u32,
            template_key: r.template_key,
            version: r.version as u32,
            created_at: parse_ts(&r.created_at),
            updated_at: parse_ts(&r.updated_at),
        }
    }
}

pub async fn load_row(conn: &mut SqliteConnection, id: &str) -> AppResult<TaskRow> {
    sqlx::query_as::<_, TaskRow>(&format!("{SELECT} WHERE id = ? AND deleted_at IS NULL"))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("task"))
}

/// Tasks in a group are visible to its members; personal tasks to their owner and assignee.
async fn ensure_can_see(conn: &mut SqliteConnection, row: &TaskRow, actor: &Actor) -> AppResult<()> {
    match &row.group_id {
        Some(g) => {
            if !is_member(conn, g, &actor.id).await? {
                return Err(AppError::Forbidden("This task belongs to a group you are not in."));
            }
        }
        None => {
            if row.owner_id != actor.id && row.assignee_id.as_deref() != Some(&actor.id) {
                return Err(AppError::Forbidden("This is someone else's personal task."));
            }
        }
    }
    Ok(())
}

async fn member_name(conn: &mut SqliteConnection, id: Option<&str>) -> AppResult<String> {
    match id {
        None => Ok("no one".into()),
        Some(id) => Ok(sqlx::query_scalar::<_, String>("SELECT display_name FROM members WHERE id = ?")
            .bind(id)
            .fetch_optional(conn)
            .await?
            .unwrap_or_else(|| "someone".into())),
    }
}

async fn group_mode_and_columns(conn: &mut SqliteConnection, group_id: &str) -> AppResult<(GroupMode, Vec<String>)> {
    let (mode, cols): (String, String) = sqlx::query_as("SELECT mode, columns FROM tgroups WHERE id = ?")
        .bind(group_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("group"))?;
    let cols: Vec<tendly_core::api::BoardColumn> = serde_json::from_str(&cols).unwrap_or_default();
    Ok((GroupMode::parse(&mode).unwrap_or(GroupMode::Household), cols.into_iter().map(|c| c.key).collect()))
}

async fn validate_assignee(conn: &mut SqliteConnection, group_id: Option<&str>, owner: &str, assignee: Option<&str>) -> AppResult<()> {
    let Some(a) = assignee else { return Ok(()) };
    match group_id {
        Some(g) => {
            if !is_member(conn, g, a).await? {
                return Err(AppError::field("assigneeId", "That person is not in this group."));
            }
        }
        None => {
            if a != owner {
                return Err(AppError::field("assigneeId", "Personal tasks can only be assigned to their owner. Put it in a group to share it."));
            }
        }
    }
    Ok(())
}

async fn validate_rotation(conn: &mut SqliteConnection, group_id: Option<&str>, rotation: &[String]) -> AppResult<()> {
    if rotation.is_empty() {
        return Ok(());
    }
    let Some(g) = group_id else {
        return Err(AppError::field("rotation", "Rotation needs a group."));
    };
    if rotation.len() > 20 {
        return Err(AppError::field("rotation", "Rotation can include up to 20 people."));
    }
    for m in rotation {
        if !is_member(conn, g, m).await? {
            return Err(AppError::field("rotation", "Everyone in the rotation must be in the group."));
        }
    }
    Ok(())
}

fn series_anchor(due_date: Option<&str>, due_time: Option<&str>) -> Option<String> {
    let d = NaiveDate::parse_from_str(due_date?, "%Y-%m-%d").ok()?;
    let t = due_time.and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok()).unwrap_or(NaiveTime::MIN);
    Some(d.and_time(t).format("%Y-%m-%dT%H:%M").to_string())
}

fn parse_anchor(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok()
}

fn subtasks_from_titles(titles: &[String]) -> AppResult<Vec<Subtask>> {
    if titles.len() > 50 {
        return Err(AppError::field("subtasks", "Up to 50 checklist items."));
    }
    Ok(titles
        .iter()
        .map(|t| tendly_core::model::clean_text(t, 120, false))
        .filter(|t| !t.is_empty())
        .map(|title| Subtask { id: new_id(), title, done: false })
        .collect())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ListQuery {
    group_id: Option<String>,
    assignee_id: Option<String>,
    category: Option<String>,
    tag: Option<String>,
    /// open (default), done, all
    status: Option<String>,
    /// Only the actor's personal tasks.
    personal: Option<bool>,
    due_before: Option<String>,
    due_after: Option<String>,
    q: Option<String>,
}

pub async fn list(State(state): State<AppState>, actor: Actor, Query(q): Query<ListQuery>) -> AppResult<Json<Vec<Task>>> {
    let mut sql = format!("{SELECT} WHERE deleted_at IS NULL");
    let mut binds: Vec<String> = Vec::new();
    if let Some(g) = &q.group_id {
        ensure_member(&state, g, &actor).await?;
        sql.push_str(" AND group_id = ?");
        binds.push(g.clone());
    } else if q.personal == Some(true) {
        sql.push_str(" AND group_id IS NULL AND (owner_id = ? OR assignee_id = ?)");
        binds.push(actor.id.clone());
        binds.push(actor.id.clone());
    } else {
        sql.push_str(" AND ((group_id IS NULL AND (owner_id = ? OR assignee_id = ?)) OR group_id IN (SELECT group_id FROM group_members WHERE member_id = ?))");
        binds.extend([actor.id.clone(), actor.id.clone(), actor.id.clone()]);
    }
    if let Some(a) = &q.assignee_id {
        if a == "none" {
            sql.push_str(" AND assignee_id IS NULL");
        } else {
            sql.push_str(" AND assignee_id = ?");
            binds.push(a.clone());
        }
    }
    if let Some(c) = &q.category {
        Category::parse(c).ok_or_else(|| AppError::field("category", "Unknown category."))?;
        sql.push_str(" AND category = ?");
        binds.push(c.clone());
    }
    match q.status.as_deref().unwrap_or("open") {
        "open" => sql.push_str(" AND completed_at IS NULL"),
        "done" => sql.push_str(" AND completed_at IS NOT NULL"),
        "all" => {}
        _ => return Err(AppError::field("status", "Use open, done or all.")),
    }
    if let Some(d) = validate::date("dueBefore", q.due_before.as_deref())? {
        sql.push_str(" AND due_date IS NOT NULL AND due_date <= ?");
        binds.push(d);
    }
    if let Some(d) = validate::date("dueAfter", q.due_after.as_deref())? {
        sql.push_str(" AND due_date IS NOT NULL AND due_date >= ?");
        binds.push(d);
    }
    if let Some(term) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(" AND (title LIKE ? ESCAPE '\\' OR notes LIKE ? ESCAPE '\\')");
        let esc = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pat = format!("%{}%", esc.chars().take(80).collect::<String>());
        binds.push(pat.clone());
        binds.push(pat);
    }
    sql.push_str(" ORDER BY completed_at IS NOT NULL, COALESCE(due_date, '9999-12-31'), COALESCE(due_time, '99:99'), position, created_at LIMIT 1000");
    let mut query = sqlx::query_as::<_, TaskRow>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    let rows = query.fetch_all(&state.db).await?;
    let mut tasks: Vec<Task> = rows.into_iter().map(Task::from).collect();
    if let Some(tag) = q.tag.as_deref().map(|t| t.to_lowercase()) {
        tasks.retain(|t| t.tags.contains(&tag));
    }
    Ok(Json(tasks))
}

pub async fn get(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Task>> {
    let mut conn = state.db.acquire().await?;
    let row = load_row(&mut conn, &id).await?;
    ensure_can_see(&mut conn, &row, &actor).await?;
    Ok(Json(row.into()))
}

pub async fn create_task(state: &AppState, actor: &Actor, input: TaskInput, source: &str) -> AppResult<Task> {
    let title = validate::title("title", &input.title, 200)?;
    let mut conn = state.db.begin().await?;
    let group_id = input.group_id.clone().filter(|g| !g.is_empty());
    let mut column_key = None;
    if let Some(g) = &group_id {
        if !is_member(&mut conn, g, &actor.id).await? {
            return Err(AppError::Forbidden("Join this group first to add items to it."));
        }
        let (mode, cols) = group_mode_and_columns(&mut conn, g).await?;
        if mode == GroupMode::Project {
            let wanted = input.column_key.clone().unwrap_or_else(|| cols.first().cloned().unwrap_or_else(|| "backlog".into()));
            if !cols.contains(&wanted) {
                return Err(AppError::field("columnKey", "That column does not exist on this board."));
            }
            column_key = Some(wanted);
        }
    }
    let tz = validate::timezone(input.timezone.as_deref(), &state.config.default_timezone)?;
    let recurrence = validate::recurrence(input.recurrence.as_deref())?;
    let mut due_date = validate::date("dueDate", input.due_date.as_deref())?;
    let due_time = validate::time("dueTime", input.due_time.as_deref())?;
    if recurrence.is_some() && due_date.is_none() {
        let tzv: Tz = tz.parse().unwrap_or(Tz::UTC);
        due_date = Some(state.now().with_timezone(&tzv).format("%Y-%m-%d").to_string());
    }
    let rotation = input.rotation.clone().unwrap_or_default();
    validate_rotation(&mut conn, group_id.as_deref(), &rotation).await?;
    let assignee = input.assignee_id.clone().or_else(|| rotation.first().cloned()).or_else(|| group_id.is_none().then(|| actor.id.clone()));
    validate_assignee(&mut conn, group_id.as_deref(), &actor.id, assignee.as_deref()).await?;
    let tags = validate::tags(&input.tags.clone().unwrap_or_default())?;
    let subtasks = subtasks_from_titles(&input.subtasks.clone().unwrap_or_default())?;
    if let Some(d) = input.duration_minutes {
        if d == 0 || d > 24 * 60 {
            return Err(AppError::field("durationMinutes", "Use between 1 minute and 24 hours."));
        }
    }
    if let Some(m) = &input.milestone_id {
        let ok: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM milestones WHERE id = ? AND group_id IS ?").bind(m).bind(&group_id).fetch_one(&mut *conn).await?;
        if ok == 0 {
            return Err(AppError::field("milestoneId", "That milestone is not in this group."));
        }
    }
    let id = new_id();
    let now = ts(state.now());
    let position: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(MAX(position), 0) + 1 AS REAL) FROM tasks WHERE group_id IS ?").bind(&group_id).fetch_one(&mut *conn).await?;
    sqlx::query(
        "INSERT INTO tasks (id, group_id, title, notes, category, priority, duration_minutes, owner_id, assignee_id, due_date, due_time, start_date, start_time, deadline, timezone, recurrence, repeat_mode, series_start, rotation, column_key, milestone_id, tags, subtasks, position, template_key, version, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,?)",
    )
    .bind(&id)
    .bind(&group_id)
    .bind(&title)
    .bind(validate::opt_text(input.notes.as_deref(), 5000, true))
    .bind(input.category.unwrap_or(Category::Personal).as_str())
    .bind(input.priority.unwrap_or(Priority::Normal).as_str())
    .bind(input.duration_minutes.map(|d| d as i64))
    .bind(&actor.id)
    .bind(&assignee)
    .bind(&due_date)
    .bind(&due_time)
    .bind(validate::date("startDate", input.start_date.as_deref())?)
    .bind(validate::time("startTime", input.start_time.as_deref())?)
    .bind(validate::date("deadline", input.deadline.as_deref())?)
    .bind(&tz)
    .bind(&recurrence)
    .bind(input.repeat_mode.unwrap_or(RepeatMode::Fixed).as_str())
    .bind(series_anchor(due_date.as_deref(), due_time.as_deref()))
    .bind(serde_json::to_string(&rotation)?)
    .bind(&column_key)
    .bind(&input.milestone_id)
    .bind(serde_json::to_string(&tags)?)
    .bind(serde_json::to_string(&subtasks)?)
    .bind(position)
    .bind(&input.template_key)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await?;
    let who = member_name(&mut conn, assignee.as_deref()).await?;
    record(&mut conn, &now, NewActivity {
        actor: Some(actor), source, entity_type: "task", entity_id: &id, group_id: group_id.as_deref(), op: "create",
        summary: format!("{} added “{title}”{}", actor.name, if assignee.is_some() { format!(" for {who}") } else { String::new() }),
        revision: 1, before: None, after: Some(json!({"assigneeId": assignee, "dueDate": due_date})),
    }).await?;
    if let Some(a) = &assignee {
        if a != &actor.id {
            notify(&mut conn, state, a, "assignment", &format!("{} gave you “{title}”", actor.name), Some(&id), Some(&actor.id)).await?;
        }
    }
    let row = load_row(&mut conn, &id).await?;
    conn.commit().await?;
    Ok(row.into())
}

pub async fn create(State(state): State<AppState>, actor: Actor, Json(input): Json<TaskInput>) -> AppResult<Json<Task>> {
    Ok(Json(create_task(&state, &actor, input, "app").await?))
}

pub async fn notify(
    conn: &mut SqliteConnection,
    state: &AppState,
    member_id: &str,
    kind: &str,
    title: &str,
    task_id: Option<&str>,
    from: Option<&str>,
) -> AppResult<()> {
    let now = state.now();
    let prefs: Option<String> = sqlx::query_scalar("SELECT prefs FROM members WHERE id = ?").bind(member_id).fetch_optional(&mut *conn).await?;
    let prefs: tendly_core::api::MemberPrefs = prefs.and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default();
    let rp = tendly_core::notify::RecipientPrefs {
        accepts_nudges: prefs.accepts_nudges,
        quiet_start: prefs.quiet_start.as_deref().and_then(tendly_core::alarm::parse_hhmm),
        quiet_end: prefs.quiet_end.as_deref().and_then(tendly_core::alarm::parse_hhmm),
        tz: prefs.timezone.parse().unwrap_or(Tz::UTC),
    };
    // Reminders you set for yourself are not deferred by quiet hours.
    let deliver = if from.is_none() && kind == "usage" { now } else { tendly_core::notify::quiet_hours_release(&rp, now) };
    sqlx::query("INSERT INTO notifications (id, member_id, kind, title, body, task_id, from_member_id, created_at, deliver_after) VALUES (?,?,?,?,NULL,?,?,?,?)")
        .bind(new_id())
        .bind(member_id)
        .bind(kind)
        .bind(tendly_core::model::clean_text(title, 200, false))
        .bind(task_id)
        .bind(from)
        .bind(ts(now))
        .bind(ts(deliver))
        .execute(conn)
        .await?;
    Ok(())
}

fn diff_fields(before: &Task, after: &Task, keys: &[&str]) -> (Value, Value) {
    let b = serde_json::to_value(before).unwrap_or(Value::Null);
    let a = serde_json::to_value(after).unwrap_or(Value::Null);
    let mut bm = Map::new();
    let mut am = Map::new();
    for k in keys {
        if b.get(*k) != a.get(*k) {
            bm.insert((*k).into(), b.get(*k).cloned().unwrap_or(Value::Null));
            am.insert((*k).into(), a.get(*k).cloned().unwrap_or(Value::Null));
        }
    }
    (Value::Object(bm), Value::Object(am))
}

fn opt_str(v: &Value) -> AppResult<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(AppError::bad("Expected text.")),
    }
}

pub async fn update(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(p): Json<TaskPatch>) -> AppResult<Json<Task>> {
    let mut tx = state.db.begin().await?;
    let row = load_row(&mut tx, &id).await?;
    ensure_can_see(&mut tx, &row, &actor).await?;
    if row.version as u32 != p.expected_version {
        let current: Task = row.into();
        return Err(AppError::Conflict {
            message: "Someone else changed this while you were editing. Review their version and try again.".into(),
            current: Some(serde_json::to_value(current)?),
        });
    }
    let before: Task = row.clone().into();
    let mut r = row.clone();
    let changes = p.changes.as_object().ok_or_else(|| AppError::bad("Changes must be an object."))?;
    for (k, v) in changes {
        match k.as_str() {
            "title" => r.title = validate::title("title", v.as_str().unwrap_or(""), 200)?,
            "notes" => r.notes = validate::opt_text(opt_str(v)?.as_deref(), 5000, true),
            "category" => {
                r.category = Category::parse(v.as_str().unwrap_or("")).ok_or_else(|| AppError::field("category", "Unknown category."))?.as_str().into()
            }
            "priority" => {
                r.priority = Priority::parse(v.as_str().unwrap_or("")).ok_or_else(|| AppError::field("priority", "Unknown priority."))?.as_str().into()
            }
            "durationMinutes" => {
                r.duration_minutes = match v {
                    Value::Null => None,
                    v => Some(v.as_u64().filter(|d| (1..=1440).contains(d)).ok_or_else(|| AppError::field("durationMinutes", "Use 1 to 1440 minutes."))? as i64),
                }
            }
            "assigneeId" => r.assignee_id = opt_str(v)?,
            "dueDate" => r.due_date = validate::date("dueDate", opt_str(v)?.as_deref())?,
            "dueTime" => r.due_time = validate::time("dueTime", opt_str(v)?.as_deref())?,
            "startDate" => r.start_date = validate::date("startDate", opt_str(v)?.as_deref())?,
            "startTime" => r.start_time = validate::time("startTime", opt_str(v)?.as_deref())?,
            "deadline" => r.deadline = validate::date("deadline", opt_str(v)?.as_deref())?,
            "timezone" => r.timezone = validate::timezone(opt_str(v)?.as_deref(), &state.config.default_timezone)?,
            "recurrence" => r.recurrence = validate::recurrence(opt_str(v)?.as_deref())?,
            "repeatMode" => {
                r.repeat_mode = RepeatMode::parse(v.as_str().unwrap_or("")).ok_or_else(|| AppError::field("repeatMode", "Unknown repeat mode."))?.as_str().into()
            }
            "rotation" => {
                let list: Vec<String> = serde_json::from_value(v.clone()).map_err(|_| AppError::field("rotation", "Expected a list of people."))?;
                r.rotation = serde_json::to_string(&list)?;
            }
            "columnKey" => r.column_key = opt_str(v)?,
            "milestoneId" => r.milestone_id = opt_str(v)?,
            "tags" => {
                let list: Vec<String> = serde_json::from_value(v.clone()).map_err(|_| AppError::field("tags", "Expected a list of tags."))?;
                r.tags = serde_json::to_string(&validate::tags(&list)?)?;
            }
            "subtasks" => {
                let list: Vec<Subtask> = serde_json::from_value(v.clone()).map_err(|_| AppError::field("subtasks", "Expected checklist items."))?;
                if list.len() > 50 {
                    return Err(AppError::field("subtasks", "Up to 50 checklist items."));
                }
                let cleaned: Vec<Subtask> = list
                    .into_iter()
                    .map(|s| Subtask { id: if s.id.is_empty() { new_id() } else { s.id.chars().take(64).collect() }, title: tendly_core::model::clean_text(&s.title, 120, false), done: s.done })
                    .filter(|s| !s.title.is_empty())
                    .collect();
                r.subtasks = serde_json::to_string(&cleaned)?;
            }
            "position" => r.position = v.as_f64().ok_or_else(|| AppError::field("position", "Expected a number."))?,
            "groupId" => {
                let g = opt_str(v)?;
                if g == r.group_id {
                    continue;
                }
                if let Some(g) = &g {
                    if !is_member(&mut tx, g, &actor.id).await? {
                        return Err(AppError::Forbidden("Join that group first."));
                    }
                    let (mode, cols) = group_mode_and_columns(&mut tx, g).await?;
                    r.column_key = (mode == GroupMode::Project).then(|| cols.first().cloned().unwrap_or_else(|| "backlog".into()));
                } else {
                    r.column_key = None;
                    r.rotation = "[]".into();
                    if r.owner_id != actor.id {
                        return Err(AppError::Forbidden("Only the owner can make a task personal."));
                    }
                    r.assignee_id = Some(r.owner_id.clone());
                }
                r.group_id = g;
            }
            // Read-only or computed fields are ignored rather than trusted.
            _ => {}
        }
    }
    let rotation: Vec<String> = serde_json::from_str(&r.rotation).unwrap_or_default();
    validate_rotation(&mut tx, r.group_id.as_deref(), &rotation).await?;
    validate_assignee(&mut tx, r.group_id.as_deref(), &r.owner_id, r.assignee_id.as_deref()).await?;
    if let (Some(g), Some(c)) = (&r.group_id, &r.column_key) {
        let (_, cols) = group_mode_and_columns(&mut tx, g).await?;
        if !cols.contains(c) {
            return Err(AppError::field("columnKey", "That column does not exist on this board."));
        }
    }
    if r.recurrence.is_some() && r.due_date.is_none() {
        return Err(AppError::field("dueDate", "Repeating tasks need a due date to start from."));
    }
    if r.recurrence != row.recurrence || r.due_date != row.due_date || r.due_time != row.due_time {
        r.series_start = series_anchor(r.due_date.as_deref(), r.due_time.as_deref());
    }
    let now = ts(state.now());
    sqlx::query(
        "UPDATE tasks SET group_id=?, title=?, notes=?, category=?, priority=?, duration_minutes=?, assignee_id=?, due_date=?, due_time=?, start_date=?, start_time=?, deadline=?, timezone=?, recurrence=?, repeat_mode=?, series_start=?, rotation=?, column_key=?, milestone_id=?, tags=?, subtasks=?, position=?, version = version + 1, updated_at=? WHERE id = ? AND version = ?",
    )
    .bind(&r.group_id)
    .bind(&r.title)
    .bind(&r.notes)
    .bind(&r.category)
    .bind(&r.priority)
    .bind(r.duration_minutes)
    .bind(&r.assignee_id)
    .bind(&r.due_date)
    .bind(&r.due_time)
    .bind(&r.start_date)
    .bind(&r.start_time)
    .bind(&r.deadline)
    .bind(&r.timezone)
    .bind(&r.recurrence)
    .bind(&r.repeat_mode)
    .bind(&r.series_start)
    .bind(&r.rotation)
    .bind(&r.column_key)
    .bind(&r.milestone_id)
    .bind(&r.tags)
    .bind(&r.subtasks)
    .bind(r.position)
    .bind(&now)
    .bind(&id)
    .bind(row.version)
    .execute(&mut *tx)
    .await?;
    let after_row = load_row(&mut tx, &id).await?;
    let after: Task = after_row.into();
    let (b, a) = diff_fields(&before, &after, &[
        "title", "notes", "category", "priority", "durationMinutes", "assigneeId", "dueDate", "dueTime", "startDate", "startTime",
        "deadline", "recurrence", "repeatMode", "rotation", "columnKey", "milestoneId", "tags", "groupId",
    ]);
    let reassigned = before.assignee_id != after.assignee_id;
    let summary = if reassigned {
        let from = member_name(&mut tx, before.assignee_id.as_deref()).await?;
        let to = member_name(&mut tx, after.assignee_id.as_deref()).await?;
        format!("{} handed “{}” from {from} to {to}", actor.name, after.title)
    } else {
        let fields: Vec<String> = a.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
        if fields.is_empty() {
            format!("{} updated the checklist of “{}”", actor.name, after.title)
        } else {
            format!("{} changed {} on “{}”", actor.name, fields.join(", "), after.title)
        }
    };
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "task", entity_id: &id, group_id: after.group_id.as_deref(),
        op: if reassigned { "reassign" } else { "update" }, summary, revision: after.version, before: Some(b), after: Some(a),
    }).await?;
    if reassigned {
        if let Some(to) = &after.assignee_id {
            if to != &actor.id {
                notify(&mut tx, &state, to, "assignment", &format!("{} handed you “{}”", actor.name, after.title), Some(&id), Some(&actor.id)).await?;
            }
        }
    }
    tx.commit().await?;
    Ok(Json(after))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VersionBody {
    expected_version: Option<u32>,
}

/// Completes a task. Recurring tasks record the occurrence, keep their
/// history, rotate responsibility and move to the next due date.
pub async fn complete(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, body: Option<Json<VersionBody>>) -> AppResult<Json<CompleteTaskResult>> {
    let mut tx = state.db.begin().await?;
    let row = load_row(&mut tx, &id).await?;
    ensure_can_see(&mut tx, &row, &actor).await?;
    if let Some(v) = body.and_then(|b| b.0.expected_version) {
        if v != row.version as u32 {
            let current: Task = row.into();
            return Err(AppError::Conflict { message: "This changed in the meantime. Take another look.".into(), current: Some(serde_json::to_value(current)?) });
        }
    }
    if row.completed_at.is_some() {
        return Err(AppError::bad("This is already done."));
    }
    let now = state.now();
    let now_s = ts(now);
    let tz: Tz = row.timezone.parse().unwrap_or(Tz::UTC);
    sqlx::query("INSERT INTO task_completions (id, task_id, occurrence_due, completed_by, completed_by_name, completed_at) VALUES (?,?,?,?,?,?)")
        .bind(new_id())
        .bind(&id)
        .bind(&row.due_date)
        .bind(&actor.id)
        .bind(&actor.name)
        .bind(&now_s)
        .execute(&mut *tx)
        .await?;
    let mut next_due = None;
    let mut next_assignee_id = None;
    let summary;
    if let Some(rule) = row.recurrence.as_deref().and_then(|r| RRule::parse(r).ok()) {
        let local_now = now.with_timezone(&tz).naive_local();
        let due_time = row.due_time.as_deref().and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok()).unwrap_or(NaiveTime::MIN);
        let current_due = row.due_date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).map(|d| d.and_time(due_time));
        let next = if row.repeat_mode == "after_completion" {
            rule.advance_from(local_now.date().and_time(due_time))
        } else {
            let anchor = row.series_start.as_deref().and_then(parse_anchor).or(current_due).unwrap_or(local_now);
            // Missed occurrences are skipped, not piled up: the next one is after today.
            let after = current_due.map(|c| c.max(local_now.date().and_time(NaiveTime::MIN))).unwrap_or(local_now);
            let after = if after.date() == local_now.date() { local_now.date().and_hms_opt(23, 59, 59).unwrap_or(after) } else { after };
            rule.next_after(anchor, after, &|n| resolve_local(tz, n))
        };
        let rotation: Vec<String> = serde_json::from_str(&row.rotation).unwrap_or_default();
        let new_assignee = if rotation.is_empty() { row.assignee_id.clone() } else { next_assignee(&rotation, row.assignee_id.as_deref()) };
        let subtasks: Vec<Subtask> = serde_json::from_str::<Vec<Subtask>>(&row.subtasks)
            .unwrap_or_default()
            .into_iter()
            .map(|s| Subtask { done: false, ..s })
            .collect();
        match next {
            Some(n) => {
                let nd = n.date().format("%Y-%m-%d").to_string();
                sqlx::query("UPDATE tasks SET due_date = ?, assignee_id = ?, subtasks = ?, completion_count = completion_count + 1, version = version + 1, updated_at = ? WHERE id = ?")
                    .bind(&nd)
                    .bind(&new_assignee)
                    .bind(serde_json::to_string(&subtasks)?)
                    .bind(&now_s)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                let who = member_name(&mut tx, new_assignee.as_deref()).await?;
                summary = format!(
                    "{} finished “{}”{}. Next: {nd}{}",
                    actor.name,
                    row.title,
                    row.due_date.as_ref().map(|d| format!(" (due {d})")).unwrap_or_default(),
                    if new_assignee.is_some() && !rotation.is_empty() { format!(", {who}'s turn") } else { String::new() }
                );
                next_due = Some(nd);
                if new_assignee != row.assignee_id {
                    if let Some(a) = &new_assignee {
                        if a != &actor.id {
                            notify(&mut tx, &state, a, "assignment", &format!("Your turn: “{}”", row.title), Some(&id), Some(&actor.id)).await?;
                        }
                    }
                }
                next_assignee_id = new_assignee;
            }
            None => {
                // The series has ended (COUNT/UNTIL reached).
                sqlx::query("UPDATE tasks SET completed_at = ?, completed_by = ?, completion_count = completion_count + 1, version = version + 1, updated_at = ? WHERE id = ?")
                    .bind(&now_s)
                    .bind(&actor.id)
                    .bind(&now_s)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                summary = format!("{} finished the last “{}”", actor.name, row.title);
            }
        }
    } else {
        let column = if row.group_id.is_some() && row.column_key.is_some() { Some(DONE_COLUMN.to_string()) } else { row.column_key.clone() };
        sqlx::query("UPDATE tasks SET completed_at = ?, completed_by = ?, column_key = ?, completion_count = completion_count + 1, version = version + 1, updated_at = ? WHERE id = ?")
            .bind(&now_s)
            .bind(&actor.id)
            .bind(&column)
            .bind(&now_s)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        summary = format!("{} finished “{}”", actor.name, row.title);
    }
    let after: Task = load_row(&mut tx, &id).await?.into();
    record(&mut tx, &now_s, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "task", entity_id: &id, group_id: row.group_id.as_deref(),
        op: "complete", summary, revision: after.version,
        before: Some(json!({"dueDate": row.due_date, "assigneeId": row.assignee_id})),
        after: Some(json!({"dueDate": after.due_date, "assigneeId": after.assignee_id, "completedAt": after.completed_at})),
    }).await?;
    tx.commit().await?;
    Ok(Json(CompleteTaskResult { task: after, next_due_date: next_due, next_assignee_id }))
}

pub async fn reopen(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Task>> {
    let mut tx = state.db.begin().await?;
    let row = load_row(&mut tx, &id).await?;
    ensure_can_see(&mut tx, &row, &actor).await?;
    if row.completed_at.is_none() {
        return Err(AppError::bad("This is not marked done."));
    }
    let col = if row.column_key.as_deref() == Some(DONE_COLUMN) {
        let (_, cols) = group_mode_and_columns(&mut tx, row.group_id.as_deref().unwrap_or("")).await?;
        cols.into_iter().find(|c| c == "in_progress").or(Some("backlog".into()))
    } else {
        row.column_key.clone()
    };
    let now = ts(state.now());
    sqlx::query("UPDATE tasks SET completed_at = NULL, completed_by = NULL, column_key = ?, version = version + 1, updated_at = ? WHERE id = ?")
        .bind(&col)
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let after: Task = load_row(&mut tx, &id).await?.into();
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "task", entity_id: &id, group_id: row.group_id.as_deref(), op: "reopen",
        summary: format!("{} reopened “{}”", actor.name, row.title), revision: after.version, before: None, after: None,
    }).await?;
    tx.commit().await?;
    Ok(Json(after))
}

/// Moves a project task between board columns. Moving into "done" completes it;
/// moving out of "done" reopens it.
pub async fn move_task(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(input): Json<MoveTaskInput>) -> AppResult<Json<Task>> {
    let mut tx = state.db.begin().await?;
    let row = load_row(&mut tx, &id).await?;
    ensure_can_see(&mut tx, &row, &actor).await?;
    if row.version as u32 != input.expected_version {
        let current: Task = row.into();
        return Err(AppError::Conflict { message: "This card moved in the meantime.".into(), current: Some(serde_json::to_value(current)?) });
    }
    let g = row.group_id.clone().ok_or_else(|| AppError::bad("Only project tasks live on a board."))?;
    let (mode, cols) = group_mode_and_columns(&mut tx, &g).await?;
    if mode != GroupMode::Project {
        return Err(AppError::bad("Only project groups have a board."));
    }
    if !cols.contains(&input.column_key) {
        return Err(AppError::field("columnKey", "That column does not exist on this board."));
    }
    let now = ts(state.now());
    let into_done = input.column_key == DONE_COLUMN;
    let completed_at = if into_done { row.completed_at.clone().or_else(|| Some(now.clone())) } else { None };
    let completed_by = if into_done { row.completed_by.clone().or_else(|| Some(actor.id.clone())) } else { None };
    sqlx::query("UPDATE tasks SET column_key = ?, position = ?, completed_at = ?, completed_by = ?, version = version + 1, updated_at = ? WHERE id = ?")
        .bind(&input.column_key)
        .bind(input.position.unwrap_or(row.position))
        .bind(&completed_at)
        .bind(&completed_by)
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if into_done && row.completed_at.is_none() {
        sqlx::query("INSERT INTO task_completions (id, task_id, occurrence_due, completed_by, completed_by_name, completed_at) VALUES (?,?,?,?,?,?)")
            .bind(new_id())
            .bind(&id)
            .bind(&row.due_date)
            .bind(&actor.id)
            .bind(&actor.name)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
    }
    let after: Task = load_row(&mut tx, &id).await?.into();
    if row.column_key.as_deref() != Some(input.column_key.as_str()) {
        record(&mut tx, &now, NewActivity {
            actor: Some(&actor), source: "app", entity_type: "task", entity_id: &id, group_id: Some(&g), op: "move",
            summary: format!("{} moved “{}” to {}", actor.name, row.title, input.column_key.replace('_', " ")),
            revision: after.version, before: Some(json!({"columnKey": row.column_key})), after: Some(json!({"columnKey": input.column_key})),
        }).await?;
    }
    tx.commit().await?;
    Ok(Json(after))
}

pub async fn delete(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    let row = load_row(&mut tx, &id).await?;
    ensure_can_see(&mut tx, &row, &actor).await?;
    let now = ts(state.now());
    sqlx::query("UPDATE tasks SET deleted_at = ?, version = version + 1, updated_at = ? WHERE id = ?").bind(&now).bind(&now).bind(&id).execute(&mut *tx).await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "task", entity_id: &id, group_id: row.group_id.as_deref(), op: "delete",
        summary: format!("{} removed “{}”", actor.name, row.title), revision: row.version as u32 + 1, before: None, after: None,
    }).await?;
    tx.commit().await?;
    Ok(Json(json!({"deleted": true})))
}

pub async fn history(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<TaskHistory>> {
    let mut conn = state.db.acquire().await?;
    let row = load_row(&mut conn, &id).await?;
    ensure_can_see(&mut conn, &row, &actor).await?;
    let activity = sqlx::query_as::<_, ActivityRow>(&format!("{ACTIVITY_SELECT} WHERE entity_type = 'task' AND entity_id = ? ORDER BY at DESC LIMIT 200"))
        .bind(&id)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    let completions = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, String)>(
        "SELECT id, task_id, occurrence_due, completed_by, completed_by_name, completed_at FROM task_completions WHERE task_id = ? ORDER BY completed_at DESC LIMIT 200",
    )
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(id, task_id, occurrence_due, completed_by, completed_by_name, at)| TaskCompletion {
        id,
        task_id,
        occurrence_due,
        completed_by,
        completed_by_name,
        completed_at: parse_ts(&at),
    })
    .collect();
    Ok(Json(TaskHistory { activity, completions }))
}
