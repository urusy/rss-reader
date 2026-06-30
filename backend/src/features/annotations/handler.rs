use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::domain::Highlight;
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

// ---- stars -----------------------------------------------------------------

pub async fn add_star(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::star(&s, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_star(State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    service::unstar(&s, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_stars(State(s): State<AppState>) -> AppResult<Json<Vec<Uuid>>> {
    Ok(Json(service::list_starred(&s).await?))
}

// ---- highlights ------------------------------------------------------------

pub async fn list_highlights(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<Highlight>>> {
    Ok(Json(service::list_highlights(&s, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct NewHighlightBody {
    pub quote: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub start_offset: Option<i32>,
    #[serde(default)]
    pub end_offset: Option<i32>,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn create_highlight(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(b): Json<NewHighlightBody>,
) -> AppResult<(StatusCode, Json<Highlight>)> {
    let h = service::create_highlight(
        &s,
        id,
        b.quote,
        b.note,
        b.start_offset,
        b.end_offset,
        b.color,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(h)))
}

#[derive(Debug, Deserialize)]
pub struct PatchHighlightBody {
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn patch_highlight(
    State(s): State<AppState>,
    Path(hid): Path<Uuid>,
    Json(b): Json<PatchHighlightBody>,
) -> AppResult<Json<Highlight>> {
    Ok(Json(
        service::update_highlight(&s, hid, b.note, b.color).await?,
    ))
}

pub async fn delete_highlight(
    State(s): State<AppState>,
    Path(hid): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete_highlight(&s, hid).await?;
    Ok(StatusCode::NO_CONTENT)
}
