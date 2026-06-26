pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, post, put};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/instapaper/credentials",
            put(handler::save_credentials).delete(handler::delete_credentials),
        )
        .route("/api/instapaper/status", get(handler::status))
        // 06（read-later）は同一ファイルでこの行に .get(handler::list) を足し、
        // service::add_to_read_later に status 永続化を加える（HTTP 契約は不変）。
        .route("/api/read-later", post(handler::add_read_later))
}
