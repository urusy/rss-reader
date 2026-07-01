use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Highlight, HighlightPatch, NewHighlight};
use crate::shared::error::AppResult;

/// Does the article exist? Used to return 404 before writing annotations.
pub async fn article_exists(pool: &PgPool, article_id: Uuid) -> AppResult<bool> {
    let found: Option<Uuid> = sqlx::query_scalar("SELECT id FROM articles WHERE id = $1")
        .bind(article_id)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

// ---- stars -----------------------------------------------------------------

pub async fn add_star(pool: &PgPool, article_id: Uuid) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO article_stars (article_id) VALUES ($1) ON CONFLICT (article_id) DO NOTHING",
    )
    .bind(article_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_star(pool: &PgPool, article_id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM article_stars WHERE article_id = $1")
        .bind(article_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn starred_ids(pool: &PgPool) -> AppResult<Vec<Uuid>> {
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT article_id FROM article_stars ORDER BY created_at DESC")
            .fetch_all(pool)
            .await?;
    Ok(ids)
}

// ---- highlights ------------------------------------------------------------

pub async fn list_highlights(pool: &PgPool, article_id: Uuid) -> AppResult<Vec<Highlight>> {
    let rows = sqlx::query_as::<_, Highlight>(
        r#"SELECT id, article_id, quote, note, start_offset, end_offset, color,
                  created_at, updated_at
           FROM highlights WHERE article_id = $1 ORDER BY created_at ASC"#,
    )
    .bind(article_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn insert_highlight(
    pool: &PgPool,
    article_id: Uuid,
    h: &NewHighlight,
) -> AppResult<Highlight> {
    let row = sqlx::query_as::<_, Highlight>(
        r#"INSERT INTO highlights (article_id, quote, note, start_offset, end_offset, color)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, article_id, quote, note, start_offset, end_offset, color,
                     created_at, updated_at"#,
    )
    .bind(article_id)
    .bind(&h.quote)
    .bind(&h.note)
    .bind(h.start_offset)
    .bind(h.end_offset)
    .bind(&h.color)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Patch note/color. COALESCE keeps the existing value when the field is NULL,
/// so a patch that only sets `note` leaves `color` untouched and vice versa.
pub async fn update_highlight(
    pool: &PgPool,
    id: Uuid,
    p: &HighlightPatch,
) -> AppResult<Option<Highlight>> {
    let row = sqlx::query_as::<_, Highlight>(
        r#"UPDATE highlights
           SET note = COALESCE($2, note), color = COALESCE($3, color), updated_at = now()
           WHERE id = $1
           RETURNING id, article_id, quote, note, start_offset, end_offset, color,
                     created_at, updated_at"#,
    )
    .bind(id)
    .bind(&p.note)
    .bind(&p.color)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete_highlight(pool: &PgPool, id: Uuid) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM highlights WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
