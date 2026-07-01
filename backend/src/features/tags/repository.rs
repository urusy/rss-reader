use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{ArticleTag, RawSuggestion, Tag, TagId, TagWithCount};
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleText {
    pub title: String,
    pub content: String,
}

// ---- tags CRUD ----

pub async fn list_tags(pool: &PgPool) -> AppResult<Vec<TagWithCount>> {
    let rows = sqlx::query_as::<_, TagWithCount>(
        r#"SELECT t.id, t.name, t.color, t.source, t.created_at,
                  COUNT(at.article_id) AS article_count
           FROM tags t
           LEFT JOIN article_tags at ON at.tag_id = t.id
           GROUP BY t.id
           ORDER BY t.name ASC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Upsert by case-insensitive name (used by user-create and AI approval).
pub async fn upsert_tag(
    pool: &PgPool,
    name: &str,
    color: Option<&str>,
    source: &str,
) -> AppResult<Tag> {
    let tag = sqlx::query_as::<_, Tag>(
        r#"INSERT INTO tags (id, name, color, source)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (lower(name)) DO UPDATE
             SET color = COALESCE(EXCLUDED.color, tags.color)
           RETURNING id, name, color, source, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(color)
    .bind(source)
    .fetch_one(pool)
    .await?;
    Ok(tag)
}

pub async fn update_tag(
    pool: &PgPool,
    id: TagId,
    name: &str,
    color: Option<&str>,
) -> AppResult<Tag> {
    sqlx::query_as::<_, Tag>(
        r#"UPDATE tags SET name = $2, color = $3
           WHERE id = $1
           RETURNING id, name, color, source, created_at"#,
    )
    .bind(id.0)
    .bind(name)
    .bind(color)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn delete_tag(pool: &PgPool, id: TagId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM tags WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

// ---- article <-> tag ----

pub async fn list_article_tags(pool: &PgPool, article_id: ArticleId) -> AppResult<Vec<ArticleTag>> {
    let rows = sqlx::query_as::<_, ArticleTag>(
        r#"SELECT t.id, t.name, t.color,
                  at.source AS attached_source, at.confidence
           FROM article_tags at
           JOIN tags t ON t.id = at.tag_id
           WHERE at.article_id = $1
           ORDER BY t.name ASC"#,
    )
    .bind(article_id.0)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn detach_tag(pool: &PgPool, article_id: ArticleId, tag_id: TagId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM article_tags WHERE article_id = $1 AND tag_id = $2")
        .bind(article_id.0)
        .bind(tag_id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Set the article's user tag-set in one transaction: drop user edges not in the
/// set, upsert the rest. AI edges are preserved.
pub async fn set_article_tags(
    pool: &PgPool,
    article_id: ArticleId,
    tag_ids: &[TagId],
) -> AppResult<()> {
    let ids: Vec<Uuid> = tag_ids.iter().map(|t| t.0).collect();
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM article_tags
         WHERE article_id = $1 AND source = 'user' AND NOT (tag_id = ANY($2))",
    )
    .bind(article_id.0)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;

    for id in &ids {
        sqlx::query(
            r#"INSERT INTO article_tags (article_id, tag_id, source, confidence)
               VALUES ($1, $2, 'user', NULL)
               ON CONFLICT (article_id, tag_id) DO UPDATE SET source = 'user'"#,
        )
        .bind(article_id.0)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

// ---- articles read-only (for suggestion) ----

pub async fn get_article_text(
    pool: &PgPool,
    article_id: ArticleId,
) -> AppResult<Option<ArticleText>> {
    let row = sqlx::query_as::<_, ArticleText>(
        "SELECT title, COALESCE(NULLIF(full_content, ''), content) AS content \
         FROM articles WHERE id = $1",
    )
    .bind(article_id.0)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Current vocabulary (existing tag names) for the prompt.
pub async fn vocabulary(pool: &PgPool) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM tags ORDER BY name ASC")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

// ---- AI suggestion cache ----

pub async fn get_cached_suggestions(
    pool: &PgPool,
    article_id: ArticleId,
) -> AppResult<Option<Vec<RawSuggestion>>> {
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT suggestions FROM article_tag_suggestions WHERE article_id = $1")
            .bind(article_id.0)
            .fetch_optional(pool)
            .await?;
    match row {
        Some((json,)) => {
            let v: Vec<RawSuggestion> =
                serde_json::from_value(json).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

pub async fn save_suggestions(
    pool: &PgPool,
    article_id: ArticleId,
    suggestions: &[RawSuggestion],
    model: &str,
) -> AppResult<()> {
    let json =
        serde_json::to_value(suggestions).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
    sqlx::query(
        r#"INSERT INTO article_tag_suggestions (article_id, suggestions, model, suggested_at)
           VALUES ($1, $2, $3, now())
           ON CONFLICT (article_id) DO UPDATE
             SET suggestions = EXCLUDED.suggestions,
                 model = EXCLUDED.model,
                 suggested_at = now()"#,
    )
    .bind(article_id.0)
    .bind(json)
    .bind(model)
    .execute(pool)
    .await?;
    Ok(())
}
