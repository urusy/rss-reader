use std::time::Duration;

use tokio::time::{interval, MissedTickBehavior};

use super::state::AppState;
use crate::features::feeds;
use crate::features::mute_rules;

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
        }
    });
}
