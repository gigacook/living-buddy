//! Session info, health checks and device pairing (remote mode).

use crate::db::{get_setting, ts};
use crate::error::{AppError, AppResult};
use crate::security::{RequestCtx, DEVICE_COOKIE};
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use tendly_core::api::{PairInput, ServerMode, SessionInfo};
use tendly_core::share::hash_token;

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&state.db).await {
        Ok(n) if n as usize >= crate::db::MIGRATOR.iter().count() => (StatusCode::OK, "ready"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "not ready"),
    }
}

pub async fn sharing_enabled(state: &AppState) -> bool {
    get_setting(&state.db, "sharing_enabled").await.ok().flatten().as_deref() == Some("true")
}

pub async fn session(State(state): State<AppState>, ctx: RequestCtx, headers: HeaderMap) -> AppResult<Json<SessionInfo>> {
    let paired = match crate::security::cookie_value(&headers, DEVICE_COOKIE) {
        Some(t) => lookup_device(&state, &t).await?.is_some(),
        None => false,
    };
    let demo = get_setting(&state.db, "demo_seeded").await?.is_some();
    Ok(Json(SessionInfo {
        mode: state.config.mode,
        version: env!("CARGO_PKG_VERSION").to_string(),
        admin_available: ctx.admin,
        sharing_enabled: sharing_enabled(&state).await,
        device_auth_required: state.config.mode == ServerMode::Remote,
        device_paired: paired,
        ai_configured: crate::ai::current_provider(&state).await.map(|p| p.is_configured()).unwrap_or(false),
        default_timezone: state.config.default_timezone.clone(),
        server_time: state.now(),
        demo_data: demo,
    }))
}

pub async fn lookup_device(state: &AppState, token: &str) -> anyhow::Result<Option<String>> {
    if token.len() < 20 || token.len() > 200 {
        return Ok(None);
    }
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM devices WHERE token_hash = ? AND revoked_at IS NULL")
        .bind(hash_token(token))
        .fetch_optional(&state.db)
        .await?;
    if let Some(id) = &id {
        sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?").bind(ts(state.now())).bind(id).execute(&state.db).await?;
    }
    Ok(id)
}

pub async fn pair(State(state): State<AppState>, ctx: RequestCtx, Json(input): Json<PairInput>) -> AppResult<impl IntoResponse> {
    if !state.limiter.check("pair", ctx.peer, 10, 60, state.now()) {
        return Err(AppError::RateLimited);
    }
    let code = input.code.trim();
    if lookup_device(&state, code).await?.is_none() {
        return Err(AppError::field("code", "That pairing code was not recognized."));
    }
    let secure = if state.config.cookie_secure { "; Secure" } else { "" };
    let cookie = format!("{DEVICE_COOKIE}={code}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000{secure}");
    let mut h = HeaderMap::new();
    h.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).map_err(|_| AppError::bad("Invalid code."))?);
    Ok((h, Json(serde_json::json!({"paired": true}))))
}

pub async fn unpair() -> impl IntoResponse {
    let mut h = HeaderMap::new();
    h.insert(header::SET_COOKIE, HeaderValue::from_static("tendly_device=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"));
    (h, Json(serde_json::json!({"paired": false})))
}
