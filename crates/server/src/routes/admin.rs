//! Administration: only reachable from the host itself (local/LAN modes) or
//! with the admin token on a paired device (remote mode). Never returns secrets.

use crate::connectors::{self, ConnectorRow, Credentials};
use crate::crypto::random_token;
use crate::db::{get_setting, new_id, set_setting, ts};
use crate::error::{AppError, AppResult};
use crate::security::Actor;
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tendly_core::api::{
    AdminSettings, AdminSettingsInput, AiSettings, AiSettingsInput, Connector, ConnectorInput, ConnectorPatch, ProviderInfo,
};
use tendly_core::share::hash_token;

pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<AdminSettings>> {
    let c = &state.config;
    Ok(Json(AdminSettings {
        sharing_enabled: crate::routes::session::sharing_enabled(&state).await,
        mode: c.mode,
        bind: c.bind.to_string(),
        allowed_networks: c.allowed_networks.iter().map(|n| n.to_string()).collect(),
        data_dir_display: c.data_dir.display().to_string(),
        encryption_key_source: state.cipher.source.clone(),
    }))
}

pub async fn patch_settings(
    State(state): State<AppState>,
    actor: Actor,
    Json(p): Json<AdminSettingsInput>,
) -> AppResult<Json<AdminSettings>> {
    if let Some(v) = p.sharing_enabled {
        set_setting(&state.db, "sharing_enabled", if v { "true" } else { "false" }).await?;
        let mut conn = state.db.acquire().await?;
        crate::activity::record(
            &mut conn,
            &ts(state.now()),
            crate::activity::NewActivity {
                actor: Some(&actor),
                source: "admin",
                entity_type: "share",
                entity_id: "settings",
                group_id: None,
                op: if v { "enable_sharing" } else { "disable_sharing" },
                summary: format!("{} turned sharing {}", actor.name, if v { "on" } else { "off (all links stop working)" }),
                revision: 0,
                before: None,
                after: None,
            },
        )
        .await?;
    }
    get_settings(State(state)).await
}

pub async fn get_ai(State(state): State<AppState>) -> AppResult<Json<AiSettings>> {
    Ok(Json(crate::ai::settings(&state).await?))
}

pub async fn put_ai(State(state): State<AppState>, Json(i): Json<AiSettingsInput>) -> AppResult<Json<AiSettings>> {
    if let Some(p) = &i.provider {
        if !["none", "anthropic", "openai_compatible"].contains(&p.as_str()) {
            return Err(AppError::field("provider", "Choose none, anthropic or openai_compatible."));
        }
        set_setting(&state.db, "ai_provider", p).await?;
    }
    if let Some(m) = &i.model {
        let m = tendly_core::model::clean_text(m, 100, false);
        if !m.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:/".contains(c)) {
            return Err(AppError::field("model", "Model names use letters, numbers, dashes and dots."));
        }
        set_setting(&state.db, "ai_model", &m).await?;
    }
    if let Some(b) = &i.base_url {
        let u = url::Url::parse(b.trim()).map_err(|_| AppError::field("baseUrl", "Enter a full URL like http://127.0.0.1:11434/v1."))?;
        if !matches!(u.scheme(), "http" | "https") || !u.username().is_empty() || u.password().is_some() {
            return Err(AppError::field("baseUrl", "Use an http(s) URL without credentials."));
        }
        set_setting(&state.db, "ai_base_url", u.as_str().trim_end_matches('/')).await?;
    }
    if let Some(k) = &i.api_key {
        crate::ai::store_key(&state, k).await.map_err(|e| AppError::field("apiKey", e.to_string()))?;
    }
    if i.clear_key == Some(true) {
        sqlx::query("DELETE FROM settings WHERE key = 'ai_key_ciphertext'").execute(&state.db).await?;
    }
    if let Some(n) = i.max_excerpt_chars {
        set_setting(&state.db, "ai_max_excerpt_chars", &n.clamp(200, 8000).to_string()).await?;
    }
    if let Some(v) = i.allow_paste_intake {
        set_setting(&state.db, "ai_allow_paste", if v { "true" } else { "false" }).await?;
    }
    get_ai(State(state)).await
}

pub async fn providers() -> Json<Vec<ProviderInfo>> {
    Json(connectors::providers())
}

pub async fn list_connectors(State(state): State<AppState>) -> AppResult<Json<Vec<Connector>>> {
    let rows = sqlx::query_as::<_, ConnectorRow>(&format!("{} ORDER BY created_at", connectors::SELECT)).fetch_all(&state.db).await?;
    Ok(Json(rows.iter().map(ConnectorRow::to_api).collect()))
}

async fn load(state: &AppState, id: &str) -> AppResult<ConnectorRow> {
    connectors::load(state, id).await?.ok_or(AppError::NotFound("connector"))
}

pub async fn create_connector(State(state): State<AppState>, actor: Actor, Json(i): Json<ConnectorInput>) -> AppResult<Json<Connector>> {
    connectors::ensure_known_provider(&i.provider).map_err(|e| AppError::field("provider", e.to_string()))?;
    let name = validate::title("displayName", &i.display_name, 60)?;
    let settings = i.settings.clone().unwrap_or(json!({}));
    if settings.to_string().len() > 4000 {
        return Err(AppError::field("settings", "Settings are too large."));
    }
    let id = new_id();
    let now = ts(state.now());
    sqlx::query("INSERT INTO connectors (id, provider, display_name, owner_member_id, enabled, status, settings, ai_consent, retention_days, created_at, updated_at) VALUES (?,?,?,?,0,?,?,?,?,?,?)")
        .bind(&id)
        .bind(&i.provider)
        .bind(&name)
        .bind(&actor.id)
        .bind(if matches!(i.provider.as_str(), "gmail" | "microsoft_graph" | "slack") { "needs_auth" } else { "never_run" })
        .bind(settings.to_string())
        .bind(i.ai_consent.unwrap_or(false) as i64)
        .bind(i.retention_days.unwrap_or(14).clamp(1, 365) as i64)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    Ok(Json(load(&state, &id).await?.to_api()))
}

pub async fn patch_connector(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<ConnectorPatch>,
) -> AppResult<Json<Connector>> {
    let row = load(&state, &id).await?;
    let now = ts(state.now());
    if let Some(n) = &p.display_name {
        sqlx::query("UPDATE connectors SET display_name = ? WHERE id = ?")
            .bind(validate::title("displayName", n, 60)?)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }
    if let Some(e) = p.enabled {
        // Re-enabling clears error states so the scheduler picks it up again.
        let status = if e && row.status == "error" { "never_run" } else { row.status.as_str() };
        sqlx::query("UPDATE connectors SET enabled = ?, status = ?, consecutive_failures = 0 WHERE id = ?")
            .bind(e as i64)
            .bind(status)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }
    if let Some(a) = p.ai_consent {
        sqlx::query("UPDATE connectors SET ai_consent = ? WHERE id = ?").bind(a as i64).bind(&id).execute(&state.db).await?;
    }
    if let Some(r) = p.retention_days {
        sqlx::query("UPDATE connectors SET retention_days = ? WHERE id = ?")
            .bind(r.clamp(1, 365) as i64)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }
    sqlx::query("UPDATE connectors SET updated_at = ? WHERE id = ?").bind(&now).bind(&id).execute(&state.db).await?;
    Ok(Json(load(&state, &id).await?.to_api()))
}

/// Deletes the connector, its stored items and its pending suggestions.
pub async fn delete_connector(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let row = load(&state, &id).await?;
    revoke_remote(&state, &row).await;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM suggestions WHERE connector_id = ? AND status = 'pending'").bind(&id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM connectors WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"deleted": true})))
}

async fn revoke_remote(state: &AppState, row: &ConnectorRow) {
    // Best effort: tell Google to revoke the grant. Graph and Slack tokens are removed locally.
    if row.provider == "gmail" {
        if let Ok(c) = row.credentials(state) {
            if let Some(t) = c.refresh_token.or(c.access_token) {
                let _ = state.http.post("https://oauth2.googleapis.com/revoke").form(&[("token", t)]).send().await;
            }
        }
    }
}

pub async fn disconnect(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Connector>> {
    let row = load(&state, &id).await?;
    revoke_remote(&state, &row).await;
    sqlx::query("UPDATE connectors SET credentials_ciphertext = NULL, cursor = NULL, status = 'needs_auth', enabled = 0, updated_at = ? WHERE id = ?")
        .bind(ts(state.now()))
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(load(&state, &id).await?.to_api()))
}

#[derive(Deserialize)]
pub struct TokenInput {
    token: String,
}

/// For providers that use a pasted token (Slack bot token). Stored encrypted.
pub async fn set_credentials(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(i): Json<TokenInput>,
) -> AppResult<Json<Connector>> {
    let row = load(&state, &id).await?;
    if row.provider != "slack" {
        return Err(AppError::bad("This provider connects with OAuth instead."));
    }
    let t = i.token.trim();
    if !t.starts_with("xoxb-") || t.len() > 300 {
        return Err(AppError::field("token", "Paste a Slack bot token (it starts with xoxb-)."));
    }
    connectors::save_credentials(&state, &id, &Credentials { access_token: Some(t.into()), ..Default::default() }).await?;
    sqlx::query("UPDATE connectors SET status = 'never_run' WHERE id = ?").bind(&id).execute(&state.db).await?;
    Ok(Json(load(&state, &id).await?.to_api()))
}

pub async fn run_now(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Connector>> {
    let row = load(&state, &id).await?;
    if row.enabled == 0 {
        return Err(AppError::bad("Turn the connector on first."));
    }
    let _ = connectors::run_sync(&state, &id).await;
    Ok(Json(load(&state, &id).await?.to_api()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStartQuery {
    connector_id: String,
}

fn redirect_uri(state: &AppState) -> String {
    let base = state.config.oauth.redirect_base.clone().unwrap_or_else(|| format!("http://127.0.0.1:{}", state.config.bind.port()));
    format!("{}/api/admin/oauth/callback", base.trim_end_matches('/'))
}

pub async fn oauth_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(q): Query<OAuthStartQuery>,
) -> AppResult<Json<Value>> {
    let row = load(&state, &q.connector_id).await?;
    if row.provider != provider {
        return Err(AppError::bad("Provider mismatch."));
    }
    let cfg = &state.config;
    let (verifier, challenge) = connectors::pkce_pair();
    let st = random_token(24);
    sqlx::query("INSERT INTO oauth_states (state_hash, connector_id, verifier_ciphertext, created_at) VALUES (?,?,?,?)")
        .bind(hash_token(&st))
        .bind(&row.id)
        .bind(state.cipher.encrypt(&verifier)?)
        .bind(ts(state.now()))
        .execute(&state.db)
        .await?;
    let redirect = redirect_uri(&state);
    let enc = |s: &str| -> String { url::form_urlencoded::byte_serialize(s.as_bytes()).collect() };
    let url = match provider.as_str() {
        "gmail" => {
            let id = cfg
                .oauth
                .google_client_id
                .clone()
                .ok_or_else(|| AppError::bad("Set TENDLY_GOOGLE_CLIENT_ID and TENDLY_GOOGLE_CLIENT_SECRET on the server first."))?;
            format!(
                "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}&code_challenge={}&code_challenge_method=S256",
                cfg.provider_base_overrides.google_auth,
                enc(&id),
                enc(&redirect),
                enc("https://www.googleapis.com/auth/gmail.readonly"),
                enc(&st),
                enc(&challenge)
            )
        }
        "microsoft_graph" => {
            let id =
                cfg.oauth.microsoft_client_id.clone().ok_or_else(|| {
                    AppError::bad("Set TENDLY_MICROSOFT_CLIENT_ID and TENDLY_MICROSOFT_CLIENT_SECRET on the server first.")
                })?;
            format!(
                "{}/{}/oauth2/v2.0/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
                cfg.provider_base_overrides.microsoft_login,
                cfg.oauth.microsoft_tenant,
                enc(&id),
                enc(&redirect),
                enc("offline_access Mail.Read"),
                enc(&st),
                enc(&challenge)
            )
        }
        _ => return Err(AppError::bad("This provider does not use OAuth here.")),
    };
    Ok(Json(json!({"url": url})))
}

#[derive(Deserialize)]
pub struct OAuthCallback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

pub async fn oauth_callback(State(state): State<AppState>, Query(q): Query<OAuthCallback>) -> AppResult<impl IntoResponse> {
    if q.error.is_some() {
        return Ok(Redirect::to("/settings?connector=denied"));
    }
    let (Some(st), Some(code)) = (q.state, q.code) else { return Err(AppError::bad("Missing OAuth parameters.")) };
    let row: Option<(String, String, String)> =
        sqlx::query_as("DELETE FROM oauth_states WHERE state_hash = ? RETURNING connector_id, verifier_ciphertext, created_at")
            .bind(hash_token(&st))
            .fetch_optional(&state.db)
            .await?;
    let (connector_id, verifier_ct, created) = row.ok_or(AppError::bad("This sign-in link expired. Start again."))?;
    if crate::db::parse_ts(&created) < state.now() - chrono::Duration::minutes(15) {
        return Err(AppError::bad("This sign-in link expired. Start again."));
    }
    let verifier = state.cipher.decrypt(&verifier_ct)?;
    let c = load(&state, &connector_id).await?;
    let cfg = &state.config;
    let (url, id, secret) = match c.provider.as_str() {
        "gmail" => {
            (cfg.provider_base_overrides.google_token.clone(), cfg.oauth.google_client_id.clone(), cfg.oauth.google_client_secret.clone())
        }
        _ => (
            format!("{}/{}/oauth2/v2.0/token", cfg.provider_base_overrides.microsoft_login, cfg.oauth.microsoft_tenant),
            cfg.oauth.microsoft_client_id.clone(),
            cfg.oauth.microsoft_client_secret.clone(),
        ),
    };
    let (Some(id), Some(secret)) = (id, secret) else { return Err(AppError::bad("OAuth client is not configured.")) };
    let redirect = redirect_uri(&state);
    let resp = state
        .http
        .post(url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|_| AppError::Upstream("Could not reach the sign-in service.".into()))?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream("The sign-in service rejected the request.".into()));
    }
    let v: Value = resp.json().await.map_err(|_| AppError::Upstream("Unexpected sign-in response.".into()))?;
    let creds = Credentials {
        access_token: v.get("access_token").and_then(|t| t.as_str()).map(String::from),
        refresh_token: v.get("refresh_token").and_then(|t| t.as_str()).map(String::from),
        expires_at: Some(state.now() + chrono::Duration::seconds(v.get("expires_in").and_then(|e| e.as_i64()).unwrap_or(3600))),
    };
    connectors::save_credentials(&state, &connector_id, &creds).await?;
    sqlx::query("UPDATE connectors SET status = 'never_run', cursor = NULL WHERE id = ?").bind(&connector_id).execute(&state.db).await?;
    Ok(Redirect::to("/settings?connector=connected"))
}

pub async fn jobs(State(state): State<AppState>) -> AppResult<Json<Vec<Value>>> {
    let rows: Vec<(String, String, String, i64, String, Option<String>, String)> =
        sqlx::query_as("SELECT id, kind, status, attempts, run_after, last_error, updated_at FROM jobs ORDER BY updated_at DESC LIMIT 50")
            .fetch_all(&state.db)
            .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, kind, status, attempts, run_after, err, updated)| json!({"id": id, "kind": kind, "status": status, "attempts": attempts, "runAfter": run_after, "lastError": err, "updatedAt": updated}))
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct DeviceInput {
    name: String,
}

/// Creates a pairing code for remote mode. The code is shown once.
pub async fn create_device(State(state): State<AppState>, Json(i): Json<DeviceInput>) -> AppResult<Json<Value>> {
    let name = validate::title("name", &i.name, 60)?;
    let (id, code) = create_device_token(&state, &name).await?;
    Ok(Json(json!({"id": id, "name": name, "code": code})))
}

pub async fn create_device_token(state: &AppState, name: &str) -> anyhow::Result<(String, String)> {
    let code = random_token(32);
    let id = new_id();
    sqlx::query("INSERT INTO devices (id, name, token_hash, created_at) VALUES (?,?,?,?)")
        .bind(&id)
        .bind(name)
        .bind(hash_token(&code))
        .bind(ts(state.now()))
        .execute(&state.db)
        .await?;
    Ok((id, code))
}

pub async fn list_devices(State(state): State<AppState>) -> AppResult<Json<Vec<Value>>> {
    let rows: Vec<(String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT id, name, created_at, last_seen_at, revoked_at FROM devices ORDER BY created_at DESC")
            .fetch_all(&state.db)
            .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, c, s, r)| json!({"id": id, "name": name, "createdAt": c, "lastSeenAt": s, "revokedAt": r}))
            .collect(),
    ))
}

pub async fn revoke_device(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE devices SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?")
        .bind(ts(state.now()))
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({"revoked": true})))
}

/// Full data export as JSON, without secrets (no tokens, keys, URLs or hashes).
pub async fn export(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let v = crate::backup::export_json(&state).await?;
    let mut h = axum::http::HeaderMap::new();
    h.insert(axum::http::header::CONTENT_DISPOSITION, axum::http::HeaderValue::from_static("attachment; filename=\"tendly-export.json\""));
    Ok((h, Json(v)))
}

pub async fn ai_test(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let provider = crate::ai::current_provider(&state).await?;
    if !provider.is_configured() {
        return Err(AppError::bad("No AI provider is configured."));
    }
    let msg = tendly_core::extraction::minimize(
        "Dentist appointment",
        "Reminder: your dentist appointment is on 2026-11-02 at 14:30.",
        state.now(),
        500,
    );
    match crate::ai::extract(&state, &provider, &msg).await {
        Ok(d) => Ok(Json(json!({"ok": true, "suggestions": d.len()}))),
        Err(e) => Ok(Json(json!({"ok": false, "error": tendly_core::redact::redact(&e.to_string())}))),
    }
}

pub async fn sharing_key_info(State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!({"sharingEnabled": get_setting(&state.db, "sharing_enabled").await?.as_deref() == Some("true")})))
}
