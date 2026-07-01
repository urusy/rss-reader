pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/clusters", get(handler::list))
        // Static "recluster" before the dynamic {id} (matchit prefers static).
        .route("/api/clusters/recluster", post(handler::recluster))
        .route("/api/clusters/{id}", get(handler::get_one))
        .route("/api/clusters/{id}/summary", post(handler::summarize))
}
