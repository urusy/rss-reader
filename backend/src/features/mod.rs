pub mod articles;
pub mod feed_overview;
pub mod feeds;
pub mod health;
pub mod stats;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::shared::state::AppState;

/// Compose every vertical slice into the top-level router.
///
/// Each feature owns its own `routes()` returning a `Router<AppState>`. Adding a
/// feature = add one module + one `.merge()` line. Existing slices stay untouched.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .merge(feeds::routes())
        .merge(articles::routes())
        .merge(stats::routes())
        .merge(feed_overview::routes())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()) // tighten before exposing beyond your LAN
        .with_state(state)
}
