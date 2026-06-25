use std::sync::Arc;

use sqlx::PgPool;

use super::config::AppConfig;

/// Shared application state, cheap to clone (Arc + pool handle + reqwest client).
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<AppConfig>,
    pub http: reqwest::Client,
}
