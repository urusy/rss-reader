mod features;
mod shared;

use std::sync::Arc;

use anyhow::Context;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::shared::{config::AppConfig, db, scheduler, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present (no-op in production where env vars are injected).
    let _ = dotenvy::dotenv();

    init_tracing();

    let config = AppConfig::from_env().context("failed to load configuration")?;
    tracing::info!(bind = %config.bind_addr, "starting rss-reader backend");

    // Connection pool + migrations.
    let pool = db::create_pool(&config.database_url)
        .await
        .context("failed to create database pool")?;
    db::run_migrations(&pool)
        .await
        .context("failed to run migrations")?;

    let http = reqwest::Client::builder()
        .user_agent(concat!("rss-reader/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build http client")?;

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        http,
    };

    // Background feed-refresh loop. Swappable for apalis later (see CLAUDE.md).
    scheduler::spawn(state.clone());
    // Optional scheduled pg_dump (no-op unless BACKUP_DIR + interval are set).
    features::backup::service::spawn_pgdump_scheduler(state.clone());
    // Optional daily AI digest (no-op unless DIGEST_ENABLED=true).
    scheduler::spawn_digest(state.clone());
    // Optional periodic re-clustering (no-op unless CLUSTERING_ENABLED=true).
    scheduler::spawn_clustering(state.clone());

    let app = features::router(state.clone());

    let listener = tokio::net::TcpListener::bind(state.config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", state.config.bind_addr))?;
    tracing::info!("listening on http://{}", state.config.bind_addr);

    axum::serve(listener, app).await.context("server error")?;

    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,rss_reader_backend=debug,tower_http=debug".into());
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
