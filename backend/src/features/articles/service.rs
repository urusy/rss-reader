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
    include_muted: bool,
) -> AppResult<Vec<Article>> {
    repository::list(
        &state.db,
        feed_id,
        unread_only,
        folder_id,
        unclassified,
        include_muted,
    )
    .await
}

pub async fn get_article(state: &AppState, id: ArticleId) -> AppResult<Article> {
    repository::get(&state.db, id).await
}

pub async fn mark_read(state: &AppState, id: ArticleId, read: bool) -> AppResult<()> {
    repository::set_read(&state.db, id, read).await
}

pub async fn mark_all_read(state: &AppState, feed_id: Option<FeedId>) -> AppResult<u64> {
    repository::mark_all_read(&state.db, feed_id).await
}

/// Discard a cached summary / translation. Lets the user drop a stale result
/// (e.g. one generated before HTML was flattened out of the LLM input).
pub async fn clear_summary(state: &AppState, id: ArticleId) -> AppResult<()> {
    repository::clear_summary(&state.db, id).await
}

pub async fn clear_translation(state: &AppState, id: ArticleId) -> AppResult<()> {
    repository::clear_translation(&state.db, id).await
}

/// Pick the body to feed the LLM: the extracted full body when present,
/// otherwise the feed-provided excerpt, then flatten HTML to plain text.
/// Feeding raw HTML made the model echo `<style>`/inline-style markup into the
/// summary/translation (rendered literally in the text-node view), and wasted
/// input tokens. We reuse the extraction slice's pure `html_to_plain_text`
/// (no new abstraction; same cross-slice function call as `llm_settings`).
fn ai_input(article: &Article) -> String {
    let raw = article
        .full_content
        .clone()
        .unwrap_or_else(|| article.content.clone());
    crate::features::extraction::domain::html_to_plain_text(&raw)
}

/// Build an Anthropic client for a specific model, or fail with a clear
/// "not enabled" error if no API key is set yet. The model is resolved per
/// operation by the `llm_settings` slice (summarize vs translate can differ).
fn llm_client(state: &AppState, model: &str) -> AppResult<AnthropicClient> {
    let key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or_else(|| AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into()))?;
    Ok(AnthropicClient::new(
        state.http.clone(),
        key,
        model.to_string(),
    ))
}

pub async fn summarize_article(
    state: &AppState,
    id: ArticleId,
    target_lang: &str,
    force: bool,
) -> AppResult<Article> {
    let article = repository::get(&state.db, id).await?;

    // Cache hit: same language already summarized. `force` bypasses it so a new
    // model/prompt (settings #llm_settings) can regenerate an existing summary.
    if !force && article.summary.is_some() && article.summary_lang.as_deref() == Some(target_lang) {
        return Ok(article);
    }

    // 要約用のモデル・プロンプト override を解決（設定画面 #llm_settings）。
    let (model, prompt) = crate::features::llm_settings::service::resolve_summarize(state).await?;
    let client = llm_client(state, &model)?;
    let summary = client
        .summarize(SummarizeRequest {
            title: article.title.clone(),
            content: ai_input(&article),
            target_lang: target_lang.to_string(),
            system_prompt: prompt,
        })
        .await?;

    // 出力ガード: モデルが自発的に HTML を吐いても保存前に決定的に除去する
    // （text-node 描画へ literal タグが漏れるのを根絶）。平文には概ね冪等。
    let summary = crate::features::extraction::domain::html_to_plain_text(&summary);
    repository::save_summary(&state.db, id, &summary, target_lang).await?;
    repository::get(&state.db, id).await
}

pub async fn translate_article(
    state: &AppState,
    id: ArticleId,
    target_lang: &str,
    force: bool,
) -> AppResult<Article> {
    let article = repository::get(&state.db, id).await?;

    // `force` bypasses the cache so a changed model/prompt can re-translate.
    if !force
        && article.translation.is_some()
        && article.translation_lang.as_deref() == Some(target_lang)
    {
        return Ok(article);
    }

    // 翻訳用のモデル・プロンプト override を解決（設定画面 #llm_settings）。
    let (model, prompt) = crate::features::llm_settings::service::resolve_translate(state).await?;
    let client = llm_client(state, &model)?;
    let translation = client
        .translate(TranslateRequest {
            content: ai_input(&article),
            target_lang: target_lang.to_string(),
            system_prompt: prompt,
        })
        .await?;

    // 出力ガード（summarize と同様）: literal HTML の漏れを保存前に除去。
    let translation = crate::features::extraction::domain::html_to_plain_text(&translation);
    repository::save_translation(&state.db, id, &translation, target_lang).await?;
    repository::get(&state.db, id).await
}
