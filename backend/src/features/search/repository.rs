use sqlx::PgPool;

use super::domain::SearchQuery;
use crate::features::articles::domain::Article;
use crate::shared::error::AppResult;

/// Search articles by substring match on title/content (trigram-indexed ILIKE).
///
/// Ranking: title matches first, then by `similarity(title, query)` so the most
/// relevant titles lead, then by recency. Content-only matches fall to the end.
pub async fn search(pool: &PgPool, query: &SearchQuery, limit: i64) -> AppResult<Vec<Article>> {
    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT * FROM articles
           WHERE title ILIKE $1 ESCAPE '\'
              OR content ILIKE $1 ESCAPE '\'
           ORDER BY (title ILIKE $1 ESCAPE '\') DESC,
                    similarity(title, $2) DESC,
                    published_at DESC NULLS LAST,
                    created_at DESC
           LIMIT $3"#,
    )
    .bind(query.like_pattern())
    .bind(query.as_str())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
