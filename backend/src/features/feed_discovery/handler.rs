use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::domain::DiscoveredFeed;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub candidates: Vec<DiscoveredFeed>,
}

pub async fn discover(
    State(state): State<AppState>,
    Json(body): Json<DiscoverRequest>,
) -> AppResult<Json<DiscoverResponse>> {
    let candidates = service::discover(&state, &body.url).await?;
    Ok(Json(DiscoverResponse { candidates }))
}
