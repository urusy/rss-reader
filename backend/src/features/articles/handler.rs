use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Article, ArticleId};
use super::service;
use crate::features::feeds::domain::FeedId;
use crate::features::folders::domain::FolderId;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub feed_id: Option<Uuid>,
    #[serde(default)]
    pub unread: bool,
    pub folder_id: Option<Uuid>,
    #[serde(default)]
    pub unclassified: bool,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Article>>> {
    let articles = service::list_articles(
        &state,
        q.feed_id.map(FeedId),
        q.unread,
        q.folder_id.map(FolderId),
        q.unclassified,
    )
    .await?;
    Ok(Json(articles))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Article>> {
    Ok(Json(service::get_article(&state, ArticleId(id)).await?))
}

#[derive(Debug, Deserialize)]
pub struct ReadBody {
    #[serde(default = "default_true")]
    pub read: bool,
}
fn default_true() -> bool {
    true
}

pub async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReadBody>,
) -> AppResult<StatusCode> {
    service::mark_read(&state, ArticleId(id), body.read).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct LangBody {
    #[serde(default = "default_lang")]
    pub lang: String,
}
fn default_lang() -> String {
    "ja".to_string()
}

pub async fn summarize(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LangBody>,
) -> AppResult<Json<Article>> {
    let article = service::summarize_article(&state, ArticleId(id), &body.lang).await?;
    Ok(Json(article))
}

pub async fn translate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LangBody>,
) -> AppResult<Json<Article>> {
    let article = service::translate_article(&state, ArticleId(id), &body.lang).await?;
    Ok(Json(article))
}
