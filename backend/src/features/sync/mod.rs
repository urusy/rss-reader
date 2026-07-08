//! Google Reader 互換同期 API（#29）。docs/design/29-sync-api.md 参照。
//!
//! ルートは 2 面: GReader 面（`routes` — public 側に merge、`GoogleLogin auth=`
//! トークンで認証）とトークン管理面（`protected_routes` — Cookie セッション
//! 保護の通常 /api）。GReader 面は `SYNC_API_ENABLED=true` のときだけ
//! features/mod.rs が merge する（無効時は 404 — 存在自体を隠す）。

pub mod domain;
mod handler;
pub mod repository;
mod service;
pub mod wire;

use axum::middleware;
use axum::routing::{any, delete, get, post};
use axum::Router;

use crate::features::usage;
use crate::shared::state::AppState;

/// GReader 面。catch-all も含め全ルートが `require_sync_auth` の内側
/// （未認証の未知パスは 401 — プローブに存在を教えない）。
/// 利用記録（track_usage）は認証の内側 = 401 は記録されない（protected 側と
/// 同型の並び。GReader トラフィックも計測対象 — ユーザー決定 2026-07-07）。
pub fn routes(state: &AppState) -> Router<AppState> {
    let api = Router::new()
        .route("/token", get(handler::token))
        .route("/user-info", get(handler::user_info))
        .route("/tag/list", get(handler::tag_list))
        .route("/subscription/list", get(handler::subscription_list))
        .route("/subscription/quickadd", post(handler::quickadd))
        .route("/subscription/edit", post(handler::subscription_edit))
        .route("/stream/items/ids", get(handler::stream_items_ids))
        .route(
            "/stream/items/contents",
            post(handler::stream_items_contents),
        )
        .route("/stream/contents", get(handler::stream_contents_bare))
        .route(
            "/stream/contents/{*stream}",
            get(handler::stream_contents_path),
        )
        .route("/edit-tag", post(handler::edit_tag))
        .route("/mark-all-as-read", post(handler::mark_all_as_read))
        .route("/rename-tag", post(handler::rename_tag))
        .route("/disable-tag", post(handler::disable_tag))
        .route("/unread-count", get(handler::unread_count))
        .route("/{*rest}", any(handler::catch_all))
        .layer(middleware::from_fn(usage::track_usage))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handler::require_sync_auth,
        ));
    Router::new()
        // ★POST のみ登録（GET は 405 — Passwd がクエリ = アクセスログ漏えいの排除）。
        .route("/accounts/ClientLogin", post(handler::client_login))
        .nest("/reader/api/0", api)
}

/// トークン管理面（features/mod.rs の protected へ merge）。
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/api/sync/tokens", get(handler::list_sync_tokens))
        .route("/api/sync/tokens/{id}", delete(handler::revoke_sync_token))
}

#[cfg(test)]
mod tests {
    //! §9.2 ルータテスト。connect_lazy プール（ソケットを開かない）+ oneshot で、
    //! DB 到達前に決まる認証境界を固定する。
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use axum::Router;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    use crate::shared::auth::LoginLimiter;
    use crate::shared::config::AppConfig;
    use crate::shared::state::AppState;

    fn state_with(sync_enabled: bool) -> AppState {
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/invalid")
            .unwrap();
        let mut config = AppConfig::for_test();
        config.sync_api_enabled = sync_enabled;
        AppState {
            db,
            config: Arc::new(config),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::new(),
            login_limiter: Arc::new(Mutex::new(LoginLimiter::default())),
        }
    }

    /// features/mod.rs の本物のルーター（public/protected 分割・条件 merge 込み）。
    fn app(sync_enabled: bool) -> Router {
        crate::features::router(state_with(sync_enabled))
    }

    async fn send(app: Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, headers, String::from_utf8_lossy(&body).into_owned())
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn disabled_by_default_sync_routes_do_not_exist() {
        // AppConfig::for_test は sync_api_enabled=false（既定 off の検証を兼ねる）。
        // このアプリでは未マッチのパスは protected 側の require_auth（フォール
        // バックにも layer が掛かる axum の挙動）で汎用 401 JSON になる。
        // 「無効時に sync ルートが存在しない」ことは、任意の未知パスと完全に
        // 同じ応答（= GReader 固有ヘッダ・text/plain が出ない）ことで検証する。
        let (unknown_status, unknown_headers, unknown_body) =
            send(app(false), get("/definitely/not/a/route")).await;
        for (method, uri) in [
            (Method::GET, "/reader/api/0/tag/list"),
            (Method::POST, "/accounts/ClientLogin"),
        ] {
            let (status, headers, body) = send(
                app(false),
                Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, unknown_status, "{method} {uri}");
            assert_eq!(body, unknown_body, "{method} {uri}");
            assert!(headers.get("Google-Bad-Token").is_none(), "{method} {uri}");
            assert_eq!(
                headers.get(header::CONTENT_TYPE),
                unknown_headers.get(header::CONTENT_TYPE),
                "{method} {uri}"
            );
        }
    }

    #[tokio::test]
    async fn unauthenticated_is_401_with_both_bad_token_headers() {
        let (status, headers, body) = send(app(true), get("/reader/api/0/tag/list")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers.get("Google-Bad-Token").unwrap(), "true");
        assert_eq!(headers.get("X-Reader-Google-Bad-Token").unwrap(), "true");
        assert!(headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/plain"));
        assert_eq!(body, "Unauthorized");
    }

    #[tokio::test]
    async fn unknown_path_unauthenticated_is_401_not_200() {
        // catch-all が認証ミドルウェアの内側にある証明。200 `[]` になったら退行。
        let (status, _, _) = send(app(true), get("/reader/api/0/whatever/probe")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_scheme_is_401() {
        let req = Request::builder()
            .uri("/reader/api/0/tag/list")
            .header(header::AUTHORIZATION, "Bearer sometoken")
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(app(true), req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_client_login_is_405() {
        // GET 不可（Passwd がクエリ文字列 = ログ漏えいの排除）。
        let (status, _, _) = send(app(true), get("/accounts/ClientLogin")).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn session_cookie_does_not_authenticate_sync_routes() {
        // ★不変条件: sync は Cookie を読まない。
        let req = Request::builder()
            .uri("/reader/api/0/tag/list")
            .header(header::COOKIE, "rss_session=validlooking")
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(app(true), req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn google_login_header_does_not_authenticate_api_routes() {
        // ★不変条件: GoogleLogin トークンは /api/* で受理されない。
        let req = Request::builder()
            .uri("/api/feeds")
            .header(header::AUTHORIZATION, "GoogleLogin auth=sometoken")
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(app(true), req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn client_login_missing_body_is_403_bad_authentication() {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/accounts/ClientLogin")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::empty())
            .unwrap();
        let (status, _, body) = send(app(true), req).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, "Error=BadAuthentication\n");
    }

    #[tokio::test]
    async fn valid_format_token_with_unreachable_db_fails_closed_500() {
        // トークン形式は正しいが DB に到達できない → 500（認証を通さない）。
        let req = Request::builder()
            .uri("/reader/api/0/tag/list")
            .header(
                header::AUTHORIZATION,
                "GoogleLogin auth=abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
            )
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(app(true), req).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
