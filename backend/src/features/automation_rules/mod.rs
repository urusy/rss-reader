pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/rules", get(handler::list).post(handler::create))
        // Static "apply" outranks the dynamic {id}.
        .route("/api/rules/apply", post(handler::apply))
        .route(
            "/api/rules/{id}",
            get(handler::get_one)
                .put(handler::update)
                .delete(handler::delete),
        )
        .route("/api/rules/{id}/test", post(handler::test))
}
