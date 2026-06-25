mod handler;

use axum::routing::get;
use axum::Router;

use crate::shared::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(handler::liveness))
        .route("/api/health/db", get(handler::readiness))
}
