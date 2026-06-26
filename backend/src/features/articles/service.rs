//! Use cases for the articles slice, including on-demand LLM summarize/translate
//! with DB caching: if a result for the requested language already exists, we
//! return it without spending tokens.

use super::domain::{Article, ArticleId};
use super::repository;
use crate::features::feeds::domain::FeedId;
use crate::features::folders::domain::FolderId;
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{LlmClient, SummarizeRequest, TranslateRequest};
use crate::shared::state::AppState;

pub async fn list_articles(
    state: &AppState,
    feed_id: Option<FeedId>,
    unread_only: bool,
    folder_id: Option<FolderId>,
    unclassified: bool,
) -> AppResult<Vec<Article>> {
    repository::list(&state.db, feed_id, unread_only, folder_id, unclassified).await
}

pub async fn get_article(state: &AppState, id: ArticleId) -> AppResult<Article> {
    repository::get(&state.db, id).await
}

pub async fn mark_read(state: &AppState, id: ArticleId, read: bool) -> AppResult<()> {
    repository::set_read(&state.db, id, read).await
}

/// Build an Anthropic client from config, or fail with a clear "not enabled"
/// error if no API key is set yet.
fn llm_client(state: &AppState) -> AppResult<AnthropicClient> {
    let key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or_else(|| AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into()))?;
    Ok(AnthropicClient::new(
        state.http.clone(),
        key,
        state.config.anthropic_model.clone(),
    ))
}

pub async fn summarize_article(
    state: &AppState,
    id: ArticleId,
    target_lang: &str,
) -> AppResult<Article> {
    let article = repository::get(&state.db, id).await?;

    // Cache hit: same language already summarized.
    if article.summary.is_some() && article.summary_lang.as_deref() == Some(target_lang) {
        return Ok(article);
    }

    let client = llm_client(state)?;
    let summary = client
        .summarize(SummarizeRequest {
            title: article.title.clone(),
            content: article.content.clone(),
            target_lang: target_lang.to_string(),
        })
        .await?;

    repository::save_summary(&state.db, id, &summary, target_lang).await?;
    repository::get(&state.db, id).await
}

pub async fn translate_article(
    state: &AppState,
    id: ArticleId,
    target_lang: &str,
) -> AppResult<Article> {
    let article = repository::get(&state.db, id).await?;

    if article.translation.is_some() && article.translation_lang.as_deref() == Some(target_lang) {
        return Ok(article);
    }

    let client = llm_client(state)?;
    let translation = client
        .translate(TranslateRequest {
            content: article.content.clone(),
            target_lang: target_lang.to_string(),
        })
        .await?;

    repository::save_translation(&state.db, id, &translation, target_lang).await?;
    repository::get(&state.db, id).await
}
