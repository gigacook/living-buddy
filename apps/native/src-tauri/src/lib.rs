//! Tendly native shell. The web UI calls one command, `api_request`, which is
//! dispatched in-process to the same Axum router the self-hosted server uses.
//! There is no listening socket: other apps and devices cannot reach it.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderName, HeaderValue, Method, Request};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use tauri::Manager;
use tokio::sync::OnceCell;
use tower::ServiceExt;

struct Backend {
    router: OnceCell<axum::Router>,
}

#[derive(Deserialize)]
struct ApiRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Option<String>,
}

#[derive(Serialize)]
struct ApiResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

/// Paths the UI may call. Share pages and static files are not served in-process.
fn allowed_path(path: &str) -> bool {
    path.starts_with("/api/") && !path.contains("..") && path.len() < 2048
}

#[tauri::command]
async fn api_request(app: tauri::AppHandle, request: ApiRequest) -> Result<ApiResponse, String> {
    if !allowed_path(&request.path) {
        return Err("path not allowed".into());
    }
    let backend = app.state::<Backend>();
    let router = backend
        .router
        .get_or_try_init(|| async {
            let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let mut cfg = tendly_server::config::Config::for_data_dir(dir);
            if let Ok(tz) = std::env::var("TZ") {
                if tz.parse::<chrono_tz_check::Tz>().is_ok() {
                    cfg.default_timezone = tz;
                }
            }
            let state = tendly_server::init_state(cfg).await.map_err(|e| e.to_string())?;
            // Calendar subscriptions and connectors refresh while the app runs.
            tauri::async_runtime::spawn(tendly_server::worker::run_forever(state.clone(), std::time::Duration::from_secs(120)));
            Ok::<_, String>(tendly_server::router(state))
        })
        .await?
        .clone();
    let method = Method::from_bytes(request.method.as_bytes()).map_err(|_| "bad method")?;
    let mut builder = Request::builder().method(method).uri(&request.path).header("host", "tauri.localhost");
    for (k, v) in &request.headers {
        let name = HeaderName::from_bytes(k.as_bytes()).map_err(|_| "bad header")?;
        if name == "host" || name == "x-forwarded-for" {
            continue;
        }
        builder = builder.header(name, HeaderValue::from_str(v).map_err(|_| "bad header")?);
    }
    let mut req = builder.body(Body::from(request.body.unwrap_or_default())).map_err(|e| e.to_string())?;
    // The native app is the local device owner: treat it as a loopback peer.
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    let res = router.oneshot(req).await.map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let headers = res.headers().iter().filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string()))).collect();
    let bytes = res.into_body().collect().await.map_err(|e| e.to_string())?.to_bytes();
    Ok(ApiResponse { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() })
}

mod chrono_tz_check {
    pub use chrono_tz::Tz;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(Backend { router: OnceCell::new() })
        .invoke_handler(tauri::generate_handler![api_request])
        .run(tauri::generate_context!())
        .expect("error while running Tendly");
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_api_paths_are_forwarded() {
        assert!(super::allowed_path("/api/tasks"));
        assert!(!super::allowed_path("/share/abc"));
        assert!(!super::allowed_path("/api/../etc/passwd"));
        assert!(!super::allowed_path("http://evil/api/x"));
    }
}
