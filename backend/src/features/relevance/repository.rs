use std::collections::HashSet;

use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{RawScore, RelevanceScore};
use crate::shared::error::AppResult;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScoreCandidate {
    pub id: Uuid,
    pub title: String,
    pub snippet: String,
}

// ---- profile materials (read-only cross-table) ----

/// Top tags by attached-article count (interest material). Reads tags/article_tags.
pub async fn top_tags(pool: &PgPool, limit: i64) -> AppResult<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"SELECT t.name, COUNT(at.article_id) AS cnt
           FROM tags t
           JOIN article_tags at ON at.tag_id = t.id
           GROUP BY t.id
           ORDER BY cnt DESC, t.name ASC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn recent_read_titles(pool: &PgPool, limit: i64) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"SELECT title FROM articles
           WHERE is_read = true
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(t,)| t).collect())
}

// ---- scoring candidates (unread) ----

pub async fn unread_candidates(pool: &PgPool, limit: i64) -> AppResult<Vec<ScoreCandidate>> {
    let rows = sqlx::query_as::<_, ScoreCandidate>(
        r#"SELECT id,
                  title,
                  COALESCE(NULLIF(summary, ''), LEFT(content, 500)) AS snippet
           FROM articles
           WHERE is_read = false
             -- 保存ページ（合成フィード）はスコアリング対象外（LLM トークン浪費防止）
             AND feed_id NOT IN (SELECT id FROM feeds WHERE kind <> 'rss')
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---- score cache ----

pub async fn list_scores(pool: &PgPool) -> AppResult<Vec<RelevanceScore>> {
    let rows = sqlx::query_as::<_, RelevanceScore>(
        r#"SELECT article_id, score, reasoning, scored_at
           FROM article_relevance_scores
           ORDER BY score DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn fresh_scored_ids(pool: &PgPool, profile_hash: &str) -> AppResult<HashSet<Uuid>> {
    let rows: Vec<(Uuid,)> =
        sqlx::query_as("SELECT article_id FROM article_relevance_scores WHERE profile_hash = $1")
            .bind(profile_hash)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

pub async fn save_scores(
    pool: &PgPool,
    scores: &[RawScore],
    profile_hash: &str,
    model: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for s in scores {
        sqlx::query(
            r#"INSERT INTO article_relevance_scores
                 (article_id, score, reasoning, profile_hash, model, scored_at)
               VALUES ($1, $2, $3, $4, $5, now())
               ON CONFLICT (article_id) DO UPDATE
                 SET score = EXCLUDED.score,
                     reasoning = EXCLUDED.reasoning,
                     profile_hash = EXCLUDED.profile_hash,
                     model = EXCLUDED.model,
                     scored_at = now()"#,
        )
        .bind(s.article_id)
        .bind(s.score)
        .bind(&s.reasoning)
        .bind(profile_hash)
        .bind(model)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
