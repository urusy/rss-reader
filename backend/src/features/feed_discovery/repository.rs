use std::collections::HashSet;

use sqlx::PgPool;

use crate::shared::error::AppResult;

/// Existing subscribed feed URLs (read-only, for already_subscribed). Same
/// CQRS-lite cross-read as feed_overview. Runtime query only.
pub async fn existing_feed_urls(pool: &PgPool) -> AppResult<HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT url FROM feeds")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}
