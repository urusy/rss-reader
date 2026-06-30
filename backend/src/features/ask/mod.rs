pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Cross-article Ask. Static "ask" outranks {id} in matchit (no conflict).
        .route("/api/articles/ask", post(handler::ask_many))
        .route("/api/articles/{id}/ask", post(handler::ask_one))
        .route("/api/articles/{id}/notes", get(handler::get_notes))
}
