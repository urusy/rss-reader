pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{get, patch, put};
use axum::Router;

use crate::shared::state::AppState;

/// Stars + highlights/annotations. Local knowledge base; never synced externally.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/stars", get(handler::list_stars))
        .route(
            "/api/articles/{id}/star",
            put(handler::add_star).delete(handler::remove_star),
        )
        .route(
            "/api/articles/{id}/highlights",
            get(handler::list_highlights).post(handler::create_highlight),
        )
        .route(
            "/api/highlights/{hid}",
            patch(handler::patch_highlight).delete(handler::delete_highlight),
        )
}
