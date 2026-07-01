use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::service;
use crate::features::articles::domain::{Article, ArticleId};
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ExtractBody {
    /// Re-extract even if already cached. Defaults to false (cache wins).
    #[serde(default)]
    pub force: bool,
}

/// POST /api/articles/{id}/extract
/// Returns 200 + the updated Article for success, cache hit, and too-thin alike.
/// A NULL `full_content` in the response means "couldn't extract" — the client
/// (and AI features) fall back to `content`. No ANTHROPIC_API_KEY required.
pub async fn extract(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Option<Json<ExtractBody>>,
) -> AppResult<Json<Article>> {
    let force = body.map(|Json(b)| b.force).unwrap_or(false);
    let article = service::extract_article(&state, ArticleId(id), force).await?;
    Ok(Json(article))
}
