use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Cluster, ClusterMember, ClusterMemberSource};
use crate::shared::error::AppResult;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArticleNode {
    pub id: Uuid,
    pub title: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClusterEdge {
    pub left_id: Uuid,
    pub right_id: Uuid,
    pub sim: f32,
}

#[derive(Debug, Clone)]
pub struct NewCluster {
    pub title: String,
    pub signature: String,
    pub members: Vec<NewMember>,
    /// (summary, lang, model) carried over when signature matches.
    pub carried_summary: Option<(String, String, String)>,
}

#[derive(Debug, Clone)]
pub struct NewMember {
    pub article_id: Uuid,
    pub is_representative: bool,
    pub is_duplicate: bool,
    pub similarity: f32,
}

pub async fn recent_nodes(pool: &PgPool, hours: i32, cap: i32) -> AppResult<Vec<ArticleNode>> {
    let rows = sqlx::query_as::<_, ArticleNode>(
        r#"SELECT id, title, published_at
           FROM articles
           WHERE COALESCE(published_at, created_at) >= now() - make_interval(hours => $1)
             AND length(title) >= 3
           ORDER BY COALESCE(published_at, created_at) DESC
           LIMIT $2"#,
    )
    .bind(hours)
    .bind(cap)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn similarity_edges(
    pool: &PgPool,
    hours: i32,
    cap: i32,
    threshold: f32,
) -> AppResult<Vec<ClusterEdge>> {
    let rows = sqlx::query_as::<_, ClusterEdge>(
        r#"WITH recent AS (
               SELECT id, title
               FROM articles
               WHERE COALESCE(published_at, created_at) >= now() - make_interval(hours => $1)
                 AND length(title) >= 3
               ORDER BY COALESCE(published_at, created_at) DESC
               LIMIT $2
           )
           SELECT a.id AS left_id,
                  b.id AS right_id,
                  similarity(a.title, b.title) AS sim
           FROM recent a
           JOIN recent b ON a.id < b.id
           WHERE similarity(a.title, b.title) >= $3"#,
    )
    .bind(hours)
    .bind(cap)
    .bind(threshold)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[allow(clippy::type_complexity)]
pub async fn existing_summaries(
    pool: &PgPool,
) -> AppResult<Vec<(String, Option<String>, Option<String>, Option<String>)>> {
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT signature, summary, summary_lang, summary_model FROM article_clusters",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Rebuild the cluster tables in one transaction (delete all → insert).
pub async fn replace_clusters(pool: &PgPool, clusters: &[NewCluster]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM article_clusters")
        .execute(&mut *tx)
        .await?;

    for c in clusters {
        let cluster_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO article_clusters
                   (title, size, signature, summary, summary_lang, summary_model, summarized_at)
               VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $4 IS NULL THEN NULL ELSE now() END)
               RETURNING id"#,
        )
        .bind(&c.title)
        .bind(c.members.len() as i32)
        .bind(&c.signature)
        .bind(c.carried_summary.as_ref().map(|s| s.0.clone()))
        .bind(c.carried_summary.as_ref().map(|s| s.1.clone()))
        .bind(c.carried_summary.as_ref().map(|s| s.2.clone()))
        .fetch_one(&mut *tx)
        .await?;

        for m in &c.members {
            sqlx::query(
                r#"INSERT INTO cluster_members
                       (cluster_id, article_id, is_representative, is_duplicate, similarity)
                   VALUES ($1, $2, $3, $4, $5)"#,
            )
            .bind(cluster_id)
            .bind(m.article_id)
            .bind(m.is_representative)
            .bind(m.is_duplicate)
            .bind(m.similarity)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

pub async fn list_clusters(pool: &PgPool, min_size: i32) -> AppResult<Vec<Cluster>> {
    let rows = sqlx::query_as::<_, Cluster>(
        "SELECT id, title, size, summary, summary_lang, created_at
         FROM article_clusters
         WHERE size >= $1
         ORDER BY size DESC, created_at DESC",
    )
    .bind(min_size)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn members_for(pool: &PgPool, cluster_ids: &[Uuid]) -> AppResult<Vec<ClusterMember>> {
    let rows = sqlx::query_as::<_, ClusterMember>(
        r#"SELECT cm.cluster_id, cm.article_id, a.title, a.url, a.feed_id,
                  f.title AS feed_title, cm.is_representative, cm.is_duplicate, cm.similarity
           FROM cluster_members cm
           JOIN articles a ON a.id = cm.article_id
           JOIN feeds f ON f.id = a.feed_id
           WHERE cm.cluster_id = ANY($1)
           ORDER BY cm.is_representative DESC, cm.similarity DESC, a.title ASC"#,
    )
    .bind(cluster_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_cluster(pool: &PgPool, id: Uuid) -> AppResult<Option<Cluster>> {
    let row = sqlx::query_as::<_, Cluster>(
        "SELECT id, title, size, summary, summary_lang, created_at
         FROM article_clusters WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn member_sources(
    pool: &PgPool,
    cluster_id: Uuid,
) -> AppResult<Vec<ClusterMemberSource>> {
    let rows = sqlx::query_as::<_, ClusterMemberSource>(
        r#"SELECT f.title AS feed_title,
                  a.title AS title,
                  COALESCE(NULLIF(a.summary, ''), LEFT(a.content, 600)) AS snippet
           FROM cluster_members cm
           JOIN articles a ON a.id = cm.article_id
           JOIN feeds f ON f.id = a.feed_id
           WHERE cm.cluster_id = $1
           ORDER BY cm.is_representative DESC"#,
    )
    .bind(cluster_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn save_summary(
    pool: &PgPool,
    id: Uuid,
    summary: &str,
    lang: &str,
    model: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"UPDATE article_clusters
           SET summary = $2, summary_lang = $3, summary_model = $4, summarized_at = now()
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(summary)
    .bind(lang)
    .bind(model)
    .execute(pool)
    .await?;
    Ok(())
}
