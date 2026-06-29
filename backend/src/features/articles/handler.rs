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
pub struct ReadAllBody {
    #[serde(default)]
    pub feed_id: Option<Uuid>, // 省略 or null = 全フィード
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    body: Option<Json<ReadAllBody>>,
) -> AppResult<StatusCode> {
    // body=None（ボディ無し or Content-Type が application/json でない）→ 全体既読。
    // body=Some(Json(b)) かつ b.feed_id=None（{} や {"feed_id":null}）→ 全体既読。
    let feed_id = body.and_then(|Json(b)| b.feed_id).map(FeedId);
    let _marked = service::mark_all_read(&state, feed_id).await?;
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

#[cfg(test)]
mod tests {
    use super::ReadAllBody;

    #[test]
    fn read_all_body_defaults_to_none_when_absent() {
        let a: ReadAllBody = serde_json::from_str("{}").unwrap();
        assert!(a.feed_id.is_none());
        let b: ReadAllBody = serde_json::from_str(r#"{"feed_id":null}"#).unwrap();
        assert!(b.feed_id.is_none());
    }

    #[test]
    fn read_all_body_parses_uuid() {
        let s = r#"{"feed_id":"00000000-0000-0000-0000-000000000001"}"#;
        let parsed: ReadAllBody = serde_json::from_str(s).unwrap();
        assert!(parsed.feed_id.is_some());
    }

    #[test]
    fn read_all_body_rejects_non_uuid() {
        assert!(serde_json::from_str::<ReadAllBody>(r#"{"feed_id":"abc"}"#).is_err());
    }
}
