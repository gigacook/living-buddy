//! Connector ingestion, duplicate handling, retries, provider adapters (mocked)
//! and untrusted-content handling.

mod common;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Duration;
use common::{Req, TestApp};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn fixture_connector_ingests_once_and_requires_confirmation() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    let c = app
        .ok(Req::new("POST", "/api/admin/connectors")
            .actor(&a)
            .json(json!({"provider": "fixture", "displayName": "Sample inbox", "settings": {"set": "sample"}})))
        .await;
    let id = c["id"].as_str().unwrap();
    assert_eq!(c["implementation"], "implemented");
    let res = app.send(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "disabled connectors don't run");
    app.ok(Req::new("PATCH", format!("/api/admin/connectors/{id}")).json(json!({"enabled": true}))).await;
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "healthy");
    // Six files, one of which repeats a message id: five items, newsletter yields nothing.
    assert_eq!(c["itemsIngested"], 5);
    let pending = app.ok(Req::new("GET", "/api/inbox/suggestions").actor(&a)).await;
    let list = pending.as_array().unwrap();
    assert_eq!(list.len(), 4, "{pending}");
    // Suggestions belong to the connector owner only.
    assert!(app.ok(Req::new("GET", "/api/inbox/suggestions").actor(&b)).await.as_array().unwrap().is_empty());

    // Running again (overlapping cursor reset) does not duplicate anything.
    sqlx::query("UPDATE connectors SET cursor = NULL").execute(&app.state.db).await.unwrap();
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["itemsIngested"], 5);
    assert_eq!(app.ok(Req::new("GET", "/api/inbox/suggestions").actor(&a)).await.as_array().unwrap().len(), 4);

    // The injection attempt is flagged, inert, and created no tasks or events by itself.
    let inj = list.iter().find(|s| s["subject"].as_str().unwrap().contains("ACTION REQUIRED")).unwrap();
    assert!(inj["draft"]["flags"].as_array().unwrap().iter().any(|f| f == "possible_instructions_in_content"));
    let tasks = app.ok(Req::new("GET", "/api/tasks?status=all").actor(&a)).await;
    assert!(tasks.as_array().unwrap().is_empty(), "nothing is created without confirmation");
    let excerpt: Option<String> =
        sqlx::query_scalar("SELECT excerpt FROM ingested_items WHERE subject LIKE 'Invoice%'").fetch_one(&app.state.db).await.unwrap();
    assert!(!excerpt.unwrap().contains("FAKE-TOKEN"), "URL query strings are stripped before storage");

    // Accept the dentist suggestion as an event and the invoice as a task.
    let dentist = list.iter().find(|s| s["subject"].as_str().unwrap().contains("dental")).unwrap();
    assert_eq!(dentist["draft"]["date"], "2026-10-14");
    let acc = app
        .ok(Req::new("POST", format!("/api/inbox/suggestions/{}/accept", dentist["id"].as_str().unwrap()))
            .actor(&a)
            .json(json!({"createAs": "event", "title": "Dentist", "date": "2026-10-14", "time": "14:30", "timezone": "UTC"})))
        .await;
    assert_eq!(acc["status"], "accepted");
    assert_eq!(acc["resultType"], "event");
    let invoice = list.iter().find(|s| s["subject"].as_str().unwrap().contains("Invoice")).unwrap();
    let acc = app
        .ok(Req::new("POST", format!("/api/inbox/suggestions/{}/accept", invoice["id"].as_str().unwrap()))
            .actor(&a)
            .json(json!({"createAs": "task", "title": "Pay electricity", "date": "2026-10-25"})))
        .await;
    let t = app.ok(Req::new("GET", format!("/api/tasks/{}", acc["resultId"].as_str().unwrap())).actor(&a)).await;
    assert_eq!(t["tags"], json!(["inbox"]));
    let res = app
        .send(
            Req::new("POST", format!("/api/inbox/suggestions/{}/accept", invoice["id"].as_str().unwrap()))
                .actor(&a)
                .json(json!({"createAs": "task", "title": "again"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "accepting twice is refused");
    let d = app.ok(Req::new("POST", format!("/api/inbox/suggestions/{}/dismiss", inj["id"].as_str().unwrap())).actor(&a)).await;
    assert_eq!(d["status"], "dismissed");
    // Someone else can't act on Alex's suggestions.
    let school = list.iter().find(|s| s["subject"].as_str().unwrap().contains("permission")).unwrap();
    assert_eq!(
        app.send(Req::new("POST", format!("/api/inbox/suggestions/{}/dismiss", school["id"].as_str().unwrap())).actor(&b)).await.status,
        StatusCode::NOT_FOUND
    );

    // Retention removes stored excerpts after the period.
    app.state.clock.advance(Duration::days(30));
    let (items, _) = tendly_server::connectors::apply_retention(&app.state).await.unwrap();
    assert_eq!(items, 5);
}

#[tokio::test]
async fn paste_intake_uses_local_rules_without_ai() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let r = app
        .ok(Req::new("POST", "/api/inbox/intake")
            .actor(&a)
            .json(json!({"subject": "Parent-teacher meeting", "text": "The meeting is on 2026-11-05 at 17:00 in room 4.", "useAi": true})))
        .await;
    assert_eq!(r["extractor"], "rules");
    assert!(r["notice"].as_str().unwrap().contains("not enabled"));
    assert_eq!(r["suggestions"][0]["draft"]["kind"], "appointment");
    assert_eq!(r["suggestions"][0]["draft"]["date"], "2026-11-05");
    assert_eq!(app.send(Req::new("POST", "/api/inbox/intake").actor(&a).json(json!({"text": "  "}))).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn job_queue_retries_with_backoff_and_is_idempotent() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let s = &app.state;
    // A connector pointing at a missing fixture set fails permanently: no retry storm.
    let c = app
        .ok(Req::new("POST", "/api/admin/connectors")
            .actor(&a)
            .json(json!({"provider": "fixture", "displayName": "Broken", "settings": {"set": "does-not-exist"}})))
        .await;
    app.ok(Req::new("PATCH", format!("/api/admin/connectors/{}", c["id"].as_str().unwrap())).json(json!({"enabled": true}))).await;
    let n = tendly_server::worker::schedule_due(s).await.unwrap();
    assert!(n >= 2, "sync + retention scheduled");
    assert_eq!(tendly_server::worker::schedule_due(s).await.unwrap(), 0, "same window: no duplicate jobs");
    tendly_server::worker::run_once(s, 10).await.unwrap();
    let (status, attempts): (String, i64) =
        sqlx::query_as("SELECT status, attempts FROM jobs WHERE kind = 'connector_sync'").fetch_one(&s.db).await.unwrap();
    assert_eq!((status.as_str(), attempts), ("dead", 1));
    let conn: (String, i64) = sqlx::query_as("SELECT status, consecutive_failures FROM connectors").fetch_one(&s.db).await.unwrap();
    assert_eq!(conn, ("error".to_string(), 1));

    // A transient failure (unreachable calendar feed) is retried later with backoff.
    let now = tendly_server::db::ts(s.now());
    sqlx::query("INSERT INTO calendar_sources (id, name, kind, url_ciphertext, enabled, created_at, updated_at) VALUES ('src', 'Feed', 'url', ?, 1, ?, ?)")
        .bind(s.cipher.encrypt("https://127.0.0.1:9/never.ics").unwrap())
        .bind(&now)
        .bind(&now)
        .execute(&s.db)
        .await
        .unwrap();
    tendly_server::worker::enqueue(s, "calendar_refresh", json!({"sourceId": "src"}), Some("cal-test"), s.now()).await.unwrap();
    assert!(
        !tendly_server::worker::enqueue(s, "calendar_refresh", json!({"sourceId": "src"}), Some("cal-test"), s.now()).await.unwrap(),
        "idempotency key"
    );
    tendly_server::worker::run_once(s, 10).await.unwrap();
    let (status, attempts, run_after): (String, i64, String) =
        sqlx::query_as("SELECT status, attempts, run_after FROM jobs WHERE idempotency_key = 'cal-test'").fetch_one(&s.db).await.unwrap();
    assert_eq!((status.as_str(), attempts), ("queued", 1));
    assert!(tendly_server::db::parse_ts(&run_after) > s.now() + Duration::seconds(20), "backoff applied");
    // After max attempts it becomes dead instead of retrying forever.
    for _ in 0..6 {
        s.clock.advance(Duration::hours(7));
        tendly_server::worker::run_once(s, 10).await.unwrap();
    }
    let (status, attempts): (String, i64) =
        sqlx::query_as("SELECT status, attempts FROM jobs WHERE idempotency_key = 'cal-test'").fetch_one(&s.db).await.unwrap();
    assert_eq!((status.as_str(), attempts), ("dead", 5));
    assert!(tendly_server::worker::backoff(1, None) < tendly_server::worker::backoff(4, None));
    assert_eq!(tendly_server::worker::backoff(3, Some(120)), Duration::seconds(120));
}

#[derive(Default)]
struct GmailMock {
    token_calls: AtomicUsize,
    history_calls: AtomicUsize,
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn gmail_adapter_refreshes_tokens_uses_history_cursor_and_detects_revocation() {
    let mock = Arc::new(GmailMock::default());
    let m1 = mock.clone();
    let m2 = mock.clone();
    let router = Router::new()
        .route(
            "/token",
            post(move |body: String| {
                let m = m1.clone();
                async move {
                    m.token_calls.fetch_add(1, Ordering::SeqCst);
                    assert!(body.contains("grant_type=refresh_token"));
                    if body.contains("refresh_token=revoked") {
                        return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_grant"})));
                    }
                    (StatusCode::OK, Json(json!({"access_token": "fresh-access", "expires_in": 3600})))
                }
            }),
        )
        .route(
            "/gmail/v1/users/me/messages",
            get(|headers: axum::http::HeaderMap| async move {
                if headers.get("authorization").unwrap() != "Bearer fresh-access" {
                    return (StatusCode::UNAUTHORIZED, Json(json!({})));
                }
                (StatusCode::OK, Json(json!({"messages": [{"id": "m1"}, {"id": "m2"}]})))
            }),
        )
        .route("/gmail/v1/users/me/profile", get(|| async { Json(json!({"historyId": "100"})) }))
        .route(
            "/gmail/v1/users/me/history",
            get(move |q: axum::extract::Query<std::collections::HashMap<String, String>>| {
                let m = m2.clone();
                async move {
                    m.history_calls.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(q.get("startHistoryId").map(String::as_str), Some("100"));
                    Json(json!({"historyId": "105", "history": [{"messagesAdded": [{"message": {"id": "m3"}}, {"message": {"id": "m2"}}]}]}))
                }
            }),
        )
        .route(
            "/gmail/v1/users/me/messages/{id}",
            get(|axum::extract::Path(id): axum::extract::Path<String>| async move {
                let (subject, snippet) = match id.as_str() {
                    "m1" => ("Library books due", "Your books are due 2026-10-20. Please return or renew."),
                    "m2" => ("Dinner Friday?", "Want to have dinner on Friday at 7pm?"),
                    _ => ("Pharmacy pickup", "Your prescription is ready for pickup until 2026-10-12."),
                };
                Json(json!({"id": id, "snippet": snippet, "internalDate": "1790000000000", "payload": {"headers": [{"name": "Subject", "value": subject}]}}))
            }),
        );
    let base = spawn(router).await;

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = tendly_server::config::Config::for_data_dir(dir.path().to_path_buf());
    cfg.provider_base_overrides.gmail = base.clone();
    cfg.provider_base_overrides.google_token = format!("{base}/token");
    cfg.oauth.google_client_id = Some("client-id".into());
    cfg.oauth.google_client_secret = Some("client-secret".into());
    let state = tendly_server::init_state(cfg).await.unwrap();
    let app = TestApp::from_state(state, dir);
    let a = app.member("Alex").await;
    let c = app.ok(Req::new("POST", "/api/admin/connectors").actor(&a).json(json!({"provider": "gmail", "displayName": "My Gmail"}))).await;
    let id = c["id"].as_str().unwrap().to_string();
    assert_eq!(c["status"], "needs_auth");
    // Simulate a completed OAuth grant with an expired access token.
    tendly_server::connectors::save_credentials(
        &app.state,
        &id,
        &tendly_server::connectors::Credentials {
            access_token: Some("expired".into()),
            refresh_token: Some("refresh-1".into()),
            expires_at: Some(app.state.now() - Duration::minutes(5)),
        },
    )
    .await
    .unwrap();
    let stored: String = sqlx::query_scalar("SELECT credentials_ciphertext FROM connectors").fetch_one(&app.state.db).await.unwrap();
    assert!(!stored.contains("refresh-1"), "tokens are encrypted at rest");
    app.ok(Req::new("PATCH", format!("/api/admin/connectors/{id}")).json(json!({"enabled": true}))).await;
    let listed = app.ok(Req::new("GET", "/api/admin/connectors")).await;
    assert!(!listed.to_string().contains("refresh-1") && !listed.to_string().contains("expired"));

    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "healthy", "{c}");
    assert_eq!(c["itemsIngested"], 2);
    assert_eq!(mock.token_calls.load(Ordering::SeqCst), 1);
    let cursor: Option<String> = sqlx::query_scalar("SELECT cursor FROM connectors").fetch_one(&app.state.db).await.unwrap();
    assert_eq!(cursor.as_deref(), Some("100"));

    // Incremental sync via history: m2 is a duplicate, m3 is new.
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["itemsIngested"], 3);
    assert_eq!(mock.history_calls.load(Ordering::SeqCst), 1);
    let cursor: Option<String> = sqlx::query_scalar("SELECT cursor FROM connectors").fetch_one(&app.state.db).await.unwrap();
    assert_eq!(cursor.as_deref(), Some("105"));

    // Revoked refresh token -> needs_auth, and the scheduler stops polling it.
    tendly_server::connectors::save_credentials(
        &app.state,
        &id,
        &tendly_server::connectors::Credentials {
            access_token: Some("x".into()),
            refresh_token: Some("revoked".into()),
            expires_at: Some(app.state.now() - Duration::minutes(1)),
        },
    )
    .await
    .unwrap();
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "needs_auth");
    assert!(!c["lastError"].as_str().unwrap().contains("revoked\""));
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'connector_sync'").fetch_one(&app.state.db).await.unwrap();
    tendly_server::worker::schedule_due(&app.state).await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'connector_sync'").fetch_one(&app.state.db).await.unwrap();
    assert_eq!(before, after);

    // Disconnect wipes credentials and cursor.
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/disconnect"))).await;
    assert_eq!(c["hasCredentials"], false);
    assert_eq!(c["hasCursor"], false);
}

#[tokio::test]
async fn slack_adapter_reads_new_messages_and_handles_rate_limits() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c2 = calls.clone();
    let router = Router::new().route(
        "/conversations.history",
        get(move |q: axum::extract::Query<std::collections::HashMap<String, String>>| {
            let calls = c2.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                if n == 2 {
                    return (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "30")], Json(json!({"ok": false})));
                }
                let oldest = q.get("oldest").cloned().unwrap_or_default();
                let msgs: Value = if oldest == "0" {
                    json!([{"ts": "1790000001.000100", "text": "Reminder: please RSVP for Saturday's party by Thursday"}, {"ts": "1790000000.000100", "subtype": "channel_join", "text": "joined"}])
                } else {
                    json!([])
                };
                (StatusCode::OK, [("retry-after", "0")], Json(json!({"ok": true, "messages": msgs})))
            }
        }),
    );
    let base = spawn(router).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = tendly_server::config::Config::for_data_dir(dir.path().to_path_buf());
    cfg.provider_base_overrides.slack = base;
    let state = tendly_server::init_state(cfg).await.unwrap();
    let app = TestApp::from_state(state, dir);
    let a = app.member("Alex").await;
    let c = app
        .ok(Req::new("POST", "/api/admin/connectors")
            .actor(&a)
            .json(json!({"provider": "slack", "displayName": "Team Slack", "settings": {"channels": ["C123"]}})))
        .await;
    let id = c["id"].as_str().unwrap();
    assert_eq!(
        app.send(Req::new("POST", format!("/api/admin/connectors/{id}/credentials")).json(json!({"token": "not-a-token"}))).await.status,
        StatusCode::BAD_REQUEST
    );
    app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/credentials")).json(json!({"token": "xoxb-synthetic-test-token"}))).await;
    app.ok(Req::new("PATCH", format!("/api/admin/connectors/{id}")).json(json!({"enabled": true}))).await;
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "healthy");
    assert_eq!(c["itemsIngested"], 1);
    let s = app.ok(Req::new("GET", "/api/inbox/suggestions").actor(&a)).await;
    assert_eq!(s[0]["draft"]["kind"], "follow_up");
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["itemsIngested"], 1);
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "rate_limited");
}

#[tokio::test]
async fn microsoft_graph_adapter_follows_paging_and_delta_links() {
    let base_holder = Arc::new(std::sync::Mutex::new(String::new()));
    let b1 = base_holder.clone();
    let router = Router::new()
        .route(
            "/v1.0/me/mailFolders/inbox/messages/delta",
            get(move |q: axum::extract::Query<std::collections::HashMap<String, String>>, headers: axum::http::HeaderMap| {
                let base = b1.lock().unwrap().clone();
                async move {
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer graph-access");
                    if q.contains_key("$deltatoken") {
                        return Json(json!({"value": [{"id": "g3", "subject": "Dentist moved", "bodyPreview": "Your appointment is now on 2026-11-03 at 10:00.", "receivedDateTime": "2026-10-04T08:00:00Z"}, {"id": "g1", "@removed": {"reason": "deleted"}}], "@odata.deltaLink": format!("{base}/v1.0/me/mailFolders/inbox/messages/delta?$deltatoken=t2")}));
                    }
                    if q.contains_key("$skiptoken") {
                        return Json(json!({"value": [{"id": "g2", "subject": "Rent reminder", "bodyPreview": "Rent is due 2026-11-01.", "receivedDateTime": "2026-10-03T09:00:00Z"}], "@odata.deltaLink": format!("{base}/v1.0/me/mailFolders/inbox/messages/delta?$deltatoken=t1")}));
                    }
                    Json(json!({"value": [{"id": "g1", "subject": "Parents evening", "bodyPreview": "Please confirm by Friday.", "receivedDateTime": "2026-10-02T09:00:00Z"}], "@odata.nextLink": format!("{base}/v1.0/me/mailFolders/inbox/messages/delta?$skiptoken=p2")}))
                }
            }),
        );
    let base = spawn(router).await;
    *base_holder.lock().unwrap() = base.clone();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = tendly_server::config::Config::for_data_dir(dir.path().to_path_buf());
    cfg.provider_base_overrides.graph = format!("{base}/v1.0");
    let state = tendly_server::init_state(cfg).await.unwrap();
    let app = TestApp::from_state(state, dir);
    let a = app.member("Alex").await;
    let c = app
        .ok(Req::new("POST", "/api/admin/connectors").actor(&a).json(json!({"provider": "microsoft_graph", "displayName": "Work mail"})))
        .await;
    let id = c["id"].as_str().unwrap().to_string();
    tendly_server::connectors::save_credentials(
        &app.state,
        &id,
        &tendly_server::connectors::Credentials {
            access_token: Some("graph-access".into()),
            refresh_token: Some("r".into()),
            expires_at: Some(app.state.now() + Duration::hours(1)),
        },
    )
    .await
    .unwrap();
    app.ok(Req::new("PATCH", format!("/api/admin/connectors/{id}")).json(json!({"enabled": true}))).await;
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "healthy", "{c}");
    assert_eq!(c["itemsIngested"], 2, "both pages read");
    let cursor: Option<String> = sqlx::query_scalar("SELECT cursor FROM connectors").fetch_one(&app.state.db).await.unwrap();
    assert!(cursor.unwrap().ends_with("$deltatoken=t1"));
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["itemsIngested"], 3, "delta adds one; removals are ignored");
    // A cursor pointing at another host is never followed.
    sqlx::query("UPDATE connectors SET cursor = 'https://evil.example/steal'").execute(&app.state.db).await.unwrap();
    let c = app.ok(Req::new("POST", format!("/api/admin/connectors/{id}/run"))).await;
    assert_eq!(c["status"], "healthy");
}
