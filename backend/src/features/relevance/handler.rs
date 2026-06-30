use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use super::domain::{ProfileView, RelevanceScore, ScoreResult};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list_scores(State(state): State<AppState>) -> AppResult<Json<Vec<RelevanceScore>>> {
    Ok(Json(service::list_scores(&state).await?))
}

pub async fn profile(State(state): State<AppState>) -> AppResult<Json<ProfileView>> {
    Ok(Json(service::profile_view(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct ScoreQuery {
    #[serde(default)]
    pub refresh: bool,
}

pub async fn score(
    State(state): State<AppState>,
    Query(q): Query<ScoreQuery>,
) -> AppResult<Json<ScoreResult>> {
    Ok(Json(service::score_unread(&state, q.refresh).await?))
}
