//! Full-text search over articles. Read-only: builds a validated query and
//! delegates to the trigram-indexed repository lookup.

use super::domain::SearchQuery;
use super::repository;
use crate::features::articles::domain::Article;
use crate::shared::error::AppResult;
use crate::shared::state::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub async fn search_articles(
    state: &AppState,
    raw_query: &str,
    limit: Option<i64>,
) -> AppResult<Vec<Article>> {
    let query = SearchQuery::parse(raw_query)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    repository::search(&state.db, &query, limit).await
}
