use sqlx::PgPool;

use super::domain::FeedOverviewRow;
use crate::shared::error::AppResult;

/// feeds を起点に articles を LEFT JOIN し、フィード別の集計を1クエリで返す。
/// LEFT JOIN なので記事ゼロのフィードも1行返り、COUNT(a.id)=0 / MAX=NULL になる。
pub async fn fetch_overview(pool: &PgPool) -> AppResult<Vec<FeedOverviewRow>> {
    let rows = sqlx::query_as::<_, FeedOverviewRow>(
        r#"SELECT
             f.id AS feed_id,
             COUNT(a.id)                                                   AS total_count,
             COUNT(a.id) FILTER (WHERE a.is_read = false)                  AS unread_count,
             MAX(a.published_at)                                           AS last_published_at,
             COUNT(a.id) FILTER (
               WHERE a.published_at >= now() - interval '30 days'
             )                                                             AS recent_count_30d
           FROM feeds f
           LEFT JOIN articles a ON a.feed_id = f.id
           GROUP BY f.id
           ORDER BY f.created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
