use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Application-wide error type.
///
/// Domain/feature code returns typed variants; the `IntoResponse` impl maps them
/// to HTTP status codes so handlers can simply `?`-propagate.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("resource not found")]
    NotFound,

    #[error("invalid input: {0}")]
    Validation(String),

    #[error("feature not yet enabled: {0}")]
    NotEnabled(String),

    #[error("upstream request failed: {0}")]
    Upstream(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::NotEnabled(_) => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            AppError::Upstream(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::Database(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
            AppError::Other(e) => {
                tracing::error!(error = %e, "unhandled error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
