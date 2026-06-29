use axum::extract::State;
use axum::Json;

use super::domain::FeedOverview;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn overview(State(state): State<AppState>) -> AppResult<Json<Vec<FeedOverview>>> {
    Ok(Json(service::list_overview(&state).await?))
}
