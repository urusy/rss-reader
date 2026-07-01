use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{InstapaperCredentials, InstapaperStatus, ReadLaterItem, ReadLaterSettings};
use super::service;
use crate::features::articles::domain::ArticleId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CredentialsBody {
    pub username: String,
    pub password: String,
}

pub async fn save_credentials(
    State(state): State<AppState>,
    Json(body): Json<CredentialsBody>,
) -> AppResult<Json<InstapaperStatus>> {
    let creds =
        InstapaperCredentials::parse(body.username, body.password).map_err(AppError::Validation)?;
    service::save_credentials(&state, creds).await?;
    Ok(Json(InstapaperStatus { configured: true }))
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<InstapaperStatus>> {
    Ok(Json(service::get_status(&state).await?))
}

pub async fn delete_credentials(State(state): State<AppState>) -> AppResult<StatusCode> {
    service::clear_credentials(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ReadLaterBody {
    pub article_id: Uuid,
}

pub async fn save_for_later(
    State(state): State<AppState>,
    Json(body): Json<ReadLaterBody>,
) -> AppResult<Json<ReadLaterItem>> {
    let item = service::save_for_later(&state, ArticleId(body.article_id)).await?;
    Ok(Json(item))
}

pub async fn get_read_later_one(
    State(state): State<AppState>,
    Path(article_id): Path<Uuid>,
) -> AppResult<Json<ReadLaterItem>> {
    service::get_read_later(&state, ArticleId(article_id))
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

pub async fn list_read_later(State(state): State<AppState>) -> AppResult<Json<Vec<ReadLaterItem>>> {
    Ok(Json(service::list_read_later(&state).await?))
}

// ---- 機能16 Read-on-Save 設定 ----

pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<ReadLaterSettings>> {
    Ok(Json(service::get_read_later_settings(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct SettingsBody {
    pub mark_read_on_save: bool,
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsBody>,
) -> AppResult<Json<ReadLaterSettings>> {
    Ok(Json(
        service::update_read_later_settings(&state, body.mark_read_on_save).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::SettingsBody;

    #[test]
    fn settings_body_parses_bool() {
        let b: SettingsBody = serde_json::from_str(r#"{"mark_read_on_save":true}"#).unwrap();
        assert!(b.mark_read_on_save);
    }

    #[test]
    fn settings_body_rejects_missing() {
        assert!(serde_json::from_str::<SettingsBody>("{}").is_err());
    }

    #[test]
    fn settings_body_rejects_non_bool() {
        assert!(serde_json::from_str::<SettingsBody>(r#"{"mark_read_on_save":"yes"}"#).is_err());
    }
}
