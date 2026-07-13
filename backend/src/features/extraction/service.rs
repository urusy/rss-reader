//! Extraction use case: fetch the article's source URL, run the pure
//! domain extraction, and cache a successful body via `articles::repository`.
//! HTTP lives here (no trait/dyn — the only abstraction boundary is shared/llm).

use super::domain::{extract_main_content, Extracted, FetchUrl};
use crate::features::articles::domain::{Article, ArticleId};
use crate::features::articles::repository as articles_repo;
use crate::shared::error::{AppError, AppResult};
use crate::shared::fetch::{read_body_limited, safe_get, UrlGuard};
use crate::shared::state::AppState;

/// On-demand extraction, returning the (possibly updated) article:
///   1) load article (NotFound if absent)
///   2) cache hit (extracted_at set & !force) → return as-is, no refetch
///   3) fetch URL → extract → sanitize → min-length check
///   4) Ok → save_full_content; TooThin → leave NULL (falls back to content)
pub async fn extract_article(state: &AppState, id: ArticleId, force: bool) -> AppResult<Article> {
    let article = articles_repo::get(&state.db, id).await?;

    if !force && article.extracted_at.is_some() {
        return Ok(article);
    }

    let url = FetchUrl::parse(article.url.clone()).map_err(AppError::Validation)?;
    let html = fetch_html(state, &url).await?;

    let is_saved_page = article.feed_id.0 == crate::features::saved::domain::SAVED_FEED_ID;

    match extract_main_content(&html, state.config.extract_min_chars) {
        Extracted::Ok(content) => {
            if is_saved_page {
                // 保存ページ（Pocket 風「後で読む」）: content が正典（pg_trgm 検索
                // 索引・LLM 入力・digest snippet は content を読む）なので本文を
                // content と full_content の両方へ書き、タイトルも確定させる。
                // RSS 記事の content は絶対に書かない — クロールの upsert が
                // フィード由来 content で上書きし返し、無限に揺れるため。
                let title = crate::features::saved::domain::extract_page_title(&html);
                crate::features::saved::repository::save_extracted(
                    &state.db,
                    id,
                    title.as_deref(),
                    &content,
                )
                .await?;
            } else {
                articles_repo::save_full_content(&state.db, id, &content).await?;
            }
            articles_repo::get(&state.db, id).await
        }
        // Too thin: leave full_content NULL so display/AI fall back to content.
        // 保存ページはタイトルだけでも反映しておく（extracted_at は立てず、
        // 再保存・force 抽出での再試行を生かす）。
        Extracted::TooThin => {
            if is_saved_page {
                if let Some(title) = crate::features::saved::domain::extract_page_title(&html) {
                    crate::features::saved::repository::save_title(&state.db, id, &title).await?;
                    return articles_repo::get(&state.db, id).await;
                }
            }
            Ok(article)
        }
    }
}

/// Crawl-time auto extraction (best-effort). Swallows errors like the initial
/// fetch in `feeds::create_feed`. Idempotent: `extract_article` skips already
/// extracted rows (force=false).
pub async fn extract_best_effort(state: &AppState, id: ArticleId) {
    if let Err(e) = extract_article(state, id, false).await {
        tracing::warn!(error = %e, article = %id.0, "auto extraction failed");
    }
}

/// Fetch the article URL and return its HTML text. Defends size and content-type.
/// SSRF-guarded (`safe_get`) and size-capped mid-stream (`read_body_limited`),
/// so neither a redirect to an internal address nor a huge body gets through.
async fn fetch_html(state: &AppState, url: &FetchUrl) -> AppResult<String> {
    let guard = UrlGuard::from_config(&state.config);
    let resp = safe_get(&state.http_external, &guard, url.as_str(), |rb| {
        rb.header("accept", "text/html,application/xhtml+xml")
    })
    .await?;

    if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
        let ct = ct.to_str().unwrap_or_default();
        if !ct.contains("html") {
            return Err(AppError::Validation(format!("not an HTML page: {ct}")));
        }
    }

    let bytes = read_body_limited(resp, state.config.extract_max_bytes).await?;
    // MVP: assume UTF-8 (lossy). Non-UTF-8 charset handling is a future task.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
