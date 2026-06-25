//! Use cases for the feeds slice: create/list/delete and the crawl loop.

use feed_rs::parser;

use super::domain::{Feed, FeedId, FeedUrl};
use super::repository;
use crate::features::articles;
use crate::shared::error::{AppError, AppResult};
use crate::shared::state::AppState;

pub async fn create_feed(state: &AppState, raw_url: &str) -> AppResult<Feed> {
    let url = FeedUrl::parse(raw_url).map_err(AppError::Validation)?;
    let feed = repository::insert(&state.db, url.as_str()).await?;
    // Best-effort immediate fetch so the user sees articles right away.
    if let Err(e) = fetch_and_store(state, &feed).await {
        tracing::warn!(error = %e, feed = %feed.url, "initial fetch failed");
    }
    Ok(feed)
}

pub async fn list_feeds(state: &AppState) -> AppResult<Vec<Feed>> {
    repository::list_all(&state.db).await
}

pub async fn delete_feed(state: &AppState, id: FeedId) -> AppResult<()> {
    let affected = repository::delete(&state.db, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn refresh_all_feeds(state: &AppState) -> AppResult<()> {
    let feeds = repository::list_all(&state.db).await?;
    tracing::info!(count = feeds.len(), "refreshing feeds");
    for feed in feeds {
        if let Err(e) = fetch_and_store(state, &feed).await {
            tracing::error!(error = %e, feed = %feed.url, "fetch failed");
        }
    }
    Ok(())
}

/// Fetch one feed over HTTP, parse it, and upsert its entries as articles.
pub async fn fetch_and_store(state: &AppState, feed: &Feed) -> AppResult<()> {
    let bytes = state
        .http
        .get(&feed.url)
        .send()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Upstream(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let parsed = parser::parse(&bytes[..])
        .map_err(|e| AppError::Upstream(format!("parse error: {e}")))?;

    let feed_title = parsed.title.as_ref().map(|t| t.content.clone());

    for entry in parsed.entries {
        let url = entry
            .links
            .first()
            .map(|l| l.href.clone())
            .unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| "(untitled)".to_string());
        let content = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            .unwrap_or_default();
        let published = entry.published.or(entry.updated);

        articles::repository::upsert(
            &state.db,
            FeedId(feed.id.0),
            &url,
            &title,
            &content,
            published,
        )
        .await?;
    }

    repository::touch_fetched(&state.db, feed.id, feed_title.as_deref()).await?;
    Ok(())
}
