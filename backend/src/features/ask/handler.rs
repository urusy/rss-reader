use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::domain::AskMessage;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct AskBody {
    pub messages: Vec<AskMessage>,
    #[serde(default)]
    pub save: bool,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    pub answer: String,
}

pub async fn ask_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AskBody>,
) -> AppResult<Json<AskResponse>> {
    let answer = service::ask_article(&state, id, body.messages, body.save).await?;
    Ok(Json(AskResponse { answer }))
}

#[derive(Debug, Deserialize)]
pub struct AskMultiBody {
    pub ids: Vec<Uuid>,
    pub messages: Vec<AskMessage>,
}

pub async fn ask_many(
    State(state): State<AppState>,
    Json(body): Json<AskMultiBody>,
) -> AppResult<Json<AskResponse>> {
    let answer = service::ask_articles(&state, body.ids, body.messages).await?;
    Ok(Json(AskResponse { answer }))
}

#[derive(Debug, Serialize)]
pub struct NotesResponse {
    pub messages: Vec<AskMessage>,
}

pub async fn get_notes(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<NotesResponse>> {
    let messages = service::get_notes(&state, id).await?;
    Ok(Json(NotesResponse { messages }))
}
