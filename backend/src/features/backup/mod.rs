pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/backup/export", get(handler::export))
        .route("/api/backup/import", post(handler::import))
        .route("/api/backup/runs", get(handler::runs))
        // import receives the whole backup body; lift the default 2MB limit.
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
}
