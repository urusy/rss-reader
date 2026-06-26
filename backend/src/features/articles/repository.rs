use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Article, ArticleId};
use crate::features::feeds::domain::FeedId;
use crate::features::folders::domain::FolderId;
use crate::shared::error::{AppError, AppResult};

#[allow(clippy::too_many_arguments)]
pub async fn upsert(
    pool: &PgPool,
    feed_id: FeedId,
    url: &str,
    title: &str,
    content: &str,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
) -> AppResult<()> {
    // De-dupe on url; keep the earliest insert, refresh title/content if changed.
    sqlx::query(
        r#"INSERT INTO articles (id, feed_id, url, title, content, published_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (url) DO UPDATE
             SET title = EXCLUDED.title,
                 content = EXCLUDED.content"#,
    )
    .bind(Uuid::new_v4())
    .bind(feed_id.0)
    .bind(url)
    .bind(title)
    .bind(content)
    .bind(published_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(
    pool: &PgPool,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,
    unclassified: bool,
) -> AppResult<Vec<Article>> {
    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT * FROM articles
           WHERE ($1::uuid IS NULL OR feed_id = $1)
             AND ($2 = false OR is_read = false)
             AND ($3::uuid IS NULL
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id = $3))
             AND ($4 = false
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))
           ORDER BY published_at DESC NULLS LAST, created_at DESC
           LIMIT 200"#,
    )
    .bind(feed_id.map(|f| f.0))
    .bind(unread_only)
    .bind(folder_id.map(|f| f.0))
    .bind(unclassified)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: ArticleId) -> AppResult<Article> {
    sqlx::query_as::<_, Article>("SELECT * FROM articles WHERE id = $1")
        .bind(id.0)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn set_read(pool: &PgPool, id: ArticleId, read: bool) -> AppResult<()> {
    let res = sqlx::query("UPDATE articles SET is_read = $2 WHERE id = $1")
        .bind(id.0)
        .bind(read)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn save_summary(
    pool: &PgPool,
    id: ArticleId,
    summary: &str,
    lang: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE articles
           SET summary = $2, summary_lang = $3, processed_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(summary)
    .bind(lang)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn save_translation(
    pool: &PgPool,
    id: ArticleId,
    translation: &str,
    lang: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE articles
           SET translation = $2, translation_lang = $3, processed_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(translation)
    .bind(lang)
    .execute(pool)
    .await?;
    Ok(())
}
