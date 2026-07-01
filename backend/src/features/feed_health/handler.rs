use axum::extract::State;
use axum::Json;

use super::domain::FeedHealth;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn health(State(state): State<AppState>) -> AppResult<Json<Vec<FeedHealth>>> {
    Ok(Json(service::list_health(&state).await?))
}
