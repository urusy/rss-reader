use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::Response;
use axum::Json;

use super::domain::{BackupRunRow, ImportSummary};
use super::service;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

/// Extract the presented backup token. Prefers `X-Backup-Token` so it doesn't
/// collide with `Authorization: Bearer` (used by the feature-14 auth middleware,
/// which guards these routes too). Falls back to Bearer when auth is disabled.
fn presented_token(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-backup-token").and_then(|h| h.to_str().ok()) {
        return Some(v.trim().to_string());
    }
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
}

pub async fn export(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    service::check_token(&state, presented_token(&headers).as_deref())?;
    let body: Body = service::export_ndjson(&state).await?;
    let resp = Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"rss-backup.ndjson\"",
        )
        .body(body)
        .expect("valid response");
    Ok(resp)
}

pub async fn import(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> AppResult<Json<ImportSummary>> {
    service::check_token(&state, presented_token(&headers).as_deref())?;
    let summary = service::import_ndjson(&state, &body).await?;
    Ok(Json(summary))
}

pub async fn runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<BackupRunRow>>> {
    service::check_token(&state, presented_token(&headers).as_deref())?;
    Ok(Json(service::list_runs(&state).await?))
}
