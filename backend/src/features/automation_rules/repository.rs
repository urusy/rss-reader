use sqlx::PgPool;
use uuid::Uuid;

use crate::shared::error::AppResult;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuleRow {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub position: i32,
    pub conditions: String,
    pub actions: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingArticle {
    pub id: Uuid,
    pub feed_id: Uuid,
    pub title: String,
    pub content: String,
    pub author: Option<String>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<RuleRow>> {
    let rows = sqlx::query_as::<_, RuleRow>(
        "SELECT * FROM automation_rules ORDER BY position ASC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_enabled(pool: &PgPool) -> AppResult<Vec<RuleRow>> {
    let rows = sqlx::query_as::<_, RuleRow>(
        "SELECT * FROM automation_rules WHERE enabled = true ORDER BY position ASC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, id: Uuid) -> AppResult<Option<RuleRow>> {
    let row = sqlx::query_as::<_, RuleRow>("SELECT * FROM automation_rules WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn insert(
    pool: &PgPool,
    name: &str,
    enabled: bool,
    position: i32,
    conditions_json: &str,
    actions_json: &str,
) -> AppResult<RuleRow> {
    let row = sqlx::query_as::<_, RuleRow>(
        r#"INSERT INTO automation_rules (name, enabled, position, conditions, actions)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING *"#,
    )
    .bind(name)
    .bind(enabled)
    .bind(position)
    .bind(conditions_json)
    .bind(actions_json)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    enabled: bool,
    position: i32,
    conditions_json: &str,
    actions_json: &str,
) -> AppResult<Option<RuleRow>> {
    let row = sqlx::query_as::<_, RuleRow>(
        r#"UPDATE automation_rules
           SET name = $2, enabled = $3, position = $4,
               conditions = $5, actions = $6, updated_at = now()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(name)
    .bind(enabled)
    .bind(position)
    .bind(conditions_json)
    .bind(actions_json)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM automation_rules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn fetch_pending(
    pool: &PgPool,
    feed_id: Uuid,
    limit: i64,
) -> AppResult<Vec<PendingArticle>> {
    let rows = sqlx::query_as::<_, PendingArticle>(
        r#"SELECT id, feed_id, title, content, author, published_at
           FROM articles
           WHERE feed_id = $1 AND rules_applied_at IS NULL
           ORDER BY created_at ASC
           LIMIT $2"#,
    )
    .bind(feed_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn fetch_all_articles(pool: &PgPool, limit: i64) -> AppResult<Vec<PendingArticle>> {
    let rows = sqlx::query_as::<_, PendingArticle>(
        r#"SELECT id, feed_id, title, content, author, published_at
           FROM articles
           -- 保存ページ（合成フィード）は手動バックフィルの対象外
           WHERE feed_id NOT IN (SELECT id FROM feeds WHERE kind <> 'rss')
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_applied(pool: &PgPool, ids: &[Uuid]) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE articles SET rules_applied_at = now() WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(())
}

/// Lowercased tag names for an article (empty if the tags tables aren't present).
pub async fn tags_for(pool: &PgPool, article_id: Uuid) -> AppResult<Vec<String>> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.article_tags')::text")
            .fetch_one(pool)
            .await?;
    if exists.is_none() {
        return Ok(Vec::new());
    }
    let tags: Vec<String> = sqlx::query_scalar(
        r#"SELECT lower(t.name)
           FROM article_tags at JOIN tags t ON t.id = at.tag_id
           WHERE at.article_id = $1"#,
    )
    .bind(article_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    Ok(tags)
}

pub async fn bump_score(pool: &PgPool, article_id: Uuid, delta: i32) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO article_scores (article_id, score, updated_at)
           VALUES ($1, $2, now())
           ON CONFLICT (article_id) DO UPDATE
             SET score = article_scores.score + EXCLUDED.score,
                 updated_at = now()"#,
    )
    .bind(article_id)
    .bind(delta)
    .execute(pool)
    .await?;
    Ok(())
}
