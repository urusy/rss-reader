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
        .route(
            "/api/read-later",
            post(handler::save_for_later).get(handler::list_read_later),
        )
        // Static segment; matchit prefers it over /{article_id} (no conflict).
        .route(
            "/api/read-later/settings",
            get(handler::get_settings).put(handler::update_settings),
        )
        .route(
            "/api/read-later/{article_id}",
            get(handler::get_read_later_one),
        )
}
