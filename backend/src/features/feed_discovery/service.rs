use std::time::Duration;

use feed_rs::parser;

use super::domain::{
    extract_feed_links, feed_kind_from_type, is_feed_content_type, DiscoverUrl, DiscoveredFeed,
};
use super::repository;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Fetch the input URL and return feed candidates (no writes).
pub async fn discover(state: &AppState, raw_url: &str) -> AppResult<Vec<DiscoveredFeed>> {
    let input = DiscoverUrl::parse(raw_url).map_err(AppError::Validation)?;

    let resp = state
        .http
        .get(input.as_str())
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let base = resp.url().clone(); // final URL after redirects
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = resp
        .bytes()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;
    let body = if body.len() > MAX_BODY_BYTES {
        body.slice(0..MAX_BODY_BYTES)
    } else {
        body
    };

    let mut candidates = if is_feed_content_type(&content_type) {
        let title = parser::parse(&body[..])
            .ok()
            .and_then(|f| f.title.map(|t| t.content));
        vec![DiscoveredFeed {
            url: base.to_string(),
            title,
            kind: feed_kind_from_type(&content_type),
            already_subscribed: false,
        }]
    } else {
        let html = String::from_utf8_lossy(&body);
        extract_feed_links(&html, &base)
    };

    let existing = repository::existing_feed_urls(&state.db).await?;
    for c in candidates.iter_mut() {
        c.already_subscribed = existing.contains(&c.url);
    }

    Ok(candidates)
}
