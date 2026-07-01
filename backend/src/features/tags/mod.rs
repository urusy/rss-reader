pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/tags",
            get(handler::list_tags).post(handler::create_tag),
        )
        .route(
            "/api/tags/{id}",
            patch(handler::update_tag).delete(handler::delete_tag),
        )
        .route(
            "/api/articles/{id}/tags",
            get(handler::list_article_tags).put(handler::set_article_tags),
        )
        .route(
            "/api/articles/{id}/tags/{tag_id}",
            delete(handler::detach_tag),
        )
        .route(
            "/api/articles/{id}/suggest-tags",
            post(handler::suggest_tags),
        )
}
