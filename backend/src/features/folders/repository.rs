use sqlx::PgPool;
use uuid::Uuid;

use super::domain::{Folder, FolderId};
use crate::shared::error::{AppError, AppResult};

/// position は MAX+1 で採番（並行挿入の厳密性は単一ユーザ前提で不問）。
pub async fn insert(pool: &PgPool, name: &str) -> AppResult<Folder> {
    let row = sqlx::query_as::<_, Folder>(
        r#"INSERT INTO folders (id, name, position)
           VALUES ($1, $2, (SELECT COALESCE(MAX(position), 0) + 1 FROM folders))
           RETURNING id, name, position, created_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list_all(pool: &PgPool) -> AppResult<Vec<Folder>> {
    let rows = sqlx::query_as::<_, Folder>(
        r#"SELECT id, name, position, created_at
           FROM folders ORDER BY position, created_at"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_name(pool: &PgPool, id: FolderId, name: &str) -> AppResult<Folder> {
    sqlx::query_as::<_, Folder>(
        r#"UPDATE folders SET name = $2 WHERE id = $1
           RETURNING id, name, position, created_at"#,
    )
    .bind(id.0)
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn delete(pool: &PgPool, id: FolderId) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
