//! Self-hosted mail and messaging connectors.
//!
//! Every adapter reads only what is needed (subject plus a short preview),
//! keeps tokens encrypted outside logs, uses incremental cursors, and hands
//! items to a single ingestion pipeline that de-duplicates and produces
//! reviewable suggestions. Nothing here sends messages or changes mailboxes.

use crate::db::{new_id, parse_ts, parse_ts_opt, ts};
use crate::state::AppState;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tendly_core::api::{Connector, ProviderInfo};
use tendly_core::extraction::{heuristic_extract, minimize};

#[derive(Debug, Clone)]
pub struct FetchedItem {
    pub external_id: String,
    pub subject: String,
    pub body: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct SyncResult {
    pub items: Vec<FetchedItem>,
    pub cursor: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("needs to be reconnected: {0}")]
    NeedsAuth(String),
    #[error("rate limited")]
    RateLimited(Option<u64>),
    #[error("temporary problem: {0}")]
    Transient(String),
    #[error("{0}")]
    Permanent(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Credentials {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl std::fmt::Display for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct ConnectorRow {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub owner_member_id: Option<String>,
    pub enabled: i64,
    pub status: String,
    pub settings: String,
    pub credentials_ciphertext: Option<String>,
    pub cursor: Option<String>,
    pub last_run_at: Option<String>,
    pub last_error: Option<String>,
    pub items_ingested: i64,
    pub ai_consent: i64,
    pub retention_days: i64,
    pub poll_minutes: i64,
    pub consecutive_failures: i64,
    pub created_at: String,
}

pub const SELECT: &str = "SELECT id, provider, display_name, owner_member_id, enabled, status, settings, credentials_ciphertext, cursor, last_run_at, last_error, items_ingested, ai_consent, retention_days, poll_minutes, consecutive_failures, created_at FROM connectors";

pub fn providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            provider: "fixture".into(),
            name: "Sample mailbox (synthetic)".into(),
            implementation: "implemented".into(),
            description: "Reads synthetic example messages from the server's fixture folder. Good for trying the review inbox without connecting real accounts.".into(),
            scopes: vec![],
            requires: vec![],
        },
        ProviderInfo {
            provider: "gmail".into(),
            name: "Gmail".into(),
            implementation: "implemented".into(),
            description: "Gmail API with OAuth (read-only). Reads subject and Gmail's short snippet of new messages. Requires your own Google Cloud OAuth client; unverified apps are limited to test users. Verified here against a mock API only.".into(),
            scopes: vec!["https://www.googleapis.com/auth/gmail.readonly".into()],
            requires: vec!["TENDLY_GOOGLE_CLIENT_ID".into(), "TENDLY_GOOGLE_CLIENT_SECRET".into()],
        },
        ProviderInfo {
            provider: "microsoft_graph".into(),
            name: "Outlook / Microsoft 365".into(),
            implementation: "implemented".into(),
            description: "Microsoft Graph delta queries with OAuth (Mail.Read). Reads subject and the short body preview. Requires your own Entra ID app registration. Verified here against a mock API only.".into(),
            scopes: vec!["offline_access".into(), "Mail.Read".into()],
            requires: vec!["TENDLY_MICROSOFT_CLIENT_ID".into(), "TENDLY_MICROSOFT_CLIENT_SECRET".into()],
        },
        ProviderInfo {
            provider: "slack".into(),
            name: "Slack".into(),
            implementation: "partial".into(),
            description: "Reads new messages from channels you list, using a bot token you create (channels:history). OAuth installation flow is not built yet; paste a bot token instead. Verified here against a mock API only.".into(),
            scopes: vec!["channels:history".into()],
            requires: vec!["Slack bot token".into()],
        },
        ProviderInfo {
            provider: "proton_bridge".into(),
            name: "Proton Mail (via Proton Mail Bridge)".into(),
            implementation: "scaffolded".into(),
            description: "Proton does not offer a Gmail-style API. The supported route is the Proton Mail Bridge app on your own machine, which exposes local IMAP. The IMAP reader is not implemented in this version.".into(),
            scopes: vec![],
            requires: vec!["Proton Mail Bridge (paid Proton plan)".into(), "IMAP adapter (not yet implemented)".into()],
        },
    ]
}

fn implementation_of(provider: &str) -> String {
    providers().into_iter().find(|p| p.provider == provider).map(|p| p.implementation).unwrap_or_else(|| "unknown".into())
}

impl ConnectorRow {
    pub fn to_api(&self) -> Connector {
        Connector {
            id: self.id.clone(),
            provider: self.provider.clone(),
            display_name: self.display_name.clone(),
            enabled: self.enabled != 0,
            status: self.status.clone(),
            last_run_at: parse_ts_opt(self.last_run_at.clone()),
            last_error: self.last_error.clone(),
            has_credentials: self.credentials_ciphertext.is_some(),
            has_cursor: self.cursor.is_some(),
            items_ingested: self.items_ingested as u32,
            ai_consent: self.ai_consent != 0,
            retention_days: self.retention_days as u32,
            implementation: implementation_of(&self.provider),
            scopes: providers().into_iter().find(|p| p.provider == self.provider).map(|p| p.scopes).unwrap_or_default(),
            created_at: parse_ts(&self.created_at),
        }
    }

    pub fn credentials(&self, state: &AppState) -> Result<Credentials> {
        match &self.credentials_ciphertext {
            Some(c) => Ok(serde_json::from_str(&state.cipher.decrypt(c)?)?),
            None => Ok(Credentials::default()),
        }
    }

    pub fn settings_json(&self) -> Value {
        serde_json::from_str(&self.settings).unwrap_or(json!({}))
    }
}

pub async fn load(state: &AppState, id: &str) -> Result<Option<ConnectorRow>> {
    Ok(sqlx::query_as::<_, ConnectorRow>(&format!("{SELECT} WHERE id = ?")).bind(id).fetch_optional(&state.db).await?)
}

pub async fn save_credentials(state: &AppState, id: &str, creds: &Credentials) -> Result<()> {
    sqlx::query("UPDATE connectors SET credentials_ciphertext = ?, updated_at = ? WHERE id = ?")
        .bind(state.cipher.encrypt(&serde_json::to_string(creds)?)?)
        .bind(ts(state.now()))
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

fn http_err(status: reqwest::StatusCode, retry_after: Option<u64>) -> ConnectorError {
    match status.as_u16() {
        401 | 403 => ConnectorError::NeedsAuth(format!("provider answered {}", status.as_u16())),
        429 => ConnectorError::RateLimited(retry_after),
        500..=599 => ConnectorError::Transient(format!("provider answered {}", status.as_u16())),
        s => ConnectorError::Permanent(format!("provider answered {s}")),
    }
}

fn retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok())
}

async fn get_json(state: &AppState, url: &str, token: &str) -> Result<Value, ConnectorError> {
    let resp = state
        .http
        .get(url)
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| ConnectorError::Transient("network error".into()))?;
    if !resp.status().is_success() {
        let ra = retry_after(&resp);
        return Err(http_err(resp.status(), ra));
    }
    resp.json().await.map_err(|_| ConnectorError::Transient("invalid response".into()))
}

/// Refreshes an OAuth access token if it is close to expiry.
async fn ensure_token(state: &AppState, row: &ConnectorRow, force: bool) -> Result<String, ConnectorError> {
    let mut creds = row.credentials(state).map_err(|_| ConnectorError::NeedsAuth("stored credentials unreadable".into()))?;
    let fresh = creds.expires_at.map(|e| e > state.now() + Duration::seconds(60)).unwrap_or(true);
    if !force && fresh {
        if let Some(t) = &creds.access_token {
            return Ok(t.clone());
        }
    }
    let refresh = creds.refresh_token.clone().ok_or_else(|| ConnectorError::NeedsAuth("no refresh token".into()))?;
    let cfg = &state.config;
    let (url, client_id, client_secret) = match row.provider.as_str() {
        "gmail" => (cfg.provider_base_overrides.google_token.clone(), cfg.oauth.google_client_id.clone(), cfg.oauth.google_client_secret.clone()),
        "microsoft_graph" => (
            format!("{}/{}/oauth2/v2.0/token", cfg.provider_base_overrides.microsoft_login, cfg.oauth.microsoft_tenant),
            cfg.oauth.microsoft_client_id.clone(),
            cfg.oauth.microsoft_client_secret.clone(),
        ),
        _ => return Err(ConnectorError::NeedsAuth("provider does not refresh".into())),
    };
    let (Some(id), Some(secret)) = (client_id, client_secret) else {
        return Err(ConnectorError::NeedsAuth("OAuth client is not configured on the server".into()));
    };
    let resp = state
        .http
        .post(url)
        .form(&[("grant_type", "refresh_token"), ("refresh_token", refresh.as_str()), ("client_id", id.as_str()), ("client_secret", secret.as_str())])
        .send()
        .await
        .map_err(|_| ConnectorError::Transient("token refresh failed".into()))?;
    if !resp.status().is_success() {
        // invalid_grant means the person revoked access or the token expired.
        return Err(ConnectorError::NeedsAuth("access was revoked or expired".into()));
    }
    let v: Value = resp.json().await.map_err(|_| ConnectorError::Transient("invalid token response".into()))?;
    let access = v.get("access_token").and_then(|t| t.as_str()).ok_or_else(|| ConnectorError::NeedsAuth("no access token".into()))?;
    creds.access_token = Some(access.to_string());
    if let Some(r) = v.get("refresh_token").and_then(|t| t.as_str()) {
        creds.refresh_token = Some(r.to_string());
    }
    creds.expires_at = Some(state.now() + Duration::seconds(v.get("expires_in").and_then(|e| e.as_i64()).unwrap_or(3600)));
    save_credentials(state, &row.id, &creds).await.map_err(|e| ConnectorError::Transient(e.to_string()))?;
    Ok(access.to_string())
}

async fn with_auth_retry<F, Fut>(state: &AppState, row: &ConnectorRow, f: F) -> Result<SyncResult, ConnectorError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<SyncResult, ConnectorError>>,
{
    let token = ensure_token(state, row, false).await?;
    match f(token).await {
        Err(ConnectorError::NeedsAuth(_)) => {
            let token = ensure_token(state, row, true).await?;
            f(token).await
        }
        other => other,
    }
}

pub async fn sync(state: &AppState, row: &ConnectorRow) -> Result<SyncResult, ConnectorError> {
    match row.provider.as_str() {
        "fixture" => sync_fixture(state, row),
        "gmail" => with_auth_retry(state, row, |t| sync_gmail(state, row.cursor.clone(), t)).await,
        "microsoft_graph" => with_auth_retry(state, row, |t| sync_graph(state, row.cursor.clone(), t)).await,
        "slack" => sync_slack(state, row).await,
        "proton_bridge" => Err(ConnectorError::Permanent("The Proton Mail Bridge (IMAP) adapter is not implemented yet.".into())),
        other => Err(ConnectorError::Permanent(format!("Unknown provider {other}"))),
    }
}

// ---------------------------------------------------------------- fixture

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureMessage {
    id: String,
    subject: String,
    body: String,
    received_at: DateTime<Utc>,
}

pub fn fixture_root(state: &AppState) -> PathBuf {
    state.config.fixture_dir.clone().unwrap_or_else(|| PathBuf::from("integrations/fixtures/mail"))
}

fn sync_fixture(state: &AppState, row: &ConnectorRow) -> Result<SyncResult, ConnectorError> {
    let set = row.settings_json().get("set").and_then(|s| s.as_str()).unwrap_or("sample").to_string();
    if !set.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || set.is_empty() {
        return Err(ConnectorError::Permanent("Invalid fixture set name.".into()));
    }
    let dir = fixture_root(state).join(&set);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|_| ConnectorError::Permanent(format!("Fixture set “{set}” not found on the server.")))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    files.sort();
    let mut items = Vec::new();
    let mut cursor = row.cursor.clone();
    for f in files {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if row.cursor.as_deref().map(|c| name.as_str() <= c).unwrap_or(false) {
            continue;
        }
        let text = std::fs::read_to_string(&f).map_err(|e| ConnectorError::Transient(e.to_string()))?;
        let msg: FixtureMessage = serde_json::from_str(&text).map_err(|_| ConnectorError::Permanent(format!("Fixture {name} is malformed.")))?;
        items.push(FetchedItem { external_id: msg.id, subject: msg.subject, body: msg.body, received_at: msg.received_at });
        cursor = Some(name);
        if items.len() >= 50 {
            break;
        }
    }
    Ok(SyncResult { items, cursor })
}

// ---------------------------------------------------------------- Gmail

async fn sync_gmail(state: &AppState, cursor: Option<String>, token: String) -> Result<SyncResult, ConnectorError> {
    let base = format!("{}/gmail/v1/users/me", state.config.provider_base_overrides.gmail.trim_end_matches('/'));
    let mut ids: Vec<String> = Vec::new();
    let mut new_cursor = cursor.clone();
    let mut need_initial = cursor.is_none();
    if let Some(c) = &cursor {
        match get_json(state, &format!("{base}/history?startHistoryId={}&historyTypes=messageAdded&maxResults=100", urlencode(c)), &token).await {
            Ok(v) => {
                for h in v.get("history").and_then(|h| h.as_array()).into_iter().flatten() {
                    for m in h.get("messagesAdded").and_then(|m| m.as_array()).into_iter().flatten() {
                        if let Some(id) = m.pointer("/message/id").and_then(|i| i.as_str()) {
                            ids.push(id.to_string());
                        }
                    }
                }
                new_cursor = v.get("historyId").and_then(|h| h.as_str().map(String::from).or_else(|| h.as_u64().map(|n| n.to_string()))).or(new_cursor);
            }
            // An expired history id means a fresh, bounded resync.
            Err(ConnectorError::Permanent(m)) if m.contains("404") => need_initial = true,
            Err(e) => return Err(e),
        }
    }
    if need_initial {
        let list = get_json(state, &format!("{base}/messages?maxResults=20&q={}", urlencode("newer_than:3d -category:promotions -category:social")), &token).await?;
        ids = list.get("messages").and_then(|m| m.as_array()).into_iter().flatten().filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from)).collect();
        let profile = get_json(state, &format!("{base}/profile"), &token).await?;
        new_cursor = profile.get("historyId").and_then(|h| h.as_str().map(String::from).or_else(|| h.as_u64().map(|n| n.to_string())));
    }
    ids.dedup();
    let mut items = Vec::new();
    for id in ids.into_iter().take(50) {
        let m = get_json(state, &format!("{base}/messages/{}?format=metadata&metadataHeaders=Subject", urlencode(&id)), &token).await?;
        let subject = m
            .pointer("/payload/headers")
            .and_then(|h| h.as_array())
            .and_then(|hs| hs.iter().find(|h| h.get("name").and_then(|n| n.as_str()).map(|n| n.eq_ignore_ascii_case("subject")).unwrap_or(false)))
            .and_then(|h| h.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let snippet = m.get("snippet").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let received = m
            .get("internalDate")
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<i64>().ok())
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
            .unwrap_or_else(|| state.now());
        items.push(FetchedItem { external_id: id, subject, body: snippet, received_at: received });
    }
    Ok(SyncResult { items, cursor: new_cursor })
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---------------------------------------------------------------- Microsoft Graph

async fn sync_graph(state: &AppState, cursor: Option<String>, token: String) -> Result<SyncResult, ConnectorError> {
    let base = state.config.provider_base_overrides.graph.trim_end_matches('/').to_string();
    let mut url = match &cursor {
        // Only follow delta links that point at the configured Graph host.
        Some(c) if c.starts_with(&base) => c.clone(),
        _ => format!("{base}/me/mailFolders/inbox/messages/delta?$select=subject,bodyPreview,receivedDateTime&$top=20"),
    };
    let mut items = Vec::new();
    for _ in 0..5 {
        let v = get_json(state, &url, &token).await?;
        for m in v.get("value").and_then(|x| x.as_array()).into_iter().flatten() {
            if m.get("@removed").is_some() {
                continue;
            }
            let Some(id) = m.get("id").and_then(|i| i.as_str()) else { continue };
            items.push(FetchedItem {
                external_id: id.to_string(),
                subject: m.get("subject").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                body: m.get("bodyPreview").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                received_at: m.get("receivedDateTime").and_then(|d| d.as_str()).map(parse_ts).filter(|d| *d > DateTime::<Utc>::MIN_UTC).unwrap_or_else(|| state.now()),
            });
        }
        if let Some(next) = v.get("@odata.nextLink").and_then(|n| n.as_str()) {
            if !next.starts_with(&base) {
                return Err(ConnectorError::Permanent("unexpected paging link".into()));
            }
            url = next.to_string();
            continue;
        }
        if let Some(delta) = v.get("@odata.deltaLink").and_then(|n| n.as_str()) {
            if !delta.starts_with(&base) {
                return Err(ConnectorError::Permanent("unexpected delta link".into()));
            }
            return Ok(SyncResult { items, cursor: Some(delta.to_string()) });
        }
        break;
    }
    Ok(SyncResult { items, cursor })
}

// ---------------------------------------------------------------- Slack

async fn sync_slack(state: &AppState, row: &ConnectorRow) -> Result<SyncResult, ConnectorError> {
    let token = row
        .credentials(state)
        .ok()
        .and_then(|c| c.access_token)
        .ok_or_else(|| ConnectorError::NeedsAuth("add a Slack bot token".into()))?;
    let channels: Vec<String> = row
        .settings_json()
        .get("channels")
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|c| c.as_str()).filter(|c| c.chars().all(|x| x.is_ascii_alphanumeric())).map(String::from).take(20).collect())
        .unwrap_or_default();
    if channels.is_empty() {
        return Err(ConnectorError::Permanent("List at least one channel id in the connector settings.".into()));
    }
    let mut cursors: serde_json::Map<String, Value> = row.cursor.as_deref().and_then(|c| serde_json::from_str(c).ok()).unwrap_or_default();
    let base = state.config.provider_base_overrides.slack.trim_end_matches('/').to_string();
    let mut items = Vec::new();
    for ch in channels {
        let oldest = cursors.get(&ch).and_then(|v| v.as_str()).unwrap_or("0").to_string();
        let v = get_json(state, &format!("{base}/conversations.history?channel={ch}&oldest={}&limit=50", urlencode(&oldest)), &token).await?;
        if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
            let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
            return Err(match err {
                "invalid_auth" | "token_revoked" | "not_authed" | "account_inactive" => ConnectorError::NeedsAuth(err.into()),
                "ratelimited" => ConnectorError::RateLimited(None),
                other => ConnectorError::Permanent(format!("Slack error: {other}")),
            });
        }
        let mut newest = oldest.clone();
        for m in v.get("messages").and_then(|m| m.as_array()).into_iter().flatten() {
            let Some(tsv) = m.get("ts").and_then(|t| t.as_str()) else { continue };
            if tsv.parse::<f64>().unwrap_or(0.0) > newest.parse::<f64>().unwrap_or(0.0) {
                newest = tsv.to_string();
            }
            if m.get("subtype").is_some() {
                continue; // joins, bot notices, edits
            }
            let secs = tsv.split('.').next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            items.push(FetchedItem {
                external_id: format!("{ch}:{tsv}"),
                subject: format!("Slack message in {ch}"),
                body: m.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                received_at: Utc.timestamp_opt(secs, 0).single().unwrap_or_else(|| state.now()),
            });
        }
        cursors.insert(ch, Value::String(newest));
    }
    Ok(SyncResult { items, cursor: Some(Value::Object(cursors).to_string()) })
}

// ---------------------------------------------------------------- ingestion

fn external_hash(connector_id: &str, external_id: &str) -> String {
    hex::encode(Sha256::digest(format!("{connector_id}\u{0}{external_id}").as_bytes()))
}

/// De-duplicates items and turns new ones into pending suggestions. Returns
/// the number of new items.
pub async fn ingest(state: &AppState, row: &ConnectorRow, items: &[FetchedItem]) -> Result<u32> {
    let provider = crate::ai::current_provider(state).await?;
    let max_chars: usize = crate::db::get_setting(&state.db, "ai_max_excerpt_chars").await?.and_then(|v| v.parse().ok()).unwrap_or(2000);
    let use_ai = row.ai_consent != 0 && provider.is_configured();
    let mut new_items = 0;
    for item in items {
        let now = state.now();
        let purge = ts(now + Duration::days(row.retention_days.clamp(1, 365)));
        let msg = minimize(&item.subject, &item.body, item.received_at, max_chars);
        let item_id = new_id();
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO ingested_items (id, connector_id, external_hash, subject, excerpt, received_at, ingested_at, purge_after) VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(&item_id)
        .bind(&row.id)
        .bind(external_hash(&row.id, &item.external_id))
        .bind(&msg.subject)
        .bind(&msg.excerpt)
        .bind(ts(item.received_at))
        .bind(ts(now))
        .bind(&purge)
        .execute(&state.db)
        .await?
        .rows_affected();
        if inserted == 0 {
            continue; // already ingested: idempotent across retries and overlapping cursors
        }
        new_items += 1;
        let (drafts, extractor) = if use_ai {
            match crate::ai::extract(state, &provider, &msg).await {
                Ok(d) => (d, provider.label()),
                Err(e) => {
                    tracing::warn!(connector = %row.id, error = %tendly_core::redact::redact(&e.to_string()), "AI extraction failed; using local rules");
                    (heuristic_extract(&msg), "rules (AI unavailable)".to_string())
                }
            }
        } else {
            (heuristic_extract(&msg), "rules".to_string())
        };
        for d in drafts {
            sqlx::query("INSERT INTO suggestions (id, item_id, connector_id, connector_name, member_id, subject, received_at, draft, extractor, status, created_at, purge_after) VALUES (?,?,?,?,?,?,?,?,?,'pending',?,?)")
                .bind(new_id())
                .bind(&item_id)
                .bind(&row.id)
                .bind(&row.display_name)
                .bind(&row.owner_member_id)
                .bind(&msg.subject)
                .bind(ts(item.received_at))
                .bind(serde_json::to_string(&d)?)
                .bind(&extractor)
                .bind(ts(now))
                .bind(&purge)
                .execute(&state.db)
                .await?;
        }
    }
    sqlx::query("UPDATE connectors SET items_ingested = items_ingested + ? WHERE id = ?").bind(new_items as i64).bind(&row.id).execute(&state.db).await?;
    Ok(new_items)
}

/// One full sync: fetch, ingest, then advance the cursor (only after ingest succeeds).
pub async fn run_sync(state: &AppState, connector_id: &str) -> Result<(), ConnectorError> {
    let row = load(state, connector_id).await.map_err(|e| ConnectorError::Transient(e.to_string()))?.ok_or_else(|| ConnectorError::Permanent("connector removed".into()))?;
    if row.enabled == 0 {
        return Ok(());
    }
    let now = ts(state.now());
    let result = sync(state, &row).await;
    match result {
        Ok(res) => {
            ingest(state, &row, &res.items).await.map_err(|e| ConnectorError::Transient(e.to_string()))?;
            sqlx::query("UPDATE connectors SET cursor = ?, status = 'healthy', last_error = NULL, last_run_at = ?, consecutive_failures = 0, updated_at = ? WHERE id = ?")
                .bind(&res.cursor)
                .bind(&now)
                .bind(&now)
                .bind(&row.id)
                .execute(&state.db)
                .await
                .map_err(|e| ConnectorError::Transient(e.to_string()))?;
            Ok(())
        }
        Err(e) => {
            let status = match &e {
                ConnectorError::NeedsAuth(_) => "needs_auth",
                ConnectorError::RateLimited(_) => "rate_limited",
                ConnectorError::Transient(_) => "degraded",
                ConnectorError::Permanent(_) => "error",
            };
            let _ = sqlx::query("UPDATE connectors SET status = ?, last_error = ?, last_run_at = ?, consecutive_failures = consecutive_failures + 1, updated_at = ? WHERE id = ?")
                .bind(status)
                .bind(tendly_core::redact::redact(&e.to_string()))
                .bind(&now)
                .bind(&now)
                .bind(&row.id)
                .execute(&state.db)
                .await;
            Err(e)
        }
    }
}

/// Deletes stored excerpts and old suggestions after their retention period.
pub async fn apply_retention(state: &AppState) -> Result<(u64, u64)> {
    let now = ts(state.now());
    let items = sqlx::query("DELETE FROM ingested_items WHERE purge_after <= ?").bind(&now).execute(&state.db).await?.rows_affected();
    let sugg = sqlx::query("DELETE FROM suggestions WHERE purge_after <= ? AND status != 'pending'").bind(&now).execute(&state.db).await?.rows_affected();
    Ok((items, sugg))
}

pub fn fixture_dir_exists(p: &Path) -> bool {
    p.is_dir()
}

pub fn pkce_pair() -> (String, String) {
    let verifier = crate::crypto::random_token(48);
    let challenge = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub fn ensure_known_provider(p: &str) -> Result<()> {
    if providers().iter().any(|x| x.provider == p) {
        Ok(())
    } else {
        Err(anyhow!("Unknown provider."))
    }
}
