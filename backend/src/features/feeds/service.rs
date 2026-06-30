//! Use cases for the feeds slice: create/list/delete and the crawl loop.

use feed_rs::parser;

use super::domain::{Feed, FeedId, FeedUrl};
use super::repository;
use crate::features::articles;
use crate::features::folders::domain::FolderId;
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

pub async fn update_feed(
    state: &AppState,
    id: FeedId,
    title: Option<String>,
    folder_id: Option<Option<FolderId>>,
) -> AppResult<Feed> {
    if let Some(t) = &title {
        if t.trim().is_empty() {
            return Err(AppError::Validation("title must not be empty".into()));
        }
    }
    // 実在しないフォルダへの割当は 400 に整形（FK 違反の 500 を避ける advisory）。
    if let Some(Some(fid)) = folder_id {
        if !repository::folder_exists(&state.db, fid).await? {
            return Err(AppError::Validation("folder not found".into()));
        }
    }
    repository::update(&state.db, id, title.as_deref(), folder_id).await
}

/// 単一フィードのみ再取得（従来の全件 refresh ではなく当該フィードだけ）。
pub async fn refresh_one(state: &AppState, id: FeedId) -> AppResult<Feed> {
    let feed = repository::get(&state.db, id).await?; // 無ければ NotFound
    fetch_and_store(state, &feed).await?; // 既存ロジックを再利用
    repository::get(&state.db, id).await // 更新後（last_fetched_at 反映）を返す
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

    let parsed =
        parser::parse(&bytes[..]).map_err(|e| AppError::Upstream(format!("parse error: {e}")))?;

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

        // Optional crawl-time full-content extraction (EXTRACT_ON_CRAWL=true).
        // Best-effort + idempotent (skips already-extracted rows). Default off,
        // so behavior is unchanged unless explicitly opted in.
        if state.config.extract_on_crawl {
            if let Some(id) = articles::repository::id_by_url(&state.db, &url).await? {
                crate::features::extraction::service::extract_best_effort(state, id).await;
            }
        }
    }

    repository::touch_fetched(&state.db, feed.id, feed_title.as_deref()).await?;
    Ok(())
}
