use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use super::domain::{MuteRule, MuteRuleId, NewMuteRule, PatchMuteRule};
use super::service::{self, ApplyReport};
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<MuteRule>>> {
    Ok(Json(service::list_rules(&state).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewMuteRule>,
) -> AppResult<(StatusCode, Json<MuteRule>)> {
    let rule = service::create_rule(&state, body).await?;
    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchMuteRule>,
) -> AppResult<Json<MuteRule>> {
    Ok(Json(
        service::update_rule(&state, MuteRuleId(id), body).await?,
    ))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::delete_rule(&state, MuteRuleId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn apply(State(state): State<AppState>) -> AppResult<Json<ApplyReport>> {
    Ok(Json(service::apply_all(&state).await?))
}
