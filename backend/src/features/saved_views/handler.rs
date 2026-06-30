use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::domain::{QuerySpec, SavedView, SavedViewId, SavedViewName};
use super::service;
use crate::features::articles::domain::Article;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn list_views(State(state): State<AppState>) -> AppResult<Json<Vec<SavedView>>> {
    Ok(Json(service::list_views(&state).await?))
}

pub async fn get_view(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<Json<SavedView>> {
    Ok(Json(service::get_view(&state, SavedViewId(id)).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpsertBody {
    pub name: String,
    pub query: QuerySpec,
    #[serde(default)]
    pub position: i32,
}

pub async fn create_view(
    State(state): State<AppState>,
    Json(body): Json<UpsertBody>,
) -> AppResult<(StatusCode, Json<SavedView>)> {
    let name = SavedViewName::parse(body.name).map_err(AppError::Validation)?;
    let view = service::create_view(&state, name, body.query, body.position).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

pub async fn update_view(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(body): Json<UpsertBody>,
) -> AppResult<Json<SavedView>> {
    let name = SavedViewName::parse(body.name).map_err(AppError::Validation)?;
    Ok(Json(
        service::update_view(&state, SavedViewId(id), name, body.query, body.position).await?,
    ))
}

pub async fn delete_view(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<StatusCode> {
    service::delete_view(&state, SavedViewId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ResolveQuery {
    pub unread: Option<bool>,
}

pub async fn resolve_view(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Query(q): Query<ResolveQuery>,
) -> AppResult<Json<Vec<Article>>> {
    Ok(Json(
        service::resolve_view(&state, SavedViewId(id), q.unread).await?,
    ))
}
