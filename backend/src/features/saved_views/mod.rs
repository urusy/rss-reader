pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::get;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/saved-views",
            get(handler::list_views).post(handler::create_view),
        )
        .route(
            "/api/saved-views/{id}",
            get(handler::get_view)
                .patch(handler::update_view)
                .delete(handler::delete_view),
        )
        .route("/api/saved-views/{id}/articles", get(handler::resolve_view))
}
