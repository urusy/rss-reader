use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::Json;

use super::domain::ImportSummary;
use super::service;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

/// POST /api/opml/import — body is OPML XML (Content-Type ignored; Bytes + UTF-8).
pub async fn import(State(state): State<AppState>, body: Bytes) -> AppResult<Json<ImportSummary>> {
    let xml = std::str::from_utf8(&body)
        .map_err(|_| AppError::Validation("OPML body must be valid UTF-8".into()))?;
    if xml.trim().is_empty() {
        return Err(AppError::Validation("OPML body is empty".into()));
    }
    Ok(Json(service::import_opml(&state, xml).await?))
}

/// GET /api/opml/export — download OPML XML.
pub async fn export(State(state): State<AppState>) -> AppResult<(HeaderMap, String)> {
    let xml = service::export_opml(&state).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/x-opml; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"feeds.opml\""),
    );
    Ok((headers, xml))
}
