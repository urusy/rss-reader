use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{InstapaperCredentials, InstapaperStatus};
use super::service;
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

pub async fn add_read_later(
    State(state): State<AppState>,
    Json(body): Json<ReadLaterBody>,
) -> AppResult<StatusCode> {
    service::add_to_read_later(&state, body.article_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
