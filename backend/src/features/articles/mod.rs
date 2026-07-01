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
        .route("/api/articles/read-all", post(handler::mark_all_read))
        .route("/api/articles/{id}", get(handler::get_one))
        .route("/api/articles/{id}/read", post(handler::mark_read))
        .route(
            "/api/articles/{id}/summarize",
            post(handler::summarize).delete(handler::delete_summary),
        )
        .route(
            "/api/articles/{id}/translate",
            post(handler::translate).delete(handler::delete_translation),
        )
}
