use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use super::domain::{Digest, DigestDate};
use super::service;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn latest(State(state): State<AppState>) -> AppResult<Json<Digest>> {
    Ok(Json(service::get_latest(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct DateQuery {
    pub date: String,
}

pub async fn by_date(
    State(state): State<AppState>,
    Query(q): Query<DateQuery>,
) -> AppResult<Json<Digest>> {
    let date = DigestDate::parse(q.date).map_err(AppError::Validation)?;
    Ok(Json(service::get_by_date(&state, date.date()).await?))
}

pub async fn refresh(State(state): State<AppState>) -> AppResult<Json<Digest>> {
    let today = chrono::Utc::now().date_naive();
    Ok(Json(service::generate_for_date(&state, today).await?))
}
