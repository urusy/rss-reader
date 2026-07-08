pub mod annotations;
pub mod articles;
pub mod ask;
pub mod auth;
pub mod automation_rules;
pub mod backup;
pub mod clustering;
pub mod digest;
pub mod extraction;
pub mod feed_discovery;
pub mod feed_health;
pub mod feed_overview;
pub mod feeds;
pub mod folders;
pub mod health;
pub mod instapaper;
pub mod llm_settings;
pub mod mute_rules;
pub mod notifications;
pub mod opml;
pub mod relevance;
pub mod saved_views;
pub mod search;
pub mod stats;
pub mod tags;
pub mod usage;

use axum::http::{header, HeaderValue, Method};
use axum::{middleware, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::shared::auth::require_auth;
use crate::shared::config::AppConfig;
use crate::shared::state::AppState;

/// Compose every vertical slice into the top-level router.
///
/// Each feature owns its own `routes()` returning a `Router<AppState>`. Adding a
/// feature = add one module + one `.merge()` line. Existing slices stay untouched.
///
/// Routes split into public (health + auth setup/login/status) and protected
/// (the rest). The protected subrouter carries `require_auth`, which demands a
/// valid session cookie — until the initial password setup completes, every
/// protected route answers 401 (secure by default).
pub fn router(state: AppState) -> Router {
    let public = Router::new().merge(health::routes()).merge(auth::routes());

    let protected = Router::new()
        .merge(auth::protected_routes())
        .merge(feeds::routes())
        .merge(feed_discovery::routes())
        .merge(articles::routes())
        .merge(annotations::routes())
        .merge(ask::routes())
        .merge(extraction::routes())
        .merge(stats::routes())
        .merge(feed_overview::routes())
        .merge(feed_health::routes())
        .merge(folders::routes())
        .merge(instapaper::routes())
        .merge(llm_settings::routes())
        .merge(search::routes())
        .merge(saved_views::routes())
        .merge(opml::routes())
        .merge(mute_rules::routes())
        .merge(notifications::routes())
        .merge(tags::routes())
        .merge(digest::routes())
        .merge(relevance::routes())
        .merge(clustering::routes())
        .merge(automation_rules::routes())
        .merge(backup::routes())
        .merge(usage::routes())
        // 利用記録は require_auth の内側（コード上は先 = 実行順は認証の後）。
        // 未認証 401 は記録されない。
        .layer(middleware::from_fn(usage::track_usage))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let cors = cors_layer(&state.config);
    Router::new()
        .merge(public)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// CORS 方針（監査 #5）: 既定は**クロスオリジン不許可**。nginx（本番）も Vite
/// proxy（開発）も /api を同一オリジンに見せるので、通常 CORS は不要。別オリジン
/// から叩きたいときだけ `CORS_ALLOWED_ORIGINS`（カンマ区切り）で明示的に開ける。
fn cors_layer(config: &AppConfig) -> CorsLayer {
    if config.cors_allowed_origins.is_empty() {
        return CorsLayer::new(); // ヘッダを一切付けない = same-origin only
    }
    let origins: Vec<HeaderValue> = config
        .cors_allowed_origins
        .iter()
        .filter_map(|o| {
            o.parse()
                .inspect_err(|e| tracing::warn!(origin = %o, error = %e, "invalid CORS origin"))
                .ok()
        })
        .collect();
    // Cookie セッションを別オリジンから送るには credentials の明示許可が要る
    // （許可リストが明示オリジンのみなので wildcard 制約には抵触しない）。
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
        ])
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn test_state(origins: Vec<String>) -> AppState {
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/invalid")
            .unwrap();
        let mut config = AppConfig::for_test();
        config.cors_allowed_origins = origins;
        AppState {
            db,
            config: Arc::new(config),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::new(),
            login_limiter: Arc::new(std::sync::Mutex::new(
                crate::shared::auth::LoginLimiter::default(),
            )),
        }
    }

    async fn preflight(state: AppState) -> axum::http::Response<Body> {
        router(state)
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/health")
                    .header("origin", "https://evil.example")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    // 既定（未設定）: クロスオリジンを許可するヘッダが付かない。
    #[tokio::test]
    async fn cors_disabled_by_default() {
        let resp = preflight(test_state(vec![])).await;
        assert!(
            resp.headers().get("access-control-allow-origin").is_none(),
            "no allow-origin header expected by default"
        );
    }

    // 許可リストにあるオリジンだけ通る。
    #[tokio::test]
    async fn cors_allows_only_configured_origins() {
        let state = test_state(vec!["https://reader.example".to_string()]);
        let resp = router(state)
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/health")
                    .header("origin", "https://reader.example")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://reader.example")
        );

        // リスト外オリジンには allow-origin が付かない。
        let resp = preflight(test_state(vec!["https://reader.example".to_string()])).await;
        assert!(resp.headers().get("access-control-allow-origin").is_none());
    }
}
