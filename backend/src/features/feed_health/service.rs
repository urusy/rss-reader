use super::domain::{classify, FeedHealth};
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list_health(state: &AppState) -> AppResult<Vec<FeedHealth>> {
    let now = chrono::Utc::now();
    let rows = repository::list_health(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| FeedHealth {
            health: classify(r.consecutive_failures, r.last_published_at, now),
            feed_id: r.feed_id,
            last_fetch_status: r.last_fetch_status,
            last_error: r.last_error,
            consecutive_failures: r.consecutive_failures,
            last_fetch_attempted_at: r.last_fetch_attempted_at,
            last_fetched_at: r.last_fetched_at,
            last_published_at: r.last_published_at,
        })
        .collect())
}
