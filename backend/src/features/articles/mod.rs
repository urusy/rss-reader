pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/articles", get(handler::list))
        .route("/api/articles/{id}", get(handler::get_one))
        .route("/api/articles/{id}/read", post(handler::mark_read))
        .route("/api/articles/{id}/summarize", post(handler::summarize))
        .route("/api/articles/{id}/translate", post(handler::translate))
}
