pub mod domain;
pub mod handler;
pub mod service;

use axum::routing::post;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Distinct trailing segment from articles' /api/articles/{id} routes,
        // so merging the two routers does not conflict.
        .route("/api/articles/{id}/extract", post(handler::extract))
}
