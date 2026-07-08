pub mod domain;
pub mod handler;
pub mod repository;
pub mod service;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;

use crate::shared::state::AppState;
use crate::shared::usage::{record, UsageEvent};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/usage/summary", get(handler::summary))
        .route("/api/usage/events", post(handler::record_event))
}

/// 全 protected ルートに掛かる利用記録ミドルウェア。
///
/// features/mod.rs で require_auth の内側（コード上は直前の .layer 行）に積む —
/// 認証を通過したリクエストだけが到達し、401 は記録されない。
/// 対応表（domain::feature_key）にないルートは何もしない。record は
/// 非ブロッキング（unbounded send）なので応答遅延を持ち込まない。
pub async fn track_usage(req: Request, next: Next) -> Response {
    let feature = req
        .extensions()
        .get::<MatchedPath>()
        .and_then(|p| domain::feature_key(req.method(), p.as_str()));
    let resp = next.run(req).await;
    if let Some(feature) = feature {
        record(UsageEvent::Server {
            feature,
            status: resp.status().as_u16(),
        });
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request as HttpRequest, StatusCode};
    use axum::middleware::{self, from_fn};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    /// 本設計の唯一の技術リスク「merge 構成 + from_fn の層で MatchedPath が
    /// extensions に入っているか」を oneshot で実証する。
    ///
    /// グローバル sink（OnceLock）はテスト間で共有され観測が不安定になるため、
    /// ここでは track_usage と同じ抽出ロジックをローカルの Arc<Mutex> に
    /// 記録するプローブで検証する（対応表の網羅は domain のテストが担う）。
    #[tokio::test]
    async fn matched_path_is_visible_to_from_fn_middleware_on_merged_router() {
        type Seen = Vec<(String, Option<&'static str>)>;
        let seen: Arc<Mutex<Seen>> = Arc::new(Mutex::new(Vec::new()));

        let probe_seen = seen.clone();
        let probe = move |req: Request, next: Next| {
            let probe_seen = probe_seen.clone();
            async move {
                let matched = req
                    .extensions()
                    .get::<MatchedPath>()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_else(|| "<missing>".into());
                let key = req
                    .extensions()
                    .get::<MatchedPath>()
                    .and_then(|p| domain::feature_key(req.method(), p.as_str()));
                probe_seen.lock().unwrap().push((matched, key));
                next.run(req).await
            }
        };

        // features/mod.rs と同じ構成（merge したサブルーターに .layer）を再現。
        let sub = Router::new()
            .route("/api/articles/{id}/read", post(|| async { "ok" }))
            .route("/api/articles", get(|| async { "ok" }));
        let app: Router = Router::new()
            .merge(sub)
            .layer(middleware::from_fn(probe))
            .with_state(());

        let resp = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method(Method::POST)
                    .uri("/api/articles/0b5f8f6e-0000-0000-0000-000000000000/read")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::GET)
                    .uri("/api/articles")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0],
            ("/api/articles/{id}/read".to_string(), Some("mark_read")),
            "MatchedPath must carry the route template, and feature_key must map it"
        );
        assert_eq!(
            seen[1],
            ("/api/articles".to_string(), None),
            "untracked route must map to None"
        );
    }

    /// track_usage 本体もパニックせず応答を素通しすること（sink 未 install = no-op）。
    #[tokio::test]
    async fn track_usage_passes_response_through() {
        let app: Router = Router::new()
            .route("/api/search", get(|| async { "found" }))
            .layer(from_fn(track_usage))
            .with_state(());
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::GET)
                    .uri("/api/search")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
