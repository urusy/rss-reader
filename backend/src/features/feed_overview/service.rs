use super::domain::{self, FeedOverview};
use super::repository;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// 集計行を読み、週あたり本数を導出して read model に詰め替える。
pub async fn list_overview(state: &AppState) -> AppResult<Vec<FeedOverview>> {
    let rows = repository::fetch_overview(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(|r| FeedOverview {
            feed_id: r.feed_id,
            total_count: r.total_count,
            unread_count: r.unread_count,
            last_published_at: r.last_published_at,
            posts_per_week: domain::posts_per_week(r.recent_count_30d),
        })
        .collect())
}
