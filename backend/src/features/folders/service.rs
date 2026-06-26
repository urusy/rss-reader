use super::domain::{Folder, FolderId, FolderName};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn create_folder(state: &AppState, raw_name: &str) -> AppResult<Folder> {
    let name = FolderName::parse(raw_name).map_err(AppError::Validation)?;
    repository::insert(&state.db, name.as_str()).await
}

pub async fn list_folders(state: &AppState) -> AppResult<Vec<Folder>> {
    repository::list_all(&state.db).await
}

pub async fn rename_folder(state: &AppState, id: FolderId, raw_name: &str) -> AppResult<Folder> {
    let name = FolderName::parse(raw_name).map_err(AppError::Validation)?;
    repository::update_name(&state.db, id, name.as_str()).await // None -> NotFound は repo 側
}

pub async fn delete_folder(state: &AppState, id: FolderId) -> AppResult<()> {
    if repository::delete(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(()) // 配下フィードは ON DELETE SET NULL で未分類へ
}
