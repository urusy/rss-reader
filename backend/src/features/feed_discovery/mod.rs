pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::routing::post;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/feeds/discover", post(handler::discover))
}
