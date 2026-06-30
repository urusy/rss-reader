//! Feed health classification (pure) + read-model rows. No LLM, no network.

use serde::Serialize;
use uuid::Uuid;

/// Consecutive failures at/above this → dead.
pub const DEAD_FAILURE_THRESHOLD: i32 = 3;
/// Last post older than this many days → stale.
pub const STALE_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    Healthy,
    Stale,
    Dead,
}

/// Raw health row from the repo (pre-classification). Correlation key is a bare
/// Uuid (no cross-slice type coupling to feeds' FeedId).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FeedHealthRow {
    pub feed_id: Uuid,
    pub last_fetch_status: Option<String>,
    pub last_error: Option<String>,
    pub consecutive_failures: i32,
    pub last_fetch_attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedHealth {
    pub feed_id: Uuid,
    pub last_fetch_status: Option<String>,
    pub last_error: Option<String>,
    pub consecutive_failures: i32,
    pub last_fetch_attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub health: HealthState,
}

/// Classify health (pure; `now` injected for deterministic tests). Order:
/// dead (failures >= threshold) > stale (never published / post older than
/// STALE_DAYS) > healthy.
pub fn classify(
    consecutive_failures: i32,
    last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> HealthState {
    if consecutive_failures >= DEAD_FAILURE_THRESHOLD {
        return HealthState::Dead;
    }
    match last_published_at {
        None => HealthState::Stale,
        Some(ts) => {
            if now - ts > chrono::Duration::days(STALE_DAYS) {
                HealthState::Stale
            } else {
                HealthState::Healthy
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn ago(days: i64) -> chrono::DateTime<chrono::Utc> {
        Utc::now() - Duration::days(days)
    }

    #[test]
    fn healthy_when_recent_post_and_no_failures() {
        assert_eq!(classify(0, Some(ago(3)), Utc::now()), HealthState::Healthy);
    }

    #[test]
    fn stale_when_last_post_older_than_threshold() {
        assert_eq!(classify(0, Some(ago(40)), Utc::now()), HealthState::Stale);
    }

    #[test]
    fn stale_when_never_published() {
        assert_eq!(classify(0, None, Utc::now()), HealthState::Stale);
    }

    #[test]
    fn dead_when_failures_at_threshold() {
        assert_eq!(classify(3, Some(ago(1)), Utc::now()), HealthState::Dead);
    }

    #[test]
    fn dead_when_failures_above_threshold() {
        assert_eq!(classify(10, None, Utc::now()), HealthState::Dead);
    }

    #[test]
    fn not_dead_below_threshold() {
        assert_eq!(classify(2, Some(ago(1)), Utc::now()), HealthState::Healthy);
    }

    #[test]
    fn dead_takes_precedence_over_stale() {
        assert_eq!(classify(5, Some(ago(1)), Utc::now()), HealthState::Dead);
    }

    #[test]
    fn just_under_stale_boundary_is_healthy() {
        assert_eq!(classify(0, Some(ago(29)), Utc::now()), HealthState::Healthy);
    }
}
