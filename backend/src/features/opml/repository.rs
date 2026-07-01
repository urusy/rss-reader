//! OPML only needs one read projection of its own (folder name -> id, for
//! idempotent import dedupe). Writes go through the existing feeds/folders repos.

use sqlx::PgPool;

use crate::features::folders::domain::FolderId;
use crate::shared::error::AppResult;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FolderNameRow {
    pub id: FolderId,
    pub name: String,
}

pub async fn list_folder_names(pool: &PgPool) -> AppResult<Vec<FolderNameRow>> {
    let rows = sqlx::query_as::<_, FolderNameRow>(
        "SELECT id, name FROM folders ORDER BY position, created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
