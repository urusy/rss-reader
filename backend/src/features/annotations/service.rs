use uuid::Uuid;

use super::domain::{Highlight, HighlightPatch, NewHighlight};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

async fn ensure_article(state: &AppState, article_id: Uuid) -> AppResult<()> {
    if repository::article_exists(&state.db, article_id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

// ---- stars -----------------------------------------------------------------

pub async fn star(state: &AppState, article_id: Uuid) -> AppResult<()> {
    ensure_article(state, article_id).await?;
    repository::add_star(&state.db, article_id).await
}

pub async fn unstar(state: &AppState, article_id: Uuid) -> AppResult<()> {
    ensure_article(state, article_id).await?;
    repository::remove_star(&state.db, article_id).await
}

pub async fn list_starred(state: &AppState) -> AppResult<Vec<Uuid>> {
    repository::starred_ids(&state.db).await
}

// ---- highlights ------------------------------------------------------------

pub async fn list_highlights(state: &AppState, article_id: Uuid) -> AppResult<Vec<Highlight>> {
    ensure_article(state, article_id).await?;
    repository::list_highlights(&state.db, article_id).await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_highlight(
    state: &AppState,
    article_id: Uuid,
    quote: String,
    note: Option<String>,
    start_offset: Option<i32>,
    end_offset: Option<i32>,
    color: Option<String>,
) -> AppResult<Highlight> {
    ensure_article(state, article_id).await?;
    let new = NewHighlight::parse(quote, note, start_offset, end_offset, color)
        .map_err(AppError::Validation)?;
    repository::insert_highlight(&state.db, article_id, &new).await
}

pub async fn update_highlight(
    state: &AppState,
    id: Uuid,
    note: Option<String>,
    color: Option<String>,
) -> AppResult<Highlight> {
    let patch = HighlightPatch::parse(note, color);
    repository::update_highlight(&state.db, id, &patch)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn delete_highlight(state: &AppState, id: Uuid) -> AppResult<()> {
    if repository::delete_highlight(&state.db, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}
