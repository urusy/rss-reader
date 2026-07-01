//! llm_settings HTTP ハンドラ: GET/PUT /api/settings/llm。

use axum::extract::State;
use axum::Json;

use super::domain::{LlmSettingsBody, LlmSettingsPatch, LlmSettingsView};
use super::service;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<LlmSettingsView>> {
    Ok(Json(service::get_view(&state).await?))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<LlmSettingsBody>,
) -> AppResult<Json<LlmSettingsView>> {
    let patch = LlmSettingsPatch::parse(body).map_err(AppError::Validation)?;
    Ok(Json(service::update(&state, patch).await?))
}
