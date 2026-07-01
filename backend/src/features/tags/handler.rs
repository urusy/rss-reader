use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::domain::{ArticleTag, RawSuggestion, Tag, TagId, TagName, TagWithCount};
use super::service;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn list_tags(State(state): State<AppState>) -> AppResult<Json<Vec<TagWithCount>>> {
    Ok(Json(service::list_tags(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct CreateTagBody {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn create_tag(
    State(state): State<AppState>,
    Json(body): Json<CreateTagBody>,
) -> AppResult<(StatusCode, Json<Tag>)> {
    let name = TagName::parse(body.name).map_err(AppError::Validation)?;
    let tag = service::create_tag(&state, name, body.color).await?;
    Ok((StatusCode::CREATED, Json(tag)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateTagBody {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn update_tag(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(body): Json<UpdateTagBody>,
) -> AppResult<Json<Tag>> {
    let name = TagName::parse(body.name).map_err(AppError::Validation)?;
    Ok(Json(
        service::update_tag(&state, TagId(id), name, body.color).await?,
    ))
}

pub async fn delete_tag(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<StatusCode> {
    service::delete_tag(&state, TagId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_article_tags(
    State(state): State<AppState>,
    Path(article_id): Path<uuid::Uuid>,
) -> AppResult<Json<Vec<ArticleTag>>> {
    Ok(Json(
        service::list_article_tags(&state, ArticleId(article_id)).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SetTagsBody {
    pub tag_ids: Vec<uuid::Uuid>,
}

pub async fn set_article_tags(
    State(state): State<AppState>,
    Path(article_id): Path<uuid::Uuid>,
    Json(body): Json<SetTagsBody>,
) -> AppResult<Json<Vec<ArticleTag>>> {
    let ids: Vec<TagId> = body.tag_ids.into_iter().map(TagId).collect();
    Ok(Json(
        service::set_article_tags(&state, ArticleId(article_id), &ids).await?,
    ))
}

pub async fn detach_tag(
    State(state): State<AppState>,
    Path((article_id, tag_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> AppResult<StatusCode> {
    service::detach_tag(&state, ArticleId(article_id), TagId(tag_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SuggestQuery {
    #[serde(default)]
    pub refresh: bool,
}

pub async fn suggest_tags(
    State(state): State<AppState>,
    Path(article_id): Path<uuid::Uuid>,
    Query(q): Query<SuggestQuery>,
) -> AppResult<Json<Vec<RawSuggestion>>> {
    Ok(Json(
        service::suggest_tags(&state, ArticleId(article_id), q.refresh).await?,
    ))
}
