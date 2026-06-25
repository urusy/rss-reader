use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::{Feed, FeedId};
use super::service;
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

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete_feed(&state, FeedId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh(
    State(state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    // Simple version refreshes everything; per-feed refresh is a future refinement.
    service::refresh_all_feeds(&state).await?;
    Ok(StatusCode::ACCEPTED)
}
