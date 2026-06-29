use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Feed, FeedId};
use super::service;
use crate::features::folders::domain::FolderId;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateFeed {
    pub url: String,
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateFeed>,
) -> AppResult<(StatusCode, Json<Feed>)> {
    let feed = service::create_feed(&state, &body.url).await?;
    Ok((StatusCode::CREATED, Json(feed)))
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Feed>>> {
    Ok(Json(service::list_feeds(&state).await?))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::delete_feed(&state, FeedId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Json<Feed>> {
    // 当該フィードのみ再取得し、更新後の Feed を返す。
    Ok(Json(service::refresh_one(&state, FeedId(id)).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateFeed {
    #[serde(default)]
    pub title: Option<String>,
    // 外側 None=キー無し(据え置き) / Some(None)=明示 null(未分類化) / Some(Some)=割当
    #[serde(default, deserialize_with = "double_option")]
    pub folder_id: Option<Option<Uuid>>,
}

// "キー無し" と "null" を区別するためのヘルパ（serde_with の double_option 相当）。
// キーが存在すれば（null でも値でも）呼ばれ、内側 Option を Some で包む。
// キーが無ければ #[serde(default)] が None を与え、本関数は呼ばれない。
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFeed>,
) -> AppResult<Json<Feed>> {
    let folder_id = body.folder_id.map(|inner| inner.map(FolderId));
    let feed = service::update_feed(&state, FeedId(id), body.title, folder_id).await?;
    Ok(Json(feed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_feed_omitted_folder_id_is_none() {
        let u: UpdateFeed = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert_eq!(u.folder_id, None);
    }

    #[test]
    fn update_feed_null_folder_id_is_some_none() {
        let u: UpdateFeed = serde_json::from_str(r#"{"folder_id":null}"#).unwrap();
        assert_eq!(u.folder_id, Some(None));
    }

    #[test]
    fn update_feed_value_folder_id_is_some_some() {
        let id = "00000000-0000-0000-0000-0000000000aa";
        let u: UpdateFeed = serde_json::from_str(&format!(r#"{{"folder_id":"{id}"}}"#)).unwrap();
        assert_eq!(u.folder_id, Some(Some(Uuid::parse_str(id).unwrap())));
    }
}
