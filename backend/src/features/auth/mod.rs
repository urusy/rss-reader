pub mod domain;
pub mod handler;
pub mod service;

use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;

/// auth exposes only public routes (status / login); they sit outside the
/// protective middleware so the login flow isn't a chicken-and-egg problem.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(handler::status))
        .route("/api/auth/login", post(handler::login))
}
