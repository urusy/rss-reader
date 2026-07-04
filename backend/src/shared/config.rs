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
    /// Shared bearer token guarding /api. None (unset) = auth disabled (LAN default).
    pub auth_token: Option<String>,
    /// Token gating /api/backup/*. None = backup feature disabled (503).
    pub backup_token: Option<String>,
    /// Output dir for the optional scheduled pg_dump. None = scheduler disabled.
    pub backup_dir: Option<String>,
    /// Interval (secs) for the optional scheduled pg_dump. None = scheduler disabled.
    pub backup_pgdump_interval_secs: Option<u64>,
    /// Anthropic model id used for summaries/translation.
    pub anthropic_model: String,
    /// How often the scheduler refreshes feeds, in seconds.
    pub feed_refresh_interval_secs: u64,
    /// Opt-in: extract full article bodies during crawl (best-effort). Default false.
    pub extract_on_crawl: bool,
    /// Opt-in: let feed/extraction fetches reach private/loopback addresses
    /// (LAN-internal feeds). Default false = SSRF guard fully on.
    pub allow_private_networks: bool,
    /// Cross-origin origins allowed on /api (comma-separated env). Empty =
    /// no CORS (same-origin only) — nginx / Vite proxy make the app
    /// same-origin, so cross-origin access is opt-in.
    pub cors_allowed_origins: Vec<String>,
    /// Max bytes of a fetched page we will attempt to extract (guards memory).
    pub extract_max_bytes: usize,
    /// Minimum plain-text chars for an extraction to count as "real body".
    pub extract_min_chars: usize,
    /// AI daily digest (#23): enable the scheduled daily generation.
    pub digest_enabled: bool,
    /// UTC hour (0-23) to run daily digest generation. Default 21 (~JST 06:00).
    pub digest_hour_utc: u32,
    /// Output language for the digest. Default "ja".
    pub digest_lang: String,
    /// Optional SMTP for digest email (sent only when host/from/to all set).
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub digest_email_from: Option<String>,
    pub digest_email_to: Option<String>,
    /// Semantic clustering (#26): enable the scheduled re-clustering loop.
    pub clustering_enabled: bool,
    pub clustering_interval_secs: u64,
    pub clustering_window_hours: i32,
    pub clustering_max_articles: i32,
    pub cluster_topic_threshold: f32,
    pub cluster_dup_threshold: f32,
    pub cluster_min_size: i32,
    pub cluster_summary_lang: String,
    /// Web Push VAPID keys (#31). Both None (unset) = push disabled (503 NotEnabled).
    /// Public key = base64url uncompressed point (served to the SW as applicationServerKey).
    /// Private key = base64url raw P-256 scalar (used to sign VAPID JWTs).
    pub vapid_public_key: Option<String>,
    pub vapid_private_key: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;

        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()?;

        let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|v| !v.is_empty());

        let auth_token = std::env::var("AUTH_TOKEN").ok().filter(|v| !v.is_empty());

        let backup_token = std::env::var("BACKUP_TOKEN").ok().filter(|v| !v.is_empty());
        let backup_dir = std::env::var("BACKUP_DIR").ok().filter(|v| !v.is_empty());
        let backup_pgdump_interval_secs = std::env::var("BACKUP_PGDUMP_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok());

        let anthropic_model =
            std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        let feed_refresh_interval_secs = std::env::var("FEED_REFRESH_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(900);

        let extract_on_crawl = std::env::var("EXTRACT_ON_CRAWL")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let allow_private_networks = std::env::var("ALLOW_PRIVATE_NETWORKS")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let cors_allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let extract_max_bytes = std::env::var("EXTRACT_MAX_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3_000_000);

        let extract_min_chars = std::env::var("EXTRACT_MIN_CHARS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);

        let digest_enabled = std::env::var("DIGEST_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let digest_hour_utc = std::env::var("DIGEST_HOUR_UTC")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|h| *h <= 23)
            .unwrap_or(21);
        let digest_lang = std::env::var("DIGEST_LANG").unwrap_or_else(|_| "ja".to_string());
        let smtp_host = std::env::var("SMTP_HOST").ok().filter(|v| !v.is_empty());
        let smtp_port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(587);
        let smtp_username = std::env::var("SMTP_USERNAME")
            .ok()
            .filter(|v| !v.is_empty());
        let smtp_password = std::env::var("SMTP_PASSWORD")
            .ok()
            .filter(|v| !v.is_empty());
        let digest_email_from = std::env::var("DIGEST_EMAIL_FROM")
            .ok()
            .filter(|v| !v.is_empty());
        let digest_email_to = std::env::var("DIGEST_EMAIL_TO")
            .ok()
            .filter(|v| !v.is_empty());

        let clustering_enabled = std::env::var("CLUSTERING_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let clustering_interval_secs = std::env::var("CLUSTERING_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600);
        let clustering_window_hours = std::env::var("CLUSTERING_WINDOW_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(48);
        let clustering_max_articles = std::env::var("CLUSTERING_MAX_ARTICLES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);
        let cluster_topic_threshold = std::env::var("CLUSTER_TOPIC_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.3_f32);
        let cluster_dup_threshold = std::env::var("CLUSTER_DUP_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.6_f32);
        let cluster_min_size = std::env::var("CLUSTER_MIN_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);
        let cluster_summary_lang =
            std::env::var("CLUSTER_SUMMARY_LANG").unwrap_or_else(|_| "ja".to_string());

        let vapid_public_key = std::env::var("VAPID_PUBLIC_KEY")
            .ok()
            .filter(|v| !v.is_empty());
        let vapid_private_key = std::env::var("VAPID_PRIVATE_KEY")
            .ok()
            .filter(|v| !v.is_empty());

        Ok(Self {
            database_url,
            bind_addr,
            anthropic_api_key,
            auth_token,
            backup_token,
            backup_dir,
            backup_pgdump_interval_secs,
            anthropic_model,
            feed_refresh_interval_secs,
            extract_on_crawl,
            allow_private_networks,
            cors_allowed_origins,
            extract_max_bytes,
            extract_min_chars,
            digest_enabled,
            digest_hour_utc,
            digest_lang,
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            digest_email_from,
            digest_email_to,
            clustering_enabled,
            clustering_interval_secs,
            clustering_window_hours,
            clustering_max_articles,
            cluster_topic_threshold,
            cluster_dup_threshold,
            cluster_min_size,
            cluster_summary_lang,
            vapid_public_key,
            vapid_private_key,
        })
    }

    /// Minimal config for unit/integration tests. Only `auth_token` is meaningful;
    /// other fields get harmless defaults. Test-only, never used in production.
    #[cfg(test)]
    pub fn for_test(auth_token: Option<String>) -> Self {
        Self {
            database_url: "postgres://invalid/invalid".to_string(),
            bind_addr: "0.0.0.0:8080".parse().unwrap(),
            anthropic_api_key: None,
            auth_token,
            backup_token: None,
            backup_dir: None,
            backup_pgdump_interval_secs: None,
            anthropic_model: "claude-sonnet-4-6".to_string(),
            feed_refresh_interval_secs: 900,
            extract_on_crawl: false,
            allow_private_networks: false,
            cors_allowed_origins: Vec::new(),
            extract_max_bytes: 3_000_000,
            extract_min_chars: 200,
            digest_enabled: false,
            digest_hour_utc: 21,
            digest_lang: "ja".to_string(),
            smtp_host: None,
            smtp_port: 587,
            smtp_username: None,
            smtp_password: None,
            digest_email_from: None,
            digest_email_to: None,
            clustering_enabled: false,
            clustering_interval_secs: 3600,
            clustering_window_hours: 48,
            clustering_max_articles: 500,
            cluster_topic_threshold: 0.3,
            cluster_dup_threshold: 0.6,
            cluster_min_size: 2,
            cluster_summary_lang: "ja".to_string(),
            vapid_public_key: None,
            vapid_private_key: None,
        }
    }
}
