pub mod articles;
pub mod ask;
pub mod auth;
pub mod backup;
pub mod digest;
pub mod extraction;
pub mod feed_discovery;
pub mod feed_health;
pub mod feed_overview;
pub mod feeds;
pub mod folders;
pub mod health;
pub mod instapaper;
pub mod mute_rules;
pub mod opml;
pub mod relevance;
pub mod search;
pub mod stats;
pub mod tags;

use axum::{middleware, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::shared::auth::require_auth;
use crate::shared::state::AppState;

/// Compose every vertical slice into the top-level router.
///
/// Each feature owns its own `routes()` returning a `Router<AppState>`. Adding a
/// feature = add one module + one `.merge()` line. Existing slices stay untouched.
///
/// Routes split into public (health + auth login/status) and protected (the
/// rest). The protected subrouter carries `require_auth`; with AUTH_TOKEN unset
/// the middleware is a pass-through, so behavior is unchanged by default.
pub fn router(state: AppState) -> Router {
    let public = Router::new().merge(health::routes()).merge(auth::routes());

    let protected = Router::new()
        .merge(feeds::routes())
        .merge(feed_discovery::routes())
        .merge(articles::routes())
        .merge(ask::routes())
        .merge(extraction::routes())
        .merge(stats::routes())
        .merge(feed_overview::routes())
        .merge(feed_health::routes())
        .merge(folders::routes())
        .merge(instapaper::routes())
        .merge(search::routes())
        .merge(opml::routes())
        .merge(mute_rules::routes())
        .merge(tags::routes())
        .merge(digest::routes())
        .merge(relevance::routes())
        .merge(backup::routes())
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()) // tighten before exposing beyond your LAN
        .with_state(state)
}
