use chrono::NaiveDate;
use sqlx::PgPool;

use super::domain::{Digest, DigestSource};
use crate::shared::error::AppResult;

pub async fn get_latest(pool: &PgPool) -> AppResult<Option<Digest>> {
    let row = sqlx::query_as::<_, Digest>(
        "SELECT date, markdown, model, article_count, created_at
         FROM digests ORDER BY date DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_by_date(pool: &PgPool, date: NaiveDate) -> AppResult<Option<Digest>> {
    let row = sqlx::query_as::<_, Digest>(
        "SELECT date, markdown, model, article_count, created_at
         FROM digests WHERE date = $1",
    )
    .bind(date)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn upsert(
    pool: &PgPool,
    date: NaiveDate,
    markdown: &str,
    model: &str,
    article_count: i32,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO digests (date, markdown, model, article_count, created_at)
           VALUES ($1, $2, $3, $4, now())
           ON CONFLICT (date) DO UPDATE
             SET markdown = EXCLUDED.markdown,
                 model = EXCLUDED.model,
                 article_count = EXCLUDED.article_count,
                 created_at = now()"#,
    )
    .bind(date)
    .bind(markdown)
    .bind(model)
    .bind(article_count)
    .execute(pool)
    .await?;
    Ok(())
}

/// Read recent unread articles as digest material (read-only cross-table). snippet
/// = summary, else first 800 chars of content. Newest first, capped at 100.
pub async fn recent_unread(pool: &PgPool, hours: i32) -> AppResult<Vec<DigestSource>> {
    let rows = sqlx::query_as::<_, DigestSource>(
        r#"SELECT title,
                  url,
                  COALESCE(NULLIF(summary, ''), LEFT(content, 800)) AS snippet
           FROM articles
           WHERE is_read = false
             AND COALESCE(published_at, created_at) >= now() - make_interval(hours => $1)
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT 100"#,
    )
    .bind(hours)
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
    #[ignore = "requires DATABASE_URL"]
    async fn digest_upsert_get_latest_roundtrip() {
        let pool = pool().await;
        let d1 = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2000, 1, 2).unwrap();

        upsert(&pool, d1, "## a", "m1", 3).await.unwrap();
        let got = get_by_date(&pool, d1).await.unwrap().expect("row");
        assert_eq!(got.markdown, "## a");
        assert_eq!(got.article_count, 3);

        upsert(&pool, d1, "## a2", "m2", 5).await.unwrap();
        let got = get_by_date(&pool, d1).await.unwrap().expect("row");
        assert_eq!(got.markdown, "## a2");
        assert_eq!(got.article_count, 5);

        upsert(&pool, d2, "## b", "m3", 1).await.unwrap();
        let latest = get_latest(&pool).await.unwrap().expect("row");
        assert_eq!(latest.date, d2);

        sqlx::query("DELETE FROM digests WHERE date IN ($1, $2)")
            .bind(d1)
            .bind(d2)
            .execute(&pool)
            .await
            .unwrap();
    }
}
