//! Tendly server library: HTTP API, worker and storage. The same router is
//! used by `tendly serve` and, in-process, by the native (Tauri) shell.

// SQL rows are read as plain tuples next to their queries; naming each one adds noise.
#![allow(clippy::type_complexity)]

pub mod activity;
pub mod ai;
pub mod backup;
pub mod calendar;
pub mod config;
pub mod connectors;
pub mod crypto;
pub mod db;
pub mod error;
pub mod fetch;
pub mod routes;
pub mod security;
pub mod seed;
pub mod share_render;
pub mod state;
pub mod validate;
pub mod worker;

use anyhow::Result;
use axum::Router;
use config::Config;
use state::{AppState, Clock, Inner, RateLimiter};
use std::path::Path;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

pub async fn init_state(config: Config) -> Result<AppState> {
    std::fs::create_dir_all(&config.data_dir)?;
    let db = db::connect(&config.database_path).await?;
    let cipher = crypto::Cipher::load(config.encryption_key.as_deref(), config.encryption_key_file.as_deref(), &config.data_dir)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(concat!("Tendly/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let state = AppState(Arc::new(Inner { db, config, cipher, limiter: RateLimiter::default(), http, clock: Clock::default() }));
    if state.config.demo {
        seed::seed_demo(&state).await?;
    }
    Ok(state)
}

/// Builds the full application router including security middleware and,
/// when configured, the static web app.
pub fn router(state: AppState) -> Router {
    let mut app = routes::api(state.clone());
    if let Some(dir) = state.config.web_dir.clone() {
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
    }
    app.layer(axum::middleware::from_fn_with_state(state, security::guard))
}

/// Test helper: a fresh state in `dir` with loopback-only defaults.
#[doc(hidden)]
pub async fn test_state(dir: &Path) -> AppState {
    let mut cfg = Config::for_data_dir(dir.to_path_buf());
    cfg.fixture_dir = Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/fixtures/mail"));
    init_state(cfg).await.expect("test state")
}
