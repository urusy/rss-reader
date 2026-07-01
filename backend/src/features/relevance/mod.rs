pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/relevance/scores", get(handler::list_scores))
        .route("/api/relevance/profile", get(handler::profile))
        .route("/api/relevance/score", post(handler::score))
}
