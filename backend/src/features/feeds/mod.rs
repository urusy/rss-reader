pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/feeds", get(handler::list).post(handler::create))
        .route("/api/feeds/{id}", axum::routing::delete(handler::delete))
        .route("/api/feeds/{id}/refresh", post(handler::refresh))
}
