use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{QuerySpec, SavedView, SavedViewId, SavedViewRow};
use crate::features::articles::domain::Article;
use crate::features::search::domain::SearchQuery;
use crate::shared::error::{AppError, AppResult};

const RESOLVE_LIMIT: i64 = 200;

pub async fn list(pool: &PgPool) -> AppResult<Vec<SavedView>> {
    let rows = sqlx::query_as::<_, SavedViewRow>(
        r#"SELECT id, name, query, position, created_at
           FROM saved_views
           ORDER BY position ASC, created_at ASC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(SavedView::from).collect())
}

pub async fn get(pool: &PgPool, id: SavedViewId) -> AppResult<SavedView> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        r#"SELECT id, name, query, position, created_at FROM saved_views WHERE id = $1"#,
    )
    .bind(id.0)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(row.into())
}

pub async fn insert(
    pool: &PgPool,
    name: &str,
    query: &QuerySpec,
    position: i32,
) -> AppResult<SavedView> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        r#"INSERT INTO saved_views (id, name, query, position)
           VALUES ($1, $2, $3, $4)
           RETURNING id, name, query, position, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(sqlx::types::Json(query))
    .bind(position)
    .fetch_one(pool)
    .await
    .map_err(translate_unique_name)?;
    Ok(row.into())
}

pub async fn update(
    pool: &PgPool,
    id: SavedViewId,
    name: &str,
    query: &QuerySpec,
    position: i32,
) -> AppResult<SavedView> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        r#"UPDATE saved_views
           SET name = $2, query = $3, position = $4
           WHERE id = $1
           RETURNING id, name, query, position, created_at"#,
    )
    .bind(id.0)
    .bind(name)
    .bind(sqlx::types::Json(query))
    .bind(position)
    .fetch_optional(pool)
    .await
    .map_err(translate_unique_name)?
    .ok_or(AppError::NotFound)?;
    Ok(row.into())
}

pub async fn delete(pool: &PgPool, id: SavedViewId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM saved_views WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Translate a lower(name) unique violation into a 400.
fn translate_unique_name(e: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &e {
        if db.constraint() == Some("idx_saved_views_name_lower") {
            return AppError::Validation("a view with this name already exists".into());
        }
    }
    AppError::Database(e)
}

/// Resolve a saved spec into articles (read-only). Reuses the articles list AND
/// predicates + the search ILIKE escaping. unread_override forces unread when set.
pub async fn resolve(
    pool: &PgPool,
    spec: &QuerySpec,
    unread_override: Option<bool>,
) -> AppResult<Vec<Article>> {
    let like = match spec.text.as_deref() {
        Some(t) if !t.trim().is_empty() => Some(SearchQuery::parse(t)?.like_pattern()),
        _ => None,
    };
    let unread = unread_override.unwrap_or(spec.unread_only);
    let tags: Option<Vec<Uuid>> = if spec.tag_ids.is_empty() {
        None
    } else {
        Some(spec.tag_ids.clone())
    };

    let rows = sqlx::query_as::<_, Article>(
        r#"SELECT * FROM articles
           WHERE ($1::text IS NULL
                  OR title ILIKE $1 ESCAPE '\'
                  OR content ILIKE $1 ESCAPE '\')
             AND ($2::uuid IS NULL OR feed_id = $2)
             AND ($3::uuid IS NULL
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id = $3))
             AND ($4 = false
                  OR feed_id IN (SELECT id FROM feeds WHERE folder_id IS NULL))
             AND ($5 = false OR is_read = false)
             AND ($6::uuid[] IS NULL
                  OR id IN (SELECT article_id FROM article_tags WHERE tag_id = ANY($6)))
           ORDER BY published_at DESC NULLS LAST, created_at DESC
           LIMIT $7"#,
    )
    .bind(like)
    .bind(spec.feed_id)
    .bind(spec.folder_id)
    .bind(spec.unclassified)
    .bind(unread)
    .bind(tags)
    .bind(RESOLVE_LIMIT)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
