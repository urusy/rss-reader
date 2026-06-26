use sqlx::PgPool;

use super::domain::Stats;
use crate::shared::error::AppResult;

/// feeds 数 / articles 数 / 未読数を 1 クエリでまとめて取得する。
pub async fn fetch(pool: &PgPool) -> AppResult<Stats> {
    let (feeds, articles, unread) = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"SELECT
             (SELECT COUNT(*) FROM feeds),
             (SELECT COUNT(*) FROM articles),
             (SELECT COUNT(*) FROM articles WHERE is_read = false)"#,
    )
    .fetch_one(pool)
    .await?;
    Ok(Stats { feeds, articles, unread })
}
