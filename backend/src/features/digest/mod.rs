pub mod domain;
pub mod email;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/digest/latest", get(handler::latest))
        .route("/api/digest", get(handler::by_date))
        .route("/api/digest/refresh", post(handler::refresh))
}
