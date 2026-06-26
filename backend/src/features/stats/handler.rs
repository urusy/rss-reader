use axum::extract::State;
use axum::Json;

use super::domain::Stats;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn get(State(state): State<AppState>) -> AppResult<Json<Stats>> {
    Ok(Json(service::get_stats(&state).await?))
}
