//! Request guards: network boundary, Host validation (DNS rebinding), CSRF,
//! device authentication in remote mode, admin boundary and actor attribution.

use crate::crypto::ct_eq;
use crate::db::ts;
use crate::error::AppError;
use crate::state::AppState;
use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::{IpAddr, SocketAddr};
use tendly_core::api::ServerMode;

pub const CSRF_HEADER: &str = "x-tendly-csrf";
pub const ACTOR_HEADER: &str = "x-tendly-actor";
pub const ADMIN_HEADER: &str = "x-tendly-admin-token";
pub const DEVICE_COOKIE: &str = "tendly_device";

#[derive(Clone, Debug)]
pub struct Actor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct RequestCtx {
    pub peer: IpAddr,
    /// Loopback peer with no proxy forwarding headers.
    pub trusted_local: bool,
    pub device_id: Option<String>,
    pub actor: Option<Actor>,
    pub admin: bool,
}

fn host_only(raw: &str) -> String {
    let raw = raw.trim().to_ascii_lowercase();
    if raw.starts_with('[') {
        return raw.split(']').next().map(|h| format!("{h}]")).unwrap_or(raw);
    }
    raw.split(':').next().unwrap_or("").to_string()
}

fn origin_host(origin: &str) -> Option<String> {
    let rest = origin.split_once("://")?.1;
    Some(host_only(rest.split('/').next()?))
}

fn has_forwarding_headers(h: &HeaderMap) -> bool {
    ["x-forwarded-for", "forwarded", "x-real-ip", "x-forwarded-host"].iter().any(|k| h.contains_key(*k))
}

pub fn cookie_value(h: &HeaderMap, name: &str) -> Option<String> {
    h.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

fn deny(status: axum::http::StatusCode, code: &str, msg: &str) -> Response {
    (status, axum::Json(tendly_core::api::ApiError { code: code.into(), message: msg.into(), field: None, current: None })).into_response()
}

pub async fn guard(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    use axum::http::StatusCode;
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        // No connection info means we cannot prove locality: treat as untrusted.
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    let cfg = &state.config;
    let headers = req.headers().clone();
    let trusted_local = peer.is_loopback() && !has_forwarding_headers(&headers);

    // 1. Network boundary.
    let peer_ok = match cfg.mode {
        ServerMode::Local => peer.is_loopback(),
        ServerMode::Lan => peer.is_loopback() || cfg.allowed_networks.iter().any(|n| n.contains(&peer)),
        ServerMode::Remote => true,
    };
    if !peer_ok {
        return deny(StatusCode::FORBIDDEN, "network_not_allowed", "This network is not allowed to reach Tendly.");
    }

    // 2. Host header validation protects against DNS rebinding.
    let allowed_hosts = cfg.effective_allowed_hosts();
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).map(host_only);
    match &host {
        Some(h) if allowed_hosts.iter().any(|a| a == h) => {}
        _ => return deny(StatusCode::MISDIRECTED_REQUEST, "host_not_allowed", "Unrecognized host name. Add it to TENDLY_ALLOWED_HOSTS if this is intended."),
    }

    let path = req.uri().path().to_string();
    let is_api = path.starts_with("/api/");
    let unsafe_method = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);

    // 3. CSRF: state-changing API calls need a custom header (forces a CORS
    // preflight we never approve) and, when present, a same-site Origin.
    if is_api && unsafe_method {
        if headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok()) != Some("1") {
            return deny(StatusCode::FORBIDDEN, "csrf", "Missing request header. Please reload the app.");
        }
        if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
            let ok = origin_host(origin).map(|h| allowed_hosts.contains(&h)).unwrap_or(false)
                || origin == "tauri://localhost"
                || origin == "http://tauri.localhost"
                || origin == "https://tauri.localhost";
            if !ok {
                return deny(StatusCode::FORBIDDEN, "csrf", "Cross-site requests are not allowed.");
            }
        }
    }

    // 4. Device authentication in remote mode.
    let mut device_id = None;
    if cfg.mode == ServerMode::Remote && is_api && !matches!(path.as_str(), "/api/session" | "/api/auth/pair") {
        let token = cookie_value(&headers, DEVICE_COOKIE);
        let found = match token {
            Some(t) => crate::routes::session::lookup_device(&state, &t).await.ok().flatten(),
            None => None,
        };
        match found {
            Some(id) => device_id = Some(id),
            None => return AppError::Unauthorized("device_auth_required").into_response(),
        }
    }

    // 5. Admin boundary.
    let admin = match cfg.mode {
        ServerMode::Local | ServerMode::Lan => trusted_local,
        ServerMode::Remote => {
            let presented = headers.get(ADMIN_HEADER).and_then(|v| v.to_str().ok()).unwrap_or("");
            cfg.admin_token.as_deref().map(|t| !presented.is_empty() && ct_eq(t.as_bytes(), presented.as_bytes())).unwrap_or(false)
                && device_id.is_some()
        }
    };
    if path.starts_with("/api/admin/") && !admin {
        return deny(
            StatusCode::FORBIDDEN,
            "admin_only",
            "Administration is only available on this computer (or with the admin token in remote mode).",
        );
    }

    // 6. Actor attribution (names are attribution, not authentication).
    let actor = match headers.get(ACTOR_HEADER).and_then(|v| v.to_str().ok()) {
        Some(id) if id.len() <= 64 => sqlx::query_as::<_, (String, String)>("SELECT id, display_name FROM members WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|(id, name)| Actor { id, name }),
        _ => None,
    };

    req.extensions_mut().insert(RequestCtx { peer, trusted_local, device_id, actor, admin });
    let started = std::time::Instant::now();
    let method = req.method().clone();
    let mut res = next.run(req).await;
    tracing::info!(
        target: "tendly::http",
        method = %method,
        path = %tendly_core::redact::redact(&path),
        status = res.status().as_u16(),
        ms = started.elapsed().as_millis() as u64,
    );
    apply_security_headers(res.headers_mut(), path.starts_with("/share/"));
    res
}

pub fn apply_security_headers(h: &mut HeaderMap, share_page: bool) {
    let csp = if share_page {
        "default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    } else {
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost; font-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
    };
    let pairs = [
        ("content-security-policy", csp),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("permissions-policy", "camera=(), microphone=(), geolocation=(), payment=()"),
        ("cross-origin-opener-policy", "same-origin"),
    ];
    for (k, v) in pairs {
        h.entry(k).or_insert(HeaderValue::from_static(v));
    }
    if share_page {
        h.insert("cache-control", HeaderValue::from_static("private, no-store"));
        // A hint for crawlers only. Access control is the unguessable, revocable token.
        h.insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow"));
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RequestCtx {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<RequestCtx>().cloned().ok_or(AppError::Forbidden("Request context missing."))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Actor {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut axum::http::request::Parts, s: &S) -> Result<Self, Self::Rejection> {
        let ctx = RequestCtx::from_request_parts(parts, s).await?;
        ctx.actor.ok_or(AppError::BadRequest { message: "Choose who you are first.".into(), field: Some("actor".into()) })
    }
}

pub fn now_ts(state: &AppState) -> String {
    ts(state.now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_parsing() {
        assert_eq!(host_only("LocalHost:7878"), "localhost");
        assert_eq!(host_only("[::1]:7878"), "[::1]");
        assert_eq!(origin_host("http://127.0.0.1:5173").as_deref(), Some("127.0.0.1"));
        assert_eq!(origin_host("https://evil.example/x").as_deref(), Some("evil.example"));
    }

    #[test]
    fn cookies() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=1; tendly_device=abc; b=2"));
        assert_eq!(cookie_value(&h, DEVICE_COOKIE).as_deref(), Some("abc"));
        assert_eq!(cookie_value(&h, "zzz"), None);
    }
}
