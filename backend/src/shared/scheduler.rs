use std::time::Duration;

use tokio::time::{interval, MissedTickBehavior};

use chrono::{Timelike, Utc};

use super::state::AppState;
use crate::features::clustering;
use crate::features::digest;
use crate::features::feeds;
use crate::features::mute_rules;
use crate::features::notifications;

/// Spawn the periodic feed-refresh loop.
///
/// This is intentionally a minimal `tokio::interval` loop. When you outgrow it
/// (retries, backoff, per-feed scheduling, observability), swap this module for
/// an `apalis` worker without touching the feature code it calls.
pub fn spawn(state: AppState) {
    let period = Duration::from_secs(state.config.feed_refresh_interval_secs);
    tokio::spawn(async move {
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Skip the immediate first tick so startup isn't blocked by a full crawl.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            tracing::info!("scheduled feed refresh starting");
            if let Err(e) = feeds::service::refresh_all_feeds(&state).await {
                tracing::error!(error = %e, "feed refresh failed");
            }
            // #19: 新着にミュートを反映（hide リセット→再付与で冪等）。失敗してもクロールは継続。
            if let Err(e) = mute_rules::service::apply_all(&state).await {
                tracing::error!(error = %e, "mute apply failed");
            }
            // #31: 高優先フィードの新着を Web Push で通知（ミュート適用後に評価）。
            // VAPID 未設定なら no-op。独立タスクで走るため、死んだ push
            // エンドポイントがあっても次の取得サイクルを遅らせない。
            notifications::service::spawn_notify_new_articles(state.clone());
        }
    });
}

/// Daily digest loop (#23). Wakes hourly; when the UTC hour matches the configured
/// hour and digests are enabled, ensures today's digest exists (idempotent).
pub fn spawn_digest(state: AppState) {
    if !state.config.digest_enabled {
        tracing::info!("daily digest disabled (DIGEST_ENABLED is not true)");
        return;
    }
    let target_hour = state.config.digest_hour_utc;
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(3600));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if Utc::now().hour() == target_hour {
                tracing::info!("daily digest tick");
                if let Err(e) = digest::service::ensure_today(&state).await {
                    tracing::error!(error = %e, "daily digest generation failed");
                }
            }
        }
    });
}

/// Re-clustering loop (#26). Trigram-only (no LLM), cheap to run often. No-op
/// unless CLUSTERING_ENABLED=true.
pub fn spawn_clustering(state: AppState) {
    if !state.config.clustering_enabled {
        tracing::info!("clustering disabled (CLUSTERING_ENABLED is not true)");
        return;
    }
    let period = Duration::from_secs(state.config.clustering_interval_secs);
    tokio::spawn(async move {
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match clustering::service::recluster(&state).await {
                Ok(n) => tracing::info!(clusters = n, "re-clustering done"),
                Err(e) => tracing::error!(error = %e, "re-clustering failed"),
            }
        }
    });
}
