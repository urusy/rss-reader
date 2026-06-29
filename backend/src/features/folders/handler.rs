use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Folder, FolderId};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateFolder {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFolder {
    pub name: String,
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Folder>>> {
    Ok(Json(service::list_folders(&state).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateFolder>,
) -> AppResult<(StatusCode, Json<Folder>)> {
    let folder = service::create_folder(&state, &body.name).await?;
    Ok((StatusCode::CREATED, Json(folder))) // 201（feeds::create 前例）
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFolder>,
) -> AppResult<Json<Folder>> {
    let folder = service::rename_folder(&state, FolderId(id), &body.name).await?;
    Ok(Json(folder)) // 更新後エンティティを返す
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::delete_folder(&state, FolderId(id)).await?;
    Ok(StatusCode::NO_CONTENT) // 204
}
