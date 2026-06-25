//! Data access for the feeds slice. Plain sqlx functions — no trait abstraction,
//! because there is no realistic second implementation of "store a feed in our DB".

use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Feed, FeedId};
use crate::shared::error::AppResult;

pub async fn insert(pool: &PgPool, url: &str) -> AppResult<Feed> {
    let row = sqlx::query_as::<_, Feed>(
        r#"INSERT INTO feeds (id, url) VALUES ($1, $2)
           ON CONFLICT (url) DO UPDATE SET url = EXCLUDED.url
           RETURNING id, url, title, created_at, last_fetched_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(url)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<Feed>> {
    let rows = sqlx::query_as::<_, Feed>(
        r#"SELECT id, url, title, created_at, last_fetched_at
           FROM feeds ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn delete(pool: &PgPool, id: FeedId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM feeds WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn touch_fetched(pool: &PgPool, id: FeedId, title: Option<&str>) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE feeds
           SET last_fetched_at = now(),
               title = COALESCE($2, title)
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(title)
    .execute(pool)
    .await?;
    Ok(())
}
