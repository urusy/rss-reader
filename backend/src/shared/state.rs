use std::sync::Arc;

use sqlx::PgPool;

use super::config::AppConfig;

/// Shared application state, cheap to clone (Arc + pool handle + reqwest client).
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
    /// Client for fixed, trusted endpoints (Anthropic API, push services,
    /// Instapaper). Follows redirects automatically.
    pub http: reqwest::Client,
    /// Client for user-supplied URLs (feeds, discovery, extraction). Redirects
    /// are disabled; `shared::fetch::safe_get` follows them manually with a
    /// per-hop SSRF check. Never use this directly — go through `safe_get`.
    pub http_external: reqwest::Client,
}
