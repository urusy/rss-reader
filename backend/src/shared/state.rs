use std::sync::{Arc, Mutex};

use sqlx::PgPool;

use super::auth::LoginLimiter;
use super::config::AppConfig;

/// Shared application state, cheap to clone (Arc + pool handle + reqwest client).
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
    /// ログイン失敗の指数バックオフ状態（プロセス内・全接続共有）。
    pub login_limiter: Arc<Mutex<LoginLimiter>>,
    /// Client for fixed, trusted endpoints (Anthropic API, push services,
    /// Instapaper). Follows redirects automatically.
    pub http: reqwest::Client,
    /// Client for user-supplied URLs (feeds, discovery, extraction). Redirects
    /// are disabled; `shared::fetch::safe_get` follows them manually with a
    /// per-hop SSRF check. Never use this directly — go through `safe_get`.
    pub http_external: reqwest::Client,
}
