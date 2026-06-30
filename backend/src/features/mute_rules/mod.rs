pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, patch, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/mute-rules", get(handler::list).post(handler::create))
        // Static "apply" outranks the dynamic {id} in matchit (no conflict).
        .route("/api/mute-rules/apply", post(handler::apply))
        .route(
            "/api/mute-rules/{id}",
            patch(handler::update).delete(handler::delete),
        )
}
