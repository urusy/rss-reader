//! Cross-cutting access control: server-side sessions in an HttpOnly cookie
//! guard /api. A session is valid when the SHA-256 of the cookie token matches
//! an unexpired `auth_sessions` row. 401/403/429 are raw Responses — `error.rs`
//! (no such variants) stays untouched.
//!
//! セッションの発行/失効は `features/auth`（縦スライス）が持ち、ここは
//! 「届いた Cookie が有効か」の判定・スライディング延長・CSRF(Origin) 検証
//! という横断関心だけを持つ。

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::shared::state::AppState;

/// セッション Cookie 名。`__Host-` prefix は Secure 必須のため使えない
/// （家庭内 LAN は http 運用があり、Secure は COOKIE_SECURE で opt-in）。
pub const SESSION_COOKIE: &str = "rss_session";

/// セッション有効期間（発行時とスライディング延長時に使う）。
pub const SESSION_TTL_DAYS: i64 = 30;

/// last_seen がこれより古いリクエストでだけ期限を延長する（毎リクエスト書込み回避）。
const TOUCH_AFTER: chrono::Duration = chrono::Duration::hours(1);

/// Cookie トークンの照合用ハッシュ（SHA-256 → base64url no pad）。DB には
/// これだけを保存し、検索もハッシュで行う（平文比較のタイミングリークが無い）。
pub fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    Base64UrlUnpadded::encode_string(&digest)
}

/// 認証済みリクエストの現セッション。ミドルウェアが request extension に挿入し、
/// logout / パスワード変更 / セッション一覧のハンドラが参照する。
#[derive(Debug, Clone, Copy)]
pub struct CurrentSession {
    pub id: Uuid,
}

/// リクエストヘッダの Cookie から有効セッションを引く。ミドルウェアと
/// GET /api/auth/status（公開ルート）が共用する。有効なら必要に応じて
/// スライディング延長も行う。
pub async fn session_from_headers(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<Option<CurrentSession>, sqlx::Error> {
    let jar = CookieJar::from_headers(headers);
    let Some(cookie) = jar.get(SESSION_COOKIE) else {
        return Ok(None);
    };
    let hash = hash_token(cookie.value());
    let row: Option<(Uuid, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, last_seen_at FROM auth_sessions WHERE token_hash = $1 AND expires_at > now()",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;
    let Some((id, last_seen_at)) = row else {
        return Ok(None);
    };
    if Utc::now() - last_seen_at > TOUCH_AFTER {
        sqlx::query("UPDATE auth_sessions SET last_seen_at = now(), expires_at = $2 WHERE id = $1")
            .bind(id)
            .bind(Utc::now() + chrono::Duration::days(SESSION_TTL_DAYS))
            .execute(&state.db)
            .await?;
    }
    Ok(Some(CurrentSession { id }))
}

/// 認証ミドルウェア。state-changing メソッドは先に Origin を検証（CSRF の
/// 二重防御。SameSite=Strict が第一防衛線）し、次にセッション Cookie を検証する。
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok());
        // Origin 欠落は許可（curl 等の非ブラウザ。ブラウザは POST に必ず付ける）。
        if let Some(origin) = origin {
            if !origin_allowed(origin, host, &state.config.cors_allowed_origins) {
                return forbidden();
            }
        }
    }

    match session_from_headers(&state, req.headers()).await {
        Ok(Some(current)) => {
            req.extensions_mut().insert(current);
            next.run(req).await
        }
        Ok(None) => unauthorized(),
        Err(e) => {
            tracing::error!(error = %e, "session lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
}

/// CSRF 用の Origin 検証（純粋関数）。Origin のオーソリティ部が Host ヘッダと
/// 一致するか、明示許可リスト（CORS_ALLOWED_ORIGINS）に完全一致すれば許可。
pub fn origin_allowed(origin: &str, host: Option<&str>, allowed_origins: &[String]) -> bool {
    if allowed_origins
        .iter()
        .any(|o| o.eq_ignore_ascii_case(origin))
    {
        return true;
    }
    let authority = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    match (authority, host) {
        (Some(a), Some(h)) => a.eq_ignore_ascii_case(h),
        _ => false, // "null" やその他スキームは拒否
    }
}

/// 401 応答。本文は AppError の JSON 形に合わせる。
pub fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response()
}

/// 403 応答（Origin 不一致 = CSRF 疑い）。
pub fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "cross-origin request rejected" })),
    )
        .into_response()
}

/// 429 応答（ログイン試行のバックオフ中）。Retry-After 秒を添える。
pub fn too_many_requests(retry_after: Duration) -> Response {
    let secs = retry_after.as_secs().max(1);
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::RETRY_AFTER, secs.to_string())],
        Json(json!({ "error": "too many attempts", "retry_after_secs": secs })),
    )
        .into_response()
}

/// Constant-time byte comparison (avoids timing leaks). Length is not secret, so
/// a length mismatch returns false — but without an early-return data-dependent
/// on content. セッション照合はハッシュ検索で置き換えたが、backup の
/// X-Backup-Token など生トークンを直接比較する箇所が引き続き使う。
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

/// ログイン/パスワード検証失敗のグローバル指数バックオフ。単一ユーザー前提
/// なので per-IP を持たない（X-Forwarded-For 偽装による回避も構造的に無い）。
/// 5 連続失敗で 30 秒、以降失敗ごとに倍増、上限 15 分。時刻は注入（テスト可能）。
#[derive(Debug, Default)]
pub struct LoginLimiter {
    consecutive_failures: u32,
    locked_until: Option<Instant>,
}

const LOCK_THRESHOLD: u32 = 5;
const LOCK_BASE_SECS: u64 = 30;
const LOCK_MAX_SECS: u64 = 900;

impl LoginLimiter {
    /// 試行してよいか。ロック中なら残り時間を Err で返す。
    pub fn check(&self, now: Instant) -> Result<(), Duration> {
        match self.locked_until {
            Some(until) if until > now => Err(until - now),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&mut self, now: Instant) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= LOCK_THRESHOLD {
            let exp = self.consecutive_failures - LOCK_THRESHOLD;
            let secs = LOCK_BASE_SECS
                .saturating_mul(1u64 << exp.min(5))
                .min(LOCK_MAX_SECS);
            self.locked_until = Some(now + Duration::from_secs(secs));
        }
    }

    pub fn record_success(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::config::AppConfig;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt; // oneshot

    // ---- origin_allowed --------------------------------------------------

    #[test]
    fn origin_matching_host_is_allowed() {
        assert!(origin_allowed(
            "http://192.168.1.10:8081",
            Some("192.168.1.10:8081"),
            &[]
        ));
        assert!(origin_allowed(
            "https://reader.example",
            Some("reader.example"),
            &[]
        ));
    }

    #[test]
    fn origin_case_is_ignored() {
        assert!(origin_allowed(
            "http://LOCALHOST:3000",
            Some("localhost:3000"),
            &[]
        ));
    }

    #[test]
    fn origin_mismatch_is_rejected() {
        assert!(!origin_allowed(
            "http://evil.example",
            Some("reader.lan"),
            &[]
        ));
        assert!(!origin_allowed(
            "http://reader.lan:9999",
            Some("reader.lan:8081"),
            &[]
        ));
    }

    #[test]
    fn origin_null_or_other_scheme_is_rejected() {
        assert!(!origin_allowed("null", Some("reader.lan"), &[]));
        assert!(!origin_allowed(
            "chrome-extension://abc",
            Some("reader.lan"),
            &[]
        ));
    }

    #[test]
    fn origin_missing_host_is_rejected() {
        assert!(!origin_allowed("http://reader.lan", None, &[]));
    }

    #[test]
    fn origin_in_explicit_allowlist_is_allowed() {
        assert!(origin_allowed(
            "https://other.example",
            Some("reader.lan"),
            &["https://other.example".to_string()]
        ));
    }

    // ---- hash_token ------------------------------------------------------

    #[test]
    fn hash_token_is_deterministic_and_url_safe() {
        let a = hash_token("some-token");
        assert_eq!(a, hash_token("some-token"));
        assert_ne!(a, hash_token("other-token"));
        assert_eq!(a.len(), 43); // SHA-256 → base64url no pad
    }

    // ---- constant_time_eq --------------------------------------------------

    #[test]
    fn cteq_equal_returns_true() {
        assert!(constant_time_eq(b"secret", b"secret"));
    }

    #[test]
    fn cteq_differs_returns_false() {
        assert!(!constant_time_eq(b"secret", b"secrxt"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
    }

    // ---- LoginLimiter ----------------------------------------------------

    #[test]
    fn limiter_allows_below_threshold() {
        let mut l = LoginLimiter::default();
        let now = Instant::now();
        for _ in 0..4 {
            l.record_failure(now);
        }
        assert!(l.check(now).is_ok());
    }

    #[test]
    fn limiter_locks_at_threshold_then_doubles() {
        let mut l = LoginLimiter::default();
        let now = Instant::now();
        for _ in 0..5 {
            l.record_failure(now);
        }
        // 5 失敗 → 30 秒ロック。
        let remaining = l.check(now).unwrap_err();
        assert!(remaining <= Duration::from_secs(30));
        assert!(remaining > Duration::from_secs(29));
        // 30 秒経過後は再試行できる。
        assert!(l.check(now + Duration::from_secs(31)).is_ok());
        // 6 失敗目 → 60 秒。
        l.record_failure(now);
        let remaining = l.check(now).unwrap_err();
        assert!(remaining > Duration::from_secs(59));
    }

    #[test]
    fn limiter_caps_at_15_minutes() {
        let mut l = LoginLimiter::default();
        let now = Instant::now();
        for _ in 0..30 {
            l.record_failure(now);
        }
        let remaining = l.check(now).unwrap_err();
        assert!(remaining <= Duration::from_secs(900));
        assert!(remaining > Duration::from_secs(899));
    }

    #[test]
    fn limiter_success_resets() {
        let mut l = LoginLimiter::default();
        let now = Instant::now();
        for _ in 0..5 {
            l.record_failure(now);
        }
        l.record_success();
        assert!(l.check(now).is_ok());
    }

    // ---- middleware (DB に触れない経路のみ; 正常系は smoke テストで) ------

    fn test_state() -> AppState {
        // connect_lazy never opens a socket; the paths under test reject the
        // request before any query runs.
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/invalid")
            .unwrap();
        AppState {
            db,
            config: Arc::new(AppConfig::for_test()),
            http: reqwest::Client::new(),
            http_external: reqwest::Client::new(),
            login_limiter: Arc::new(Mutex::new(LoginLimiter::default())),
        }
    }

    fn protected_app(state: AppState) -> Router {
        Router::new()
            .route("/api/x", get(|| async { "ok" }).post(|| async { "ok" }))
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
    async fn rejects_request_without_cookie() {
        let app = protected_app(test_state());
        let req = Request::builder()
            .uri("/api/x")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_cross_origin_post_before_auth() {
        let app = protected_app(test_state());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/x")
            .header(header::ORIGIN, "http://evil.example")
            .header(header::HOST, "reader.lan")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn same_origin_post_without_cookie_is_401() {
        let app = protected_app(test_state());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/x")
            .header(header::ORIGIN, "http://reader.lan")
            .header(header::HOST, "reader.lan")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cross_origin_get_skips_origin_check_but_needs_cookie() {
        // GET は Origin 検証対象外（副作用なし・SameSite で Cookie も付かない）。
        let app = protected_app(test_state());
        let req = Request::builder()
            .uri("/api/x")
            .header(header::ORIGIN, "http://evil.example")
            .header(header::HOST, "reader.lan")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status_of(app, req).await, StatusCode::UNAUTHORIZED);
    }
}
