use sqlx::PgPool;
use uuid::Uuid;

use super::domain::FeedHealthRow;
use crate::shared::error::AppResult;

/// Record a successful crawl on the feed row (reset failures, clear error).
pub async fn record_success(pool: &PgPool, feed_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE feeds
           SET last_fetch_status       = 'ok',
               last_error              = NULL,
               consecutive_failures    = 0,
               last_fetch_attempted_at = now()
           WHERE id = $1"#,
    )
    .bind(feed_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a failed crawl (increment failures, store the reason, truncated).
pub async fn record_failure(pool: &PgPool, feed_id: Uuid, error: &str) -> AppResult<()> {
    let truncated: String = error.chars().take(1000).collect();
    sqlx::query(
        r#"UPDATE feeds
           SET last_fetch_status       = 'error',
               last_error              = $2,
               consecutive_failures    = consecutive_failures + 1,
               last_fetch_attempted_at = now()
           WHERE id = $1"#,
    )
    .bind(feed_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}

/// Per-feed health rows (LEFT JOIN so zero-article feeds still appear).
pub async fn list_health(pool: &PgPool) -> AppResult<Vec<FeedHealthRow>> {
    let rows = sqlx::query_as::<_, FeedHealthRow>(
        r#"SELECT
             f.id                       AS feed_id,
             f.last_fetch_status        AS last_fetch_status,
             f.last_error               AS last_error,
             f.consecutive_failures     AS consecutive_failures,
             f.last_fetch_attempted_at  AS last_fetch_attempted_at,
             f.last_fetched_at          AS last_fetched_at,
             MAX(a.published_at)        AS last_published_at
           FROM feeds f
           LEFT JOIN articles a ON a.feed_id = f.id
           GROUP BY f.id
           ORDER BY f.created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
