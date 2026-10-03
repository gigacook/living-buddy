//! Read-only share links (bearer capabilities) and the public share endpoints.
//!
//! * Sharing is off until an administrator enables it.
//! * Tokens have 256 bits of entropy, are shown once, and only their SHA-256
//!   hash is stored. Paths containing tokens are redacted in logs.
//! * Scopes are explicit and narrow; links can expire and be revoked.
//! * Unknown, revoked, expired and disabled links all return the same 404.

use crate::activity::{record, NewActivity};
use crate::calendar::{self, OccQuery, Viewer};
use crate::crypto::random_token;
use crate::db::{new_id, parse_ts, parse_ts_opt, ts};
use crate::error::{AppError, AppResult};
use crate::routes::groups::ensure_member;
use crate::routes::session::sharing_enabled;
use crate::security::{Actor, RequestCtx};
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use chrono::Duration;
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::json;
use tendly_core::api::{CreatedShareLink, ShareLink, ShareLinkInput};
use tendly_core::share::{hash_token, ShareScope};

#[derive(sqlx::FromRow)]
struct ShareRow {
    id: String,
    label: String,
    scope: String,
    created_by: Option<String>,
    created_at: String,
    expires_at: Option<String>,
    revoked_at: Option<String>,
    last_used_at: Option<String>,
    use_count: i64,
}

impl From<ShareRow> for ShareLink {
    fn from(r: ShareRow) -> Self {
        ShareLink {
            id: r.id,
            label: r.label,
            scope: serde_json::from_str(&r.scope).unwrap_or_default(),
            created_at: parse_ts(&r.created_at),
            created_by: r.created_by,
            expires_at: parse_ts_opt(r.expires_at),
            revoked_at: parse_ts_opt(r.revoked_at),
            last_used_at: parse_ts_opt(r.last_used_at),
            use_count: r.use_count as u32,
        }
    }
}

const SELECT: &str = "SELECT id, label, scope, created_by, created_at, expires_at, revoked_at, last_used_at, use_count FROM share_links";

pub async fn list(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Vec<ShareLink>>> {
    let rows = sqlx::query_as::<_, ShareRow>(&format!("{SELECT} ORDER BY created_at DESC")).fetch_all(&state.db).await?;
    let groups: Vec<String> =
        sqlx::query_scalar("SELECT group_id FROM group_members WHERE member_id = ?").bind(&actor.id).fetch_all(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(ShareLink::from)
            .filter(|l| l.created_by.as_deref() == Some(&actor.id) || l.scope.group_ids.iter().any(|g| groups.contains(g)))
            .collect(),
    ))
}

async fn check_scope(state: &AppState, actor: &Actor, scope: &ShareScope) -> AppResult<()> {
    scope.validate().map_err(|m| AppError::field("scope", m))?;
    for g in &scope.group_ids {
        ensure_member(state, g, actor).await?;
    }
    let visible: Vec<String> = calendar::visible_sources(state, actor).await?.into_iter().map(|s| s.id).collect();
    for s in &scope.source_ids {
        if !visible.contains(s) {
            return Err(AppError::Forbidden("You can only share calendars you can see."));
        }
    }
    if scope.include_personal && scope.member_ids.iter().any(|m| m != &actor.id) {
        return Err(AppError::Forbidden("You can only share your own personal tasks."));
    }
    Ok(())
}

pub async fn create(State(state): State<AppState>, actor: Actor, Json(input): Json<ShareLinkInput>) -> AppResult<Json<CreatedShareLink>> {
    if !sharing_enabled(&state).await {
        return Err(AppError::Forbidden("Sharing is turned off. An administrator can turn it on in Settings on the host computer."));
    }
    let label = validate::title("label", &input.label, 80)?;
    check_scope(&state, &actor, &input.scope).await?;
    let token = random_token(32);
    let id = new_id();
    let now = state.now();
    let expires = input.expires_in_days.map(|d| now + Duration::days(d.clamp(1, 366) as i64));
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO share_links (id, token_hash, label, scope, created_by, created_at, expires_at) VALUES (?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(hash_token(&token))
        .bind(&label)
        .bind(serde_json::to_string(&input.scope)?)
        .bind(&actor.id)
        .bind(ts(now))
        .bind(expires.map(ts))
        .execute(&mut *tx)
        .await?;
    record(
        &mut tx,
        &ts(now),
        NewActivity {
            actor: Some(&actor),
            source: "app",
            entity_type: "share",
            entity_id: &id,
            group_id: input.scope.group_ids.first().map(String::as_str),
            op: "publish",
            summary: format!("{} published a read-only link “{label}”", actor.name),
            revision: 1,
            before: None,
            after: Some(json!({"scope": input.scope, "expiresAt": expires})),
        },
    )
    .await?;
    tx.commit().await?;
    let link: ShareLink = sqlx::query_as::<_, ShareRow>(&format!("{SELECT} WHERE id = ?")).bind(&id).fetch_one(&state.db).await?.into();
    Ok(Json(CreatedShareLink {
        link,
        html_path: format!("/share/{token}"),
        ics_path: format!("/share/{token}/calendar.ics"),
        json_path: format!("/share/{token}/calendar.json"),
        token,
    }))
}

pub async fn revoke(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<ShareLink>> {
    let row = sqlx::query_as::<_, ShareRow>(&format!("{SELECT} WHERE id = ?"))
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("link"))?;
    let link: ShareLink = row.into();
    let in_group = {
        let mut ok = false;
        for g in &link.scope.group_ids {
            if ensure_member(&state, g, &actor).await.is_ok() {
                ok = true;
            }
        }
        ok
    };
    if link.created_by.as_deref() != Some(&actor.id) && !in_group {
        return Err(AppError::Forbidden("You can only revoke links you created or that share your groups."));
    }
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE share_links SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?").bind(&now).bind(&id).execute(&mut *tx).await?;
    record(
        &mut tx,
        &now,
        NewActivity {
            actor: Some(&actor),
            source: "app",
            entity_type: "share",
            entity_id: &id,
            group_id: link.scope.group_ids.first().map(String::as_str),
            op: "revoke",
            summary: format!("{} revoked the link “{}”", actor.name, link.label),
            revision: 2,
            before: None,
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(sqlx::query_as::<_, ShareRow>(&format!("{SELECT} WHERE id = ?")).bind(&id).fetch_one(&state.db).await?.into()))
}

// ---------------------------------------------------------------- public endpoints

fn not_found() -> Response {
    let mut res = (StatusCode::NOT_FOUND, "This link is not available.").into_response();
    crate::security::apply_security_headers(res.headers_mut(), true);
    res
}

async fn resolve(state: &AppState, ctx: &RequestCtx, token: &str) -> Option<ShareLink> {
    if !state.limiter.check("share", ctx.peer, 120, 60, state.now()) {
        return None;
    }
    if token.len() < 40 || token.len() > 64 || !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    if !sharing_enabled(state).await {
        return None;
    }
    let row = sqlx::query_as::<_, ShareRow>(&format!("{SELECT} WHERE token_hash = ?"))
        .bind(hash_token(token))
        .fetch_optional(&state.db)
        .await
        .ok()??;
    let link: ShareLink = row.into();
    let now = state.now();
    if link.revoked_at.is_some() || link.expires_at.map(|e| e <= now).unwrap_or(false) {
        return None;
    }
    let _ = sqlx::query("UPDATE share_links SET last_used_at = ?, use_count = use_count + 1 WHERE id = ?")
        .bind(ts(now))
        .bind(&link.id)
        .execute(&state.db)
        .await;
    Some(link)
}

fn share_query(state: &AppState, scope: &ShareScope) -> OccQuery {
    let now = state.now();
    OccQuery {
        from: now - Duration::days(scope.days_back as i64),
        to: now + Duration::days(scope.days_ahead as i64),
        group_ids: vec![],
        member_id: None,
        categories: vec![],
        source_ids: vec![],
        include_tasks: scope.include_tasks,
        include_events: true,
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ShareViewQuery {
    tz: Option<String>,
}

pub async fn public_html(
    State(state): State<AppState>,
    ctx: RequestCtx,
    Path(token): Path<String>,
    Query(q): Query<ShareViewQuery>,
) -> Response {
    let Some(link) = resolve(&state, &ctx, &token).await else { return not_found() };
    let occ = match calendar::occurrences(&state, Viewer::Share(&link.scope), &share_query(&state, &link.scope)).await {
        Ok(o) => o,
        Err(_) => return not_found(),
    };
    let tz: Tz = q.tz.as_deref().and_then(|t| t.parse().ok()).unwrap_or_else(|| state.config.default_timezone.parse().unwrap_or(Tz::UTC));
    let html = crate::share_render::render(&link.label, &occ, tz, &token, state.now());
    let mut res = Html(html).into_response();
    crate::security::apply_security_headers(res.headers_mut(), true);
    res
}

pub async fn public_ics(State(state): State<AppState>, ctx: RequestCtx, Path(token): Path<String>) -> Response {
    let Some(link) = resolve(&state, &ctx, &token).await else { return not_found() };
    match calendar::export_ics(&state, Viewer::Share(&link.scope), &share_query(&state, &link.scope), &link.label).await {
        Ok(body) => {
            let mut h = HeaderMap::new();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/calendar; charset=utf-8"));
            let mut res = (h, body).into_response();
            crate::security::apply_security_headers(res.headers_mut(), true);
            res
        }
        Err(_) => not_found(),
    }
}

pub async fn public_json(State(state): State<AppState>, ctx: RequestCtx, Path(token): Path<String>) -> Response {
    let Some(link) = resolve(&state, &ctx, &token).await else { return not_found() };
    match calendar::occurrences(&state, Viewer::Share(&link.scope), &share_query(&state, &link.scope)).await {
        Ok(items) => {
            let mut res = Json(json!({"name": link.label, "generatedAt": state.now(), "items": items})).into_response();
            crate::security::apply_security_headers(res.headers_mut(), true);
            res
        }
        Err(_) => not_found(),
    }
}
