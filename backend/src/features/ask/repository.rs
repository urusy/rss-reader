use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{ArticleContext, AskMessage};
use crate::shared::error::AppResult;

#[derive(Debug, Clone, sqlx::FromRow)]
struct ArticleContextRow {
    title: String,
    content: String,
}

/// Read an article body (read-only projection; bare Uuid, no articles domain
/// dependency). full_content is preferred when present (extraction feature 13).
pub async fn get_article_context(pool: &PgPool, id: Uuid) -> AppResult<Option<ArticleContext>> {
    let row = sqlx::query_as::<_, ArticleContextRow>(
        "SELECT title, COALESCE(NULLIF(full_content, ''), content) AS content \
         FROM articles WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ArticleContext {
        title: r.title,
        body: r.content,
    }))
}

/// Read several articles (cross-Ask), preserving the given order. Missing ids
/// are silently dropped (caller checks for empty → NotFound).
pub async fn get_article_contexts(pool: &PgPool, ids: &[Uuid]) -> AppResult<Vec<ArticleContext>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(c) = get_article_context(pool, *id).await? {
            out.push(c);
        }
    }
    Ok(out)
}

/// Append Q&A rows (save=true only).
pub async fn save_notes(pool: &PgPool, article_id: Uuid, rows: &[AskMessage]) -> AppResult<()> {
    for m in rows {
        sqlx::query(
            "INSERT INTO article_notes (id, article_id, role, content) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(article_id)
        .bind(&m.role)
        .bind(&m.content)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Saved Q&A in chronological order (GET /notes).
pub async fn list_notes(pool: &PgPool, article_id: Uuid) -> AppResult<Vec<AskMessage>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM article_notes WHERE article_id = $1 ORDER BY created_at ASC",
    )
    .bind(article_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(role, content)| AskMessage { role, content })
        .collect())
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
    async fn notes_save_and_list_in_order() {
        let pool = pool().await;
        let feed_id = Uuid::new_v4();
        let article_id = Uuid::new_v4();
        sqlx::query("INSERT INTO feeds (id, url) VALUES ($1, $2)")
            .bind(feed_id)
            .bind(format!("https://example.com/{feed_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO articles (id, feed_id, url, title, content) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(article_id)
        .bind(feed_id)
        .bind(format!("https://example.com/a/{article_id}"))
        .bind("t")
        .bind("body")
        .execute(&pool)
        .await
        .unwrap();

        let rows = vec![
            AskMessage {
                role: "user".into(),
                content: "q".into(),
            },
            AskMessage {
                role: "assistant".into(),
                content: "a".into(),
            },
        ];
        save_notes(&pool, article_id, &rows).await.unwrap();
        let got = list_notes(&pool, article_id).await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].role, "user");
        assert_eq!(got[1].role, "assistant");

        let ctx = get_article_context(&pool, article_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ctx.title, "t");
        assert_eq!(ctx.body, "body");

        sqlx::query("DELETE FROM articles WHERE id = $1")
            .bind(article_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(list_notes(&pool, article_id).await.unwrap().is_empty());
        sqlx::query("DELETE FROM feeds WHERE id = $1")
            .bind(feed_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
