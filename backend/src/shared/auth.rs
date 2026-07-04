//! Cross-cutting access control: a single shared bearer token guards /api.
//! `AUTH_TOKEN` unset = auth disabled (existing LAN deployments keep working).
//! 401 is returned as a raw Response — `error.rs` (no 401 variant) stays untouched.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::shared::state::AppState;

/// Auth middleware. Passes through when `AUTH_TOKEN` is unset; otherwise requires
/// a matching `Authorization: Bearer <token>` (constant-time compared) or 401s.
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Auth disabled (no token configured) → let everything through.
    let Some(expected) = state.config.auth_token.as_deref() else {
        return next.run(req).await;
    };

    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    match provided {
        Some(token) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => {
            next.run(req).await
        }
        _ => unauthorized(),
    }
}

/// 401 response (with WWW-Authenticate). Body matches the AppError JSON shape.
pub fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response()
}

/// Constant-time byte comparison (avoids timing leaks). Length is not secret, so
/// a length mismatch returns false — but without an early-return data-dependent
/// on content.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::config::AppConfig;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use std::sync::Arc;
    use tower::ServiceExt; // oneshot

    #[test]
    fn cteq_equal_returns_true() {
        assert!(constant_time_eq(b"secret", b"secret"));
    }

    #[test]
    fn cteq_differs_returns_false() {
        assert!(!constant_time_eq(b"secret", b"secrxt"));
    }

    #[test]
    fn cteq_different_length_returns_false() {
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
    }

    #[test]
    fn cteq_empty_vs_empty_true() {
        assert!(constant_time_eq(b"", b""));
    }

    fn test_state(token: Option<&str>) -> AppState {
        // connect_lazy never opens a socket; protected routes aren't exercised, so
        // the bogus URL is fine — we only check the 401/pass-through decision.
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/invalid")
            .unwrap();
        AppState {
            db,
            config: Arc::new(AppConfig::for_test(token.map(|s| s.to_string()))),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::new(),
        }
    }

    fn protected_app(state: AppState) -> Router {
        Router::new()
            .route("/api/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                require_auth,
            ))
            .with_state(state)
    }

    async fn status_of(app: Router, req: Request<Body>) -> StatusCode {
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn disabled_passes_without_header() {
        let app = protected_app(test_state(None));
        let req = Request::builder()
            .uri("/api/x")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn enabled_rejects_missing_header() {
        let app = protected_app(test_state(Some("s3cr3t")));
        let req = Request::builder()
            .uri("/api/x")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn enabled_rejects_wrong_token() {
        let app = protected_app(test_state(Some("s3cr3t")));
        let req = Request::builder()
            .uri("/api/x")
            .header(header::AUTHORIZATION, "Bearer wrong")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn enabled_accepts_correct_token() {
        let app = protected_app(test_state(Some("s3cr3t")));
        let req = Request::builder()
            .uri("/api/x")
            .header(header::AUTHORIZATION, "Bearer s3cr3t")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn enabled_rejects_non_bearer_scheme() {
        let app = protected_app(test_state(Some("s3cr3t")));
        let req = Request::builder()
            .uri("/api/x")
            .header(header::AUTHORIZATION, "Basic s3cr3t")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::UNAUTHORIZED);
    }
}
