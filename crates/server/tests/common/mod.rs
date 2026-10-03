#![allow(dead_code)]

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use std::net::SocketAddr;
use tendly_server::state::AppState;
use tower::ServiceExt;

pub struct TestApp {
    pub state: AppState,
    pub router: Router,
    pub _dir: tempfile::TempDir,
}

pub struct Req {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
    pub raw: Option<String>,
    pub actor: Option<String>,
    pub peer: SocketAddr,
    pub host: String,
    pub csrf: bool,
    pub headers: Vec<(String, String)>,
}

impl Req {
    pub fn new(method: &'static str, path: impl Into<String>) -> Self {
        Req {
            method,
            path: path.into(),
            body: None,
            raw: None,
            actor: None,
            peer: "127.0.0.1:50000".parse().unwrap(),
            host: "127.0.0.1:7878".into(),
            csrf: true,
            headers: vec![],
        }
    }
    pub fn json(mut self, v: Value) -> Self {
        self.body = Some(v);
        self
    }
    pub fn raw(mut self, s: impl Into<String>) -> Self {
        self.raw = Some(s.into());
        self
    }
    pub fn actor(mut self, id: &str) -> Self {
        self.actor = Some(id.to_string());
        self
    }
    pub fn peer(mut self, p: &str) -> Self {
        self.peer = p.parse().unwrap();
        self
    }
    pub fn host(mut self, h: &str) -> Self {
        self.host = h.into();
        self
    }
    pub fn no_csrf(mut self) -> Self {
        self.csrf = false;
        self
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
}

pub struct Res {
    pub status: StatusCode,
    pub body: Value,
    pub text: String,
    pub headers: axum::http::HeaderMap,
}

impl TestApp {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = tendly_server::test_state(dir.path()).await;
        Self::from_state(state, dir)
    }

    pub fn from_state(state: AppState, dir: tempfile::TempDir) -> Self {
        let router = tendly_server::router(state.clone());
        TestApp { state, router, _dir: dir }
    }

    pub async fn send(&self, r: Req) -> Res {
        let mut b = Request::builder().method(r.method).uri(&r.path).header("host", &r.host);
        if r.csrf {
            b = b.header("x-tendly-csrf", "1");
        }
        if let Some(a) = &r.actor {
            b = b.header("x-tendly-actor", a);
        }
        for (k, v) in &r.headers {
            b = b.header(k.as_str(), v.as_str());
        }
        let body = if let Some(v) = &r.body {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        } else if let Some(s) = &r.raw {
            b = b.header("content-type", "text/calendar");
            Body::from(s.clone())
        } else {
            Body::empty()
        };
        let mut req = b.body(body).unwrap();
        req.extensions_mut().insert(ConnectInfo(r.peer));
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Res { status, body, text, headers }
    }

    pub async fn ok(&self, r: Req) -> Value {
        let path = r.path.clone();
        let res = self.send(r).await;
        assert!(res.status.is_success(), "{path} -> {} {}", res.status, res.text);
        res.body
    }

    pub async fn member(&self, name: &str) -> String {
        let v = self.ok(Req::new("POST", "/api/members").json(serde_json::json!({"displayName": name, "timezone": "UTC"}))).await;
        let id = v["id"].as_str().unwrap().to_string();
        // Clear default quiet hours so notification tests don't depend on the time of day.
        let mut prefs = v["prefs"].clone();
        prefs["quietStart"] = serde_json::Value::Null;
        prefs["quietEnd"] = serde_json::Value::Null;
        self.ok(Req::new("PATCH", format!("/api/members/{id}")).actor(&id).json(serde_json::json!({"prefs": prefs}))).await;
        id
    }

    pub async fn group(&self, actor: &str, name: &str, mode: &str, members: &[&str]) -> Value {
        self.ok(
            Req::new("POST", "/api/groups")
                .actor(actor)
                .json(serde_json::json!({"name": name, "kind": if mode == "project" {"project_team"} else {"family"}, "mode": mode, "memberIds": members})),
        )
        .await
    }

    pub async fn enable_sharing(&self, actor: &str) {
        self.ok(Req::new("PATCH", "/api/admin/settings").actor(actor).json(serde_json::json!({"sharingEnabled": true}))).await;
    }
}
