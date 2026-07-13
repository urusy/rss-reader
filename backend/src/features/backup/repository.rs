//! Backup repository: export reads (FK-dependency order) and idempotent import
//! upserts (in a caller-owned transaction). Runtime queries only (no query!).

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::domain::{ArticleRow, BackupRunRow, FeedRow, FolderRow, ReadLaterRow, SavedPageRow};
use crate::shared::error::AppResult;

// ---- export (read, FK-dependency order) ----

pub async fn all_folders(pool: &PgPool) -> AppResult<Vec<FolderRow>> {
    let rows = sqlx::query_as::<_, FolderRow>(
        "SELECT id, name, position, created_at FROM folders ORDER BY position, created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_feeds(pool: &PgPool) -> AppResult<Vec<FeedRow>> {
    let rows = sqlx::query_as::<_, FeedRow>(
        "SELECT id, url, title, folder_id, created_at, last_fetched_at, kind \
         FROM feeds ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_saved_pages(pool: &PgPool) -> AppResult<Vec<SavedPageRow>> {
    let rows = sqlx::query_as::<_, SavedPageRow>(
        "SELECT article_id, saved_at, archived_at FROM saved_pages ORDER BY saved_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_articles(pool: &PgPool) -> AppResult<Vec<ArticleRow>> {
    let rows = sqlx::query_as::<_, ArticleRow>(
        "SELECT id, feed_id, url, title, content, published_at, is_read, summary, \
                summary_lang, translation, translation_lang, processed_at, created_at \
         FROM articles ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn all_read_later(pool: &PgPool) -> AppResult<Vec<ReadLaterRow>> {
    let rows = sqlx::query_as::<_, ReadLaterRow>(
        "SELECT article_id, status, instapaper_added_at, last_error, created_at, updated_at \
         FROM read_later_items ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- import (idempotent upserts inside a transaction) ----

pub async fn upsert_folder(tx: &mut Transaction<'_, Postgres>, r: &FolderRow) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO folders (id, name, position, created_at)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (id) DO UPDATE
             SET name = EXCLUDED.name, position = EXCLUDED.position"#,
    )
    .bind(r.id)
    .bind(&r.name)
    .bind(r.position)
    .bind(r.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// feeds.url is UNIQUE; on conflict adopt the existing row and return its real id
/// (used to remap article.feed_id).
pub async fn upsert_feed(tx: &mut Transaction<'_, Postgres>, r: &FeedRow) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO feeds (id, url, title, folder_id, created_at, last_fetched_at, kind)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (url) DO UPDATE
             SET title = EXCLUDED.title,
                 folder_id = COALESCE(EXCLUDED.folder_id, feeds.folder_id),
                 last_fetched_at = GREATEST(feeds.last_fetched_at, EXCLUDED.last_fetched_at)
           RETURNING id"#,
    )
    .bind(r.id)
    .bind(&r.url)
    .bind(&r.title)
    .bind(r.folder_id)
    .bind(r.created_at)
    .bind(r.last_fetched_at)
    .bind(&r.kind)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// saved_pages の upsert。article_id は呼び出し側で再マップ済みの実 id。
/// archived_at は「消さない」方向でマージ（is_read の OR と同じ token 防御思想）。
pub async fn upsert_saved_page(
    tx: &mut Transaction<'_, Postgres>,
    r: &SavedPageRow,
    mapped_article_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO saved_pages (article_id, saved_at, archived_at)
           VALUES ($1, $2, $3)
           ON CONFLICT (article_id) DO UPDATE
             SET archived_at = COALESCE(saved_pages.archived_at, EXCLUDED.archived_at)"#,
    )
    .bind(mapped_article_id)
    .bind(r.saved_at)
    .bind(r.archived_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// articles.url is UNIQUE. feed_id is the caller-remapped value. LLM cache and
/// is_read merge to "never lose info" (COALESCE / OR) — token defense.
pub async fn upsert_article(
    tx: &mut Transaction<'_, Postgres>,
    r: &ArticleRow,
    mapped_feed_id: Uuid,
) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO articles
             (id, feed_id, url, title, content, published_at, is_read,
              summary, summary_lang, translation, translation_lang, processed_at, created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
           ON CONFLICT (url) DO UPDATE
             SET title = EXCLUDED.title,
                 content = EXCLUDED.content,
                 published_at = COALESCE(EXCLUDED.published_at, articles.published_at),
                 is_read = articles.is_read OR EXCLUDED.is_read,
                 summary = COALESCE(EXCLUDED.summary, articles.summary),
                 summary_lang = COALESCE(EXCLUDED.summary_lang, articles.summary_lang),
                 translation = COALESCE(EXCLUDED.translation, articles.translation),
                 translation_lang = COALESCE(EXCLUDED.translation_lang, articles.translation_lang),
                 processed_at = COALESCE(EXCLUDED.processed_at, articles.processed_at)
           RETURNING id"#,
    )
    .bind(r.id)
    .bind(mapped_feed_id)
    .bind(&r.url)
    .bind(&r.title)
    .bind(&r.content)
    .bind(r.published_at)
    .bind(r.is_read)
    .bind(&r.summary)
    .bind(&r.summary_lang)
    .bind(&r.translation)
    .bind(&r.translation_lang)
    .bind(r.processed_at)
    .bind(r.created_at)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

pub async fn upsert_read_later(
    tx: &mut Transaction<'_, Postgres>,
    r: &ReadLaterRow,
    mapped_article_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO read_later_items
             (article_id, status, instapaper_added_at, last_error, created_at, updated_at)
           VALUES ($1,$2,$3,$4,$5,$6)
           ON CONFLICT (article_id) DO UPDATE
             SET status = EXCLUDED.status,
                 instapaper_added_at = COALESCE(EXCLUDED.instapaper_added_at, read_later_items.instapaper_added_at),
                 last_error = EXCLUDED.last_error,
                 updated_at = now()"#,
    )
    .bind(mapped_article_id)
    .bind(&r.status)
    .bind(r.instapaper_added_at)
    .bind(&r.last_error)
    .bind(r.created_at)
    .bind(r.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---- pg_dump scheduler (optional) ----

pub async fn insert_run_started(pool: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query("INSERT INTO backup_runs (id, status) VALUES ($1, 'running')")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish_run_ok(pool: &PgPool, id: Uuid, path: &str, bytes: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE backup_runs SET status='succeeded', finished_at=now(), file_path=$2, byte_size=$3 WHERE id=$1",
    )
    .bind(id)
    .bind(path)
    .bind(bytes)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_run_err(pool: &PgPool, id: Uuid, err: &str) -> AppResult<()> {
    sqlx::query("UPDATE backup_runs SET status='failed', finished_at=now(), error=$2 WHERE id=$1")
        .bind(id)
        .bind(err)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn recent_runs(pool: &PgPool, limit: i64) -> AppResult<Vec<BackupRunRow>> {
    let rows = sqlx::query_as::<_, BackupRunRow>(
        "SELECT id, started_at, finished_at, status, file_path, byte_size, error \
         FROM backup_runs ORDER BY started_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    //! Round-trip tests against a real DB. Run with:
    //!   DATABASE_URL=... cargo test -- --ignored
    use super::*;
    use crate::features::feeds::domain::FeedUrl;
    use crate::features::feeds::repository as feeds_repo;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        PgPool::connect(&url).await.expect("connect")
    }

    fn feed_row(url: &str) -> FeedRow {
        FeedRow {
            id: Uuid::new_v4(),
            url: url.to_string(),
            title: Some("t".into()),
            folder_id: None,
            created_at: chrono::Utc::now(),
            last_fetched_at: None,
            kind: "rss".into(),
        }
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn upsert_feed_is_idempotent_on_url() {
        let pool = pool().await;
        let mut tx = pool.begin().await.unwrap();
        let r = feed_row(&format!("https://example.com/bkp/{}", Uuid::new_v4()));
        let id1 = upsert_feed(&mut tx, &r).await.unwrap();
        let id2 = upsert_feed(&mut tx, &r).await.unwrap();
        assert_eq!(id1, id2);
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn upsert_article_preserves_cache_and_read_on_conflict() {
        let pool = pool().await;
        // Seed a feed + article with a cached summary and is_read=true.
        let furl = format!("https://example.com/bkp/{}", Uuid::new_v4());
        let fid = feeds_repo::insert(&pool, FeedUrl::parse(&furl).unwrap().as_str())
            .await
            .unwrap()
            .id
            .0;
        let aurl = format!("https://example.com/bkp/a/{}", Uuid::new_v4());

        let mut tx = pool.begin().await.unwrap();
        let mut a = ArticleRow {
            id: Uuid::new_v4(),
            feed_id: fid,
            url: aurl.clone(),
            title: "t".into(),
            content: "c".into(),
            published_at: None,
            is_read: true,
            summary: Some("cached".into()),
            summary_lang: Some("ja".into()),
            translation: None,
            translation_lang: None,
            processed_at: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
        };
        upsert_article(&mut tx, &a, fid).await.unwrap();
        // Re-import same url with null cache and is_read=false → must not erase.
        a.summary = None;
        a.summary_lang = None;
        a.is_read = false;
        upsert_article(&mut tx, &a, fid).await.unwrap();

        let (summary, is_read): (Option<String>, bool) =
            sqlx::query_as("SELECT summary, is_read FROM articles WHERE url = $1")
                .bind(&aurl)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(summary.as_deref(), Some("cached"));
        assert!(is_read);
        tx.rollback().await.unwrap();
    }
}
