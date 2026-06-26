use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{ReadLaterItem, StoredCredentials};
use crate::features::articles::domain::ArticleId;
use crate::shared::error::AppResult;

/// 記事 URL/タイトル取得用の読み取り射影。
/// 本スライス内に閉じた read-only projection（articles の書き込み所有は移さない）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleRef {
    pub url: String,
    pub title: String,
}

pub async fn get_credentials(pool: &PgPool) -> AppResult<Option<StoredCredentials>> {
    let row = sqlx::query_as::<_, StoredCredentials>(
        "SELECT username, password FROM instapaper_credentials WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn upsert_credentials(pool: &PgPool, username: &str, password: &str) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO instapaper_credentials (id, username, password, updated_at)
           VALUES (1, $1, $2, now())
           ON CONFLICT (id) DO UPDATE
             SET username = EXCLUDED.username,
                 password = EXCLUDED.password,
                 updated_at = now()"#,
    )
    .bind(username)
    .bind(password)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_credentials(pool: &PgPool) -> AppResult<()> {
    sqlx::query("DELETE FROM instapaper_credentials WHERE id = 1")
        .execute(pool)
        .await?;
    Ok(())
}

/// article_id から URL/タイトルを引く（読み取り専用）。素の Uuid を bind するので
/// articles スライスの domain 型には依存しない（結合面を最小化）。
pub async fn get_article_ref(pool: &PgPool, article_id: Uuid) -> AppResult<Option<ArticleRef>> {
    let row = sqlx::query_as::<_, ArticleRef>("SELECT url, title FROM articles WHERE id = $1")
        .bind(article_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

// ---- 機能06「後で読む」: read_later_items ----

/// pending として 1 行を確保（既存行があっても pending に戻し last_error をクリア。PK で冪等）。
pub async fn upsert_pending(pool: &PgPool, id: ArticleId) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO read_later_items (article_id, status, updated_at)
           VALUES ($1, 'pending', now())
           ON CONFLICT (article_id) DO UPDATE
             SET status = 'pending', last_error = NULL, updated_at = now()"#,
    )
    .bind(id.0)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_added(pool: &PgPool, id: ArticleId) -> AppResult<ReadLaterItem> {
    let item = sqlx::query_as::<_, ReadLaterItem>(
        r#"UPDATE read_later_items
           SET status = 'added', instapaper_added_at = now(), last_error = NULL, updated_at = now()
           WHERE article_id = $1
           RETURNING *"#,
    )
    .bind(id.0)
    .fetch_one(pool)
    .await?;
    Ok(item)
}

pub async fn mark_failed(pool: &PgPool, id: ArticleId, err: &str) -> AppResult<ReadLaterItem> {
    let item = sqlx::query_as::<_, ReadLaterItem>(
        r#"UPDATE read_later_items
           SET status = 'failed', last_error = $2, updated_at = now()
           WHERE article_id = $1
           RETURNING *"#,
    )
    .bind(id.0)
    .bind(err)
    .fetch_one(pool)
    .await?;
    Ok(item)
}

pub async fn get_item(pool: &PgPool, id: ArticleId) -> AppResult<Option<ReadLaterItem>> {
    let row =
        sqlx::query_as::<_, ReadLaterItem>("SELECT * FROM read_later_items WHERE article_id = $1")
            .bind(id.0)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

pub async fn list_items(pool: &PgPool) -> AppResult<Vec<ReadLaterItem>> {
    let rows = sqlx::query_as::<_, ReadLaterItem>(
        "SELECT * FROM read_later_items ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL for repo tests");
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap()
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres (DATABASE_URL)"]
    async fn credentials_roundtrip_upsert_get_delete() {
        let pool = pool().await;
        delete_credentials(&pool).await.unwrap();
        assert!(get_credentials(&pool).await.unwrap().is_none());

        upsert_credentials(&pool, "user@example.com", "pw1")
            .await
            .unwrap();
        let got = get_credentials(&pool).await.unwrap().expect("row present");
        assert_eq!(got.username, "user@example.com");
        assert_eq!(got.password, "pw1");

        // 2回目は単一行を更新（singleton）
        upsert_credentials(&pool, "user2@example.com", "pw2")
            .await
            .unwrap();
        let got = get_credentials(&pool).await.unwrap().expect("row present");
        assert_eq!(got.username, "user2@example.com");
        assert_eq!(got.password, "pw2");

        delete_credentials(&pool).await.unwrap();
        assert!(get_credentials(&pool).await.unwrap().is_none());
    }
}
