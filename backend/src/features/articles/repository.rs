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
    include_muted: bool,
) -> AppResult<Vec<Article>> {
    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT * FROM articles
           WHERE ($1::uuid IS NULL OR feed_id = $1)
             AND ($2 = false OR is_read = false)
             AND ($3::uuid IS NULL
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id = $3))
             AND ($4 = false
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))
             AND ($5 = true OR muted_at IS NULL)
           ORDER BY published_at DESC NULLS LAST, created_at DESC
           LIMIT 200"#,
    )
    .bind(feed_id.map(|f| f.0))
    .bind(unread_only)
    .bind(folder_id.map(|f| f.0))
    .bind(unclassified)
    .bind(include_muted)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// is_read=false の記事を一括で既読にする。
/// feed_id=None なら全フィード、Some(id) ならそのフィードのみ。
/// 既に既読の行は対象外なので、戻り値（rows_affected）= 今回新たに既読化した件数。
pub async fn mark_all_read(pool: &PgPool, feed_id: Option<FeedId>) -> AppResult<u64> {
    let res = sqlx::query(
        r#"UPDATE articles
           SET is_read = true
           WHERE is_read = false
             AND ($1::uuid IS NULL OR feed_id = $1)"#,
    )
    .bind(feed_id.map(|f| f.0))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
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

/// Clear a cached summary (set it and its language back to NULL) so the user
/// can discard a stale/garbled result. NotFound if the article doesn't exist.
pub async fn clear_summary(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    let res = sqlx::query("UPDATE articles SET summary = NULL, summary_lang = NULL WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Clear a cached translation (mirrors `clear_summary`).
pub async fn clear_translation(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    let res = sqlx::query(
        "UPDATE articles SET translation = NULL, translation_lang = NULL WHERE id = $1",
    )
    .bind(id.0)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Cache the extracted full body. Only called on a successful extraction; on
/// failure the caller leaves full_content NULL so AI/display fall back to
/// `content`. Called from the `extraction` slice (same-aggregate write, mirrors
/// how `feeds` writes articles via `upsert`).
pub async fn save_full_content(pool: &PgPool, id: ArticleId, full_content: &str) -> AppResult<()> {
    let res = sqlx::query(
        r#"UPDATE articles
           SET full_content = $2, extracted_at = now()
           WHERE id = $1"#,
    )
    .bind(id.0)
    .bind(full_content)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Set an article's author by url, only when not already set (crawl populates it
/// for the rules engine #28; additive, same articles aggregate).
pub async fn set_author(pool: &PgPool, url: &str, author: &str) -> AppResult<()> {
    sqlx::query("UPDATE articles SET author = $2 WHERE url = $1 AND author IS NULL")
        .bind(url)
        .bind(author)
        .execute(pool)
        .await?;
    Ok(())
}

/// Look up an article id by its (unique) url. Used by crawl-time auto-extraction
/// since `upsert` does not return the id.
pub async fn id_by_url(pool: &PgPool, url: &str) -> AppResult<Option<ArticleId>> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM articles WHERE url = $1")
        .bind(url)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id,)| ArticleId(id)))
}

#[cfg(test)]
mod tests {
    //! Round-trip tests against a real DB. Run with:
    //!   DATABASE_URL=... cargo test -- --ignored
    //! Requires migrations applied (`just dev-db` / `just migrate`).
    use super::*;
    use crate::features::feeds::domain::FeedUrl;
    use crate::features::feeds::repository as feeds_repo;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        PgPool::connect(&url).await.expect("connect")
    }

    async fn a_feed(pool: &PgPool) -> FeedId {
        // Unique url per run to avoid clashing with other tests / existing rows.
        let raw = format!("https://example.com/extraction-test/{}", Uuid::new_v4());
        let url = FeedUrl::parse(&raw).expect("feed url");
        let feed = feeds_repo::insert(pool, url.as_str())
            .await
            .expect("insert feed");
        FeedId(feed.id.0)
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn save_full_content_sets_full_content_and_extracted_at() {
        let pool = pool().await;
        let feed_id = a_feed(&pool).await;
        let url = format!("https://example.com/post/{}", Uuid::new_v4());
        upsert(&pool, feed_id, &url, "t", "feed excerpt", None)
            .await
            .unwrap();
        let id = id_by_url(&pool, &url).await.unwrap().expect("id");

        let before = get(&pool, id).await.unwrap();
        assert!(before.full_content.is_none());
        assert!(before.extracted_at.is_none());

        save_full_content(&pool, id, "<p>extracted body</p>")
            .await
            .unwrap();

        let after = get(&pool, id).await.unwrap();
        assert_eq!(after.full_content.as_deref(), Some("<p>extracted body</p>"));
        assert!(after.extracted_at.is_some());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn save_full_content_missing_id_is_not_found() {
        let pool = pool().await;
        let res = save_full_content(&pool, ArticleId(Uuid::new_v4()), "x").await;
        assert!(matches!(res, Err(AppError::NotFound)));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn id_by_url_roundtrip() {
        let pool = pool().await;
        let feed_id = a_feed(&pool).await;
        let url = format!("https://example.com/post/{}", Uuid::new_v4());
        upsert(&pool, feed_id, &url, "t", "c", None).await.unwrap();

        assert!(id_by_url(&pool, &url).await.unwrap().is_some());
        let missing = format!("https://example.com/nope/{}", Uuid::new_v4());
        assert!(id_by_url(&pool, &missing).await.unwrap().is_none());
    }
}
