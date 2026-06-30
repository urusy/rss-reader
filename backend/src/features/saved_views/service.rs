use super::domain::{QuerySpec, SavedView, SavedViewId, SavedViewName};
use super::repository;
use crate::features::articles::domain::Article;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn list_views(state: &AppState) -> AppResult<Vec<SavedView>> {
    repository::list(&state.db).await
}

pub async fn get_view(state: &AppState, id: SavedViewId) -> AppResult<SavedView> {
    repository::get(&state.db, id).await
}

pub async fn create_view(
    state: &AppState,
    name: SavedViewName,
    query: QuerySpec,
    position: i32,
) -> AppResult<SavedView> {
    let query = query.validate()?;
    repository::insert(&state.db, name.as_str(), &query, position).await
}

pub async fn update_view(
    state: &AppState,
    id: SavedViewId,
    name: SavedViewName,
    query: QuerySpec,
    position: i32,
) -> AppResult<SavedView> {
    let query = query.validate()?;
    repository::update(&state.db, id, name.as_str(), &query, position).await
}

pub async fn delete_view(state: &AppState, id: SavedViewId) -> AppResult<()> {
    if repository::delete(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn resolve_view(
    state: &AppState,
    id: SavedViewId,
    unread_override: Option<bool>,
) -> AppResult<Vec<Article>> {
    let view = repository::get(&state.db, id).await?;
    repository::resolve(&state.db, &view.query, unread_override).await
}
