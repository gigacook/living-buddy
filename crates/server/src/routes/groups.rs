//! Groups: households, partners, families, friends, roommates, project teams.

use crate::activity::{record, NewActivity};
use crate::db::{new_id, parse_ts, ts};
use crate::error::{AppError, AppResult};
use crate::security::Actor;
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;
use sqlx::SqliteConnection;
use tendly_core::api::{BoardColumn, Group, GroupInput, GroupPatch, Milestone, MilestoneInput};
use tendly_core::model::{GroupKind, GroupMode, DEFAULT_BOARD_COLUMNS, DONE_COLUMN};

#[derive(sqlx::FromRow)]
struct GroupRow {
    id: String,
    name: String,
    kind: String,
    mode: String,
    description: Option<String>,
    goal: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    columns: String,
    archived: i64,
    created_at: String,
}

const SELECT: &str = "SELECT id, name, kind, mode, description, goal, start_date, end_date, columns, archived, created_at FROM tgroups";

async fn hydrate(state: &AppState, r: GroupRow) -> AppResult<Group> {
    let member_ids: Vec<String> = sqlx::query_scalar("SELECT gm.member_id FROM group_members gm JOIN members m ON m.id = gm.member_id WHERE gm.group_id = ? ORDER BY m.created_at")
        .bind(&r.id)
        .fetch_all(&state.db)
        .await?;
    let milestones = sqlx::query_as::<_, (String, String, Option<String>, i64)>(
        "SELECT id, title, due_date, done FROM milestones WHERE group_id = ? ORDER BY COALESCE(due_date, '9999'), position",
    )
    .bind(&r.id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(id, title, due_date, done)| Milestone { id, title, due_date, done: done != 0 })
    .collect();
    Ok(Group {
        columns: serde_json::from_str(&r.columns).unwrap_or_default(),
        kind: GroupKind::parse(&r.kind).unwrap_or(GroupKind::Custom),
        mode: GroupMode::parse(&r.mode).unwrap_or(GroupMode::Household),
        id: r.id,
        name: r.name,
        description: r.description,
        goal: r.goal,
        start_date: r.start_date,
        end_date: r.end_date,
        member_ids,
        milestones,
        created_at: parse_ts(&r.created_at),
        archived: r.archived != 0,
    })
}

pub async fn load(state: &AppState, id: &str) -> AppResult<Group> {
    let row = sqlx::query_as::<_, GroupRow>(&format!("{SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("group"))?;
    hydrate(state, row).await
}

pub async fn is_member(conn: &mut SqliteConnection, group_id: &str, member_id: &str) -> AppResult<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = ? AND member_id = ?")
        .bind(group_id)
        .bind(member_id)
        .fetch_one(conn)
        .await?;
    Ok(n > 0)
}

pub async fn ensure_member(state: &AppState, group_id: &str, actor: &Actor) -> AppResult<()> {
    let mut conn = state.db.acquire().await?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tgroups WHERE id = ?").bind(group_id).fetch_one(&mut *conn).await?;
    if exists == 0 {
        return Err(AppError::NotFound("group"));
    }
    if !is_member(&mut conn, group_id, &actor.id).await? {
        return Err(AppError::Forbidden("Join this group first to see or change its items."));
    }
    Ok(())
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Group>>> {
    let rows = sqlx::query_as::<_, GroupRow>(&format!("{SELECT} ORDER BY archived, created_at")).fetch_all(&state.db).await?;
    let mut out = Vec::new();
    for r in rows {
        out.push(hydrate(&state, r).await?);
    }
    Ok(Json(out))
}

pub async fn get(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Group>> {
    Ok(Json(load(&state, &id).await?))
}

async fn validate_members(state: &AppState, ids: &[String]) -> AppResult<()> {
    for id in ids {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM members WHERE id = ?").bind(id).fetch_one(&state.db).await?;
        if n == 0 {
            return Err(AppError::field("memberIds", "Unknown person in the member list."));
        }
    }
    Ok(())
}

fn default_columns(mode: GroupMode) -> Vec<BoardColumn> {
    match mode {
        GroupMode::Project => DEFAULT_BOARD_COLUMNS
            .iter()
            .enumerate()
            .map(|(i, (k, n))| BoardColumn { key: k.to_string(), name: n.to_string(), position: i as i32 })
            .collect(),
        GroupMode::Household => vec![],
    }
}

pub async fn create(State(state): State<AppState>, actor: Actor, Json(input): Json<GroupInput>) -> AppResult<Json<Group>> {
    let name = validate::title("name", &input.name, 60)?;
    let start = validate::date("startDate", input.start_date.as_deref())?;
    let end = validate::date("endDate", input.end_date.as_deref())?;
    if let (Some(s), Some(e)) = (&start, &end) {
        if e < s {
            return Err(AppError::field("endDate", "The end date is before the start date."));
        }
    }
    let mut members = input.member_ids.clone().unwrap_or_default();
    if !members.contains(&actor.id) {
        members.insert(0, actor.id.clone());
    }
    members.dedup();
    validate_members(&state, &members).await?;
    let id = new_id();
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO tgroups (id, name, kind, mode, description, goal, start_date, end_date, columns, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(&name)
        .bind(input.kind.as_str())
        .bind(input.mode.as_str())
        .bind(validate::opt_text(input.description.as_deref(), 500, true))
        .bind(validate::opt_text(input.goal.as_deref(), 300, true))
        .bind(&start)
        .bind(&end)
        .bind(serde_json::to_string(&default_columns(input.mode))?)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    for m in &members {
        sqlx::query("INSERT OR IGNORE INTO group_members (group_id, member_id) VALUES (?, ?)").bind(&id).bind(m).execute(&mut *tx).await?;
    }
    record(
        &mut tx,
        &now,
        NewActivity {
            actor: Some(&actor),
            source: "app",
            entity_type: "group",
            entity_id: &id,
            group_id: Some(&id),
            op: "create",
            summary: format!("{} created “{name}”", actor.name),
            revision: 1,
            before: None,
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if s.is_empty() {
        "column".into()
    } else {
        s.chars().take(30).collect()
    }
}

pub async fn update(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(p): Json<GroupPatch>) -> AppResult<Json<Group>> {
    ensure_member(&state, &id, &actor).await?;
    let current = load(&state, &id).await?;
    let mut tx = state.db.begin().await?;
    let now = ts(state.now());
    if let Some(n) = &p.name {
        sqlx::query("UPDATE tgroups SET name = ? WHERE id = ?").bind(validate::title("name", n, 60)?).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(k) = p.kind {
        sqlx::query("UPDATE tgroups SET kind = ? WHERE id = ?").bind(k.as_str()).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(d) = &p.description {
        sqlx::query("UPDATE tgroups SET description = ? WHERE id = ?").bind(validate::opt_text(Some(d), 500, true)).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(g) = &p.goal {
        sqlx::query("UPDATE tgroups SET goal = ? WHERE id = ?").bind(validate::opt_text(Some(g), 300, true)).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(s) = &p.start_date {
        sqlx::query("UPDATE tgroups SET start_date = ? WHERE id = ?").bind(validate::date("startDate", Some(s))?).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(e) = &p.end_date {
        sqlx::query("UPDATE tgroups SET end_date = ? WHERE id = ?").bind(validate::date("endDate", Some(e))?).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(a) = p.archived {
        sqlx::query("UPDATE tgroups SET archived = ? WHERE id = ?").bind(a as i64).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(members) = &p.member_ids {
        if members.is_empty() {
            return Err(AppError::field("memberIds", "A group needs at least one person."));
        }
        validate_members(&state, members).await?;
        sqlx::query("DELETE FROM group_members WHERE group_id = ?").bind(&id).execute(&mut *tx).await?;
        for m in members {
            sqlx::query("INSERT OR IGNORE INTO group_members (group_id, member_id) VALUES (?, ?)").bind(&id).bind(m).execute(&mut *tx).await?;
        }
        record(&mut tx, &now, NewActivity {
            actor: Some(&actor), source: "app", entity_type: "group", entity_id: &id, group_id: Some(&id), op: "members",
            summary: format!("{} updated who is in “{}”", actor.name, current.name), revision: 0,
            before: Some(json!(current.member_ids)), after: Some(json!(members)),
        }).await?;
    }
    if let Some(cols) = &p.columns {
        if current.mode != GroupMode::Project {
            return Err(AppError::field("columns", "Only project groups have a board."));
        }
        if cols.is_empty() || cols.len() > 10 {
            return Err(AppError::field("columns", "Use between 1 and 10 columns."));
        }
        let mut out: Vec<BoardColumn> = Vec::new();
        for (i, c) in cols.iter().enumerate() {
            let name = validate::title("columns", &c.name, 30)?;
            let key = c.key.clone().map(|k| slug(&k)).unwrap_or_else(|| slug(&name));
            if out.iter().any(|o| o.key == key) {
                return Err(AppError::field("columns", "Column names must be unique."));
            }
            out.push(BoardColumn { key, name, position: i as i32 });
        }
        if !out.iter().any(|c| c.key == DONE_COLUMN) {
            return Err(AppError::field("columns", "Keep a “Done” column so finished work has a home."));
        }
        // Tasks in removed columns move to the first column rather than disappearing.
        let first = out[0].key.clone();
        let keys: Vec<String> = out.iter().map(|c| c.key.clone()).collect();
        let in_group: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, column_key FROM tasks WHERE group_id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
        for (tid, col) in in_group {
            if col.map(|c| !keys.contains(&c)).unwrap_or(true) {
                sqlx::query("UPDATE tasks SET column_key = ?, version = version + 1 WHERE id = ?").bind(&first).bind(&tid).execute(&mut *tx).await?;
            }
        }
        sqlx::query("UPDATE tgroups SET columns = ? WHERE id = ?").bind(serde_json::to_string(&out)?).bind(&id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE tgroups SET updated_at = ? WHERE id = ?").bind(&now).bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

pub async fn join(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Group>> {
    let g = load(&state, &id).await?;
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT OR IGNORE INTO group_members (group_id, member_id) VALUES (?, ?)").bind(&id).bind(&actor.id).execute(&mut *tx).await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "group", entity_id: &id, group_id: Some(&id), op: "join",
        summary: format!("{} joined “{}”", actor.name, g.name), revision: 0, before: None, after: None,
    }).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

pub async fn leave(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Group>> {
    ensure_member(&state, &id, &actor).await?;
    let g = load(&state, &id).await?;
    if g.member_ids.len() <= 1 {
        return Err(AppError::bad("You're the last person here. Archive the group instead."));
    }
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM group_members WHERE group_id = ? AND member_id = ?").bind(&id).bind(&actor.id).execute(&mut *tx).await?;
    // Unassign open items so nothing silently stays on someone who left.
    sqlx::query("UPDATE tasks SET assignee_id = NULL, version = version + 1 WHERE group_id = ? AND assignee_id = ? AND completed_at IS NULL")
        .bind(&id)
        .bind(&actor.id)
        .execute(&mut *tx)
        .await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "group", entity_id: &id, group_id: Some(&id), op: "leave",
        summary: format!("{} left “{}”; their open items are unassigned", actor.name, g.name), revision: 0, before: None, after: None,
    }).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

pub async fn add_milestone(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(input): Json<MilestoneInput>) -> AppResult<Json<Group>> {
    ensure_member(&state, &id, &actor).await?;
    let title = validate::title("title", &input.title, 120)?;
    let due = validate::date("dueDate", input.due_date.as_deref())?;
    let now = ts(state.now());
    let mid = new_id();
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO milestones (id, group_id, title, due_date, done, position, created_at) VALUES (?,?,?,?,?,(SELECT COUNT(*) FROM milestones WHERE group_id = ?),?)")
        .bind(&mid)
        .bind(&id)
        .bind(&title)
        .bind(&due)
        .bind(input.done.unwrap_or(false) as i64)
        .bind(&id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "milestone", entity_id: &mid, group_id: Some(&id), op: "create",
        summary: format!("{} added milestone “{title}”", actor.name), revision: 1, before: None, after: Some(json!({"dueDate": due})),
    }).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

pub async fn update_milestone(
    State(state): State<AppState>,
    actor: Actor,
    Path((id, mid)): Path<(String, String)>,
    Json(input): Json<MilestoneInput>,
) -> AppResult<Json<Group>> {
    ensure_member(&state, &id, &actor).await?;
    let title = validate::title("title", &input.title, 120)?;
    let due = validate::date("dueDate", input.due_date.as_deref())?;
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE milestones SET title = ?, due_date = ?, done = ? WHERE id = ? AND group_id = ?")
        .bind(&title)
        .bind(&due)
        .bind(input.done.unwrap_or(false) as i64)
        .bind(&mid)
        .bind(&id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("milestone"));
    }
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "milestone", entity_id: &mid, group_id: Some(&id), op: "update",
        summary: format!("{} updated milestone “{title}”", actor.name), revision: 0, before: None,
        after: Some(json!({"dueDate": due, "done": input.done.unwrap_or(false)})),
    }).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

pub async fn delete_milestone(State(state): State<AppState>, actor: Actor, Path((id, mid)): Path<(String, String)>) -> AppResult<Json<Group>> {
    ensure_member(&state, &id, &actor).await?;
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM milestones WHERE id = ? AND group_id = ?").bind(&mid).bind(&id).execute(&mut *tx).await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "milestone", entity_id: &mid, group_id: Some(&id), op: "delete",
        summary: format!("{} removed a milestone", actor.name), revision: 0, before: None, after: None,
    }).await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}
