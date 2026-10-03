//! Name-only identities. A display name is attribution, not authentication;
//! stable internal ids keep assignments intact when names change or repeat.

use crate::activity::{record, NewActivity};
use crate::db::{new_id, parse_ts, ts};
use crate::error::{AppError, AppResult};
use crate::security::{Actor, RequestCtx};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use tendly_core::api::{Member, MemberInput, MemberPatch, MemberPrefs};
use tendly_core::model::normalize_display_name;

pub const COLORS: &[&str] = &["mint", "peach", "lilac", "sky", "butter", "rose"];

#[derive(sqlx::FromRow)]
struct MemberRow {
    id: String,
    display_name: String,
    color: String,
    prefs: String,
    created_at: String,
}

impl From<MemberRow> for Member {
    fn from(r: MemberRow) -> Self {
        Member {
            id: r.id,
            display_name: r.display_name,
            color: r.color,
            prefs: serde_json::from_str(&r.prefs).unwrap_or_default(),
            created_at: parse_ts(&r.created_at),
        }
    }
}

pub async fn load(state: &AppState, id: &str) -> AppResult<Member> {
    sqlx::query_as::<_, MemberRow>("SELECT id, display_name, color, prefs, created_at FROM members WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .map(Member::from)
        .ok_or(AppError::NotFound("person"))
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Member>>> {
    let rows = sqlx::query_as::<_, MemberRow>("SELECT id, display_name, color, prefs, created_at FROM members ORDER BY created_at")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(Member::from).collect()))
}

pub async fn create(State(state): State<AppState>, ctx: RequestCtx, Json(input): Json<MemberInput>) -> AppResult<Json<Member>> {
    if !state.limiter.check("member_create", ctx.peer, 30, 3600, state.now()) {
        return Err(AppError::RateLimited);
    }
    let name = normalize_display_name(&input.display_name).map_err(|m| AppError::field("displayName", m))?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM members").fetch_one(&state.db).await?;
    if count >= 200 {
        return Err(AppError::bad("This space already has the maximum number of people."));
    }
    let tz = crate::validate::timezone(input.timezone.as_deref(), &state.config.default_timezone)?;
    let prefs = MemberPrefs { timezone: tz, ..MemberPrefs::default() };
    let id = new_id();
    let now = ts(state.now());
    let color = COLORS[(count as usize) % COLORS.len()];
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO members (id, display_name, color, prefs, created_at, updated_at) VALUES (?,?,?,?,?,?)")
        .bind(&id)
        .bind(&name)
        .bind(color)
        .bind(serde_json::to_string(&prefs)?)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    let actor = Actor { id: id.clone(), name: name.clone() };
    record(
        &mut tx,
        &now,
        NewActivity {
            actor: Some(&actor),
            source: "app",
            entity_type: "member",
            entity_id: &id,
            group_id: None,
            op: "join",
            summary: format!("{name} joined"),
            revision: 1,
            before: None,
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}

pub async fn update(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(p): Json<MemberPatch>) -> AppResult<Json<Member>> {
    // People edit their own profile; names are not a security boundary, but this avoids accidents.
    if actor.id != id {
        return Err(AppError::Forbidden("You can only change your own profile."));
    }
    let current = load(&state, &id).await?;
    let name = match &p.display_name {
        Some(n) => normalize_display_name(n).map_err(|m| AppError::field("displayName", m))?,
        None => current.display_name.clone(),
    };
    let color = match &p.color {
        Some(c) if COLORS.contains(&c.as_str()) => c.clone(),
        Some(_) => return Err(AppError::field("color", "Unknown color.")),
        None => current.color.clone(),
    };
    let prefs = match p.prefs {
        Some(mut pr) => {
            pr.timezone = crate::validate::timezone(Some(&pr.timezone), &state.config.default_timezone)?;
            pr.quiet_start = crate::validate::time("quietStart", pr.quiet_start.as_deref())?;
            pr.quiet_end = crate::validate::time("quietEnd", pr.quiet_end.as_deref())?;
            pr
        }
        None => current.prefs.clone(),
    };
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE members SET display_name = ?, color = ?, prefs = ?, updated_at = ? WHERE id = ?")
        .bind(&name)
        .bind(&color)
        .bind(serde_json::to_string(&prefs)?)
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if name != current.display_name {
        record(
            &mut tx,
            &now,
            NewActivity {
                actor: Some(&actor),
                source: "app",
                entity_type: "member",
                entity_id: &id,
                group_id: None,
                op: "rename",
                summary: format!("{} is now called {name}", current.display_name),
                revision: 0,
                before: Some(serde_json::json!({"displayName": current.display_name})),
                after: Some(serde_json::json!({"displayName": name})),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(load(&state, &id).await?))
}
