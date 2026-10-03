//! Network boundary, Host validation, CSRF, admin boundary and remote-mode device auth.

mod common;
use axum::http::StatusCode;
use common::{Req, TestApp};
use serde_json::json;
use tendly_core::api::ServerMode;
use tendly_server::config::Config;

#[tokio::test]
async fn health_and_session() {
    let app = TestApp::new().await;
    assert_eq!(app.send(Req::new("GET", "/healthz")).await.text, "ok");
    assert_eq!(app.send(Req::new("GET", "/readyz")).await.text, "ready");
    let s = app.ok(Req::new("GET", "/api/session")).await;
    assert_eq!(s["mode"], "local");
    assert_eq!(s["adminAvailable"], true);
    assert_eq!(s["sharingEnabled"], false);
    let res = app.send(Req::new("GET", "/api/session")).await;
    assert_eq!(res.headers.get("x-frame-options").unwrap(), "DENY");
    assert!(res.headers.get("content-security-policy").unwrap().to_str().unwrap().contains("default-src 'self'"));
}

#[tokio::test]
async fn local_mode_rejects_remote_peers_and_foreign_hosts() {
    let app = TestApp::new().await;
    let res = app.send(Req::new("GET", "/api/session").peer("192.168.1.20:4000")).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.send(Req::new("GET", "/api/session").host("evil.example")).await;
    assert_eq!(res.status, StatusCode::MISDIRECTED_REQUEST);
    // DNS rebinding: attacker host name resolving to 127.0.0.1.
    let res = app.send(Req::new("GET", "/api/members").host("rebind.attacker.test:7878")).await;
    assert_eq!(res.status, StatusCode::MISDIRECTED_REQUEST);
}

#[tokio::test]
async fn csrf_header_and_origin_are_required_for_writes() {
    let app = TestApp::new().await;
    let res = app.send(Req::new("POST", "/api/members").json(json!({"displayName": "A"})).no_csrf()).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.body["code"], "csrf");
    let res = app.send(Req::new("POST", "/api/members").json(json!({"displayName": "A"})).header("origin", "https://evil.example")).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.send(Req::new("POST", "/api/members").json(json!({"displayName": "A"})).header("origin", "http://localhost:5173")).await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn admin_is_loopback_only_and_not_behind_proxies() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::for_data_dir(dir.path().to_path_buf());
    cfg.mode = ServerMode::Lan;
    cfg.bind = "0.0.0.0:7878".parse().unwrap();
    cfg.allowed_networks = vec!["192.168.1.0/24".parse().unwrap()];
    let state = tendly_server::init_state(cfg).await.unwrap();
    let app = TestApp::from_state(state, dir);
    // A LAN device can use the app...
    let lan = "192.168.1.20:5000";
    let res = app.send(Req::new("GET", "/api/session").peer(lan)).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["adminAvailable"], false);
    // ...but not admin endpoints.
    for path in ["/api/admin/settings", "/api/admin/ai", "/api/admin/connectors", "/api/admin/export"] {
        let res = app.send(Req::new("GET", path).peer(lan)).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(res.body["code"], "admin_only");
    }
    // A loopback peer carrying proxy headers is not trusted as local.
    let res = app.send(Req::new("GET", "/api/admin/settings").header("x-forwarded-for", "203.0.113.9")).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    // Outside the allowlist: rejected entirely.
    let res = app.send(Req::new("GET", "/api/session").peer("10.0.0.5:5000")).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    // Loopback admin works.
    assert_eq!(app.send(Req::new("GET", "/api/admin/settings")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn remote_mode_requires_paired_device_and_admin_token() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::for_data_dir(dir.path().to_path_buf());
    cfg.mode = ServerMode::Remote;
    cfg.bind = "0.0.0.0:7878".parse().unwrap();
    cfg.admin_token = Some("a".repeat(40));
    cfg.allowed_hosts = vec!["tendly.example.org".into()];
    cfg.cookie_secure = true;
    let state = tendly_server::init_state(cfg).await.unwrap();
    let (_id, code) = tendly_server::routes::admin::create_device_token(&state, "Phone").await.unwrap();
    let app = TestApp::from_state(state, dir);
    let host = "tendly.example.org";
    let peer = "203.0.113.50:443";
    // Unpaired: session works (to show the pairing screen), everything else is 401.
    let s = app.ok(Req::new("GET", "/api/session").host(host).peer(peer)).await;
    assert_eq!(s["deviceAuthRequired"], true);
    assert_eq!(s["devicePaired"], false);
    let res = app.send(Req::new("GET", "/api/members").host(host).peer(peer)).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    assert_eq!(res.body["code"], "device_auth_required");
    // Wrong code.
    let res = app.send(Req::new("POST", "/api/auth/pair").host(host).peer(peer).json(json!({"code": "x".repeat(43)}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    // Pair.
    let res = app.send(Req::new("POST", "/api/auth/pair").host(host).peer(peer).json(json!({"code": code}))).await;
    assert_eq!(res.status, StatusCode::OK);
    let cookie = res.headers.get("set-cookie").unwrap().to_str().unwrap().to_string();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict") && cookie.contains("Secure"));
    let cookie_pair = cookie.split(';').next().unwrap().to_string();
    let res = app.send(Req::new("GET", "/api/members").host(host).peer(peer).header("cookie", &cookie_pair)).await;
    assert_eq!(res.status, StatusCode::OK);
    // Admin needs the admin token in addition to the device.
    let res = app.send(Req::new("GET", "/api/admin/settings").host(host).peer(peer).header("cookie", &cookie_pair)).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app
        .send(
            Req::new("GET", "/api/admin/settings")
                .host(host)
                .peer(peer)
                .header("cookie", &cookie_pair)
                .header("x-tendly-admin-token", &"a".repeat(40)),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn secrets_never_leave_admin_endpoints() {
    let app = TestApp::new().await;
    let a = app.member("Admin").await;
    let key = "sk-ant-api03-SYNTHETIC-TEST-KEY-000000";
    let v = app.ok(Req::new("PUT", "/api/admin/ai").json(json!({"provider": "anthropic", "apiKey": key}))).await;
    assert_eq!(v["hasKey"], true);
    assert_eq!(v["keySource"], "stored");
    assert!(!v.to_string().contains("SYNTHETIC"));
    let stored: String =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'ai_key_ciphertext'").fetch_one(&app.state.db).await.unwrap();
    assert!(!stored.contains("SYNTHETIC"));
    let export = app.send(Req::new("GET", "/api/admin/export")).await;
    assert!(!export.text.contains("SYNTHETIC") && !export.text.contains("ai_key_ciphertext"));
    let session = app.ok(Req::new("GET", "/api/session").actor(&a)).await;
    assert!(!session.to_string().contains("SYNTHETIC"));
}

#[tokio::test]
async fn unknown_api_paths_return_json_404() {
    let app = TestApp::new().await;
    let res = app.send(Req::new("GET", "/api/nope")).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.body["code"], "not_found");
}
