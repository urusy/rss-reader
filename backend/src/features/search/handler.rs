use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use super::service;
use crate::features::articles::domain::Article;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// The search text. Trimmed/validated downstream; blank → 400.
    pub q: String,
    /// Optional result cap (clamped to 1..=200, default 50).
    pub limit: Option<i64>,
}

/// GET /api/search?q=<text>&limit=<n> — returns matching articles, ranked.
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> AppResult<Json<Vec<Article>>> {
    let hits = service::search_articles(&state, &params.q, params.limit).await?;
    Ok(Json(hits))
}
