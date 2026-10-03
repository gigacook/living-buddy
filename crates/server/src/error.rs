//! Structured, sanitized API errors. Internal details are logged (redacted)
//! and never returned to clients.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::Value;
use tendly_core::api::ApiError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    BadRequest { message: String, field: Option<String> },
    #[error("not found")]
    NotFound(&'static str),
    #[error("conflict")]
    Conflict { message: String, current: Option<Value> },
    #[error("forbidden: {0}")]
    Forbidden(&'static str),
    #[error("unauthorized")]
    Unauthorized(&'static str),
    #[error("rate limited")]
    RateLimited,
    #[error("payload too large")]
    TooLarge,
    #[error("upstream: {0}")]
    Upstream(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn bad(message: impl Into<String>) -> Self {
        AppError::BadRequest { message: message.into(), field: None }
    }
    pub fn field(field: &str, message: impl Into<String>) -> Self {
        AppError::BadRequest { message: message.into(), field: Some(field.to_string()) }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, field, current) = match self {
            AppError::BadRequest { message, field } => (StatusCode::BAD_REQUEST, "invalid_input", message, field, None),
            AppError::NotFound(what) => (StatusCode::NOT_FOUND, "not_found", format!("That {what} could not be found."), None, None),
            AppError::Conflict { message, current } => (StatusCode::CONFLICT, "conflict", message, None, current),
            AppError::Forbidden(why) => (StatusCode::FORBIDDEN, "forbidden", why.to_string(), None, None),
            AppError::Unauthorized(code) => {
                (StatusCode::UNAUTHORIZED, code, "This device needs to be paired first.".to_string(), None, None)
            }
            AppError::RateLimited => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited", "Too many requests. Please wait a moment.".into(), None, None)
            }
            AppError::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large", "That is larger than allowed.".into(), None, None),
            AppError::Upstream(msg) => (StatusCode::BAD_GATEWAY, "upstream_error", tendly_core::redact::redact(&msg), None, None),
            AppError::Internal(err) => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                tracing::error!(error_id = %id, error = %tendly_core::redact::redact(&format!("{err:#}")), "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", format!("Something went wrong on our side (ref {}).", &id[..8]), None, None)
            }
        };
        (status, Json(ApiError { code: code.to_string(), message, field, current })).into_response()
    }
}
