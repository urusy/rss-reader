use std::net::SocketAddr;

/// Application configuration, loaded from environment variables.
///
/// Keep this struct flat and explicit. Each field maps to one env var so the
/// 12-factor contract stays obvious to anyone reading `.env.example`.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Optional until the summarization/translation feature is enabled.
    pub anthropic_api_key: Option<String>,
    /// Anthropic model id used for summaries/translation.
    pub anthropic_model: String,
    /// How often the scheduler refreshes feeds, in seconds.
    pub feed_refresh_interval_secs: u64,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;

        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()?;

        let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|v| !v.is_empty());

        let anthropic_model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        let feed_refresh_interval_secs = std::env::var("FEED_REFRESH_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(900);

        Ok(Self {
            database_url,
            bind_addr,
            anthropic_api_key,
            anthropic_model,
            feed_refresh_interval_secs,
        })
    }
}
